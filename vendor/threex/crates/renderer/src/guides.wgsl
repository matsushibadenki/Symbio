struct View {
    vp: mat4x4<f32>,
    inverse_vp: mat4x4<f32>,
    model: mat4x4<f32>,
    eye_distance: vec4<f32>,
    region: vec4<f32>,
    options: vec4<f32>, // grid, axes, physical scale, grid step
    gizmo_origin: vec4<f32>,
    gizmo_x: vec4<f32>,
    gizmo_y: vec4<f32>,
    gizmo_z: vec4<f32>,
}
@group(0) @binding(0) var<uniform> view: View;
@group(1) @binding(0) var mask: texture_2d<f32>;

@vertex fn fullscreen(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = array<vec2<f32>,3>(vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.));
    return vec4(p[i],0.,1.);
}
struct GridOut { @location(0) color: vec4<f32>, @builtin(frag_depth) depth: f32 }
fn grid_line(p: vec2<f32>, step: f32) -> f32 {
    let q = p / step;
    let width = max(fwidth(q), vec2(1e-5));
    let d = abs(fract(q - 0.5) - 0.5) / width;
    let coverage = (vec2(1.)-min(d,vec2(1.))) * (vec2(1.)-smoothstep(vec2(0.4),vec2(1.),width));
    return max(coverage.x,coverage.y);
}
@fragment fn grid(@builtin(position) position: vec4<f32>) -> GridOut {
    let uv = (position.xy-view.region.xy)/view.region.zw;
    let ndc = vec2(uv.x*2.-1.,1.-uv.y*2.);
    let near_h = view.inverse_vp * vec4(ndc,0.,1.);
    let mid_h = view.inverse_vp * vec4(ndc,0.5,1.);
    let origin = near_h.xyz/near_h.w;
    let ray = mid_h.xyz/mid_h.w-origin;
    // Derivatives are evaluated uniformly before any discard.
    let safe_y = select(-max(abs(ray.y),1e-8),max(abs(ray.y),1e-8),ray.y>=0.);
    let t = -origin.y/safe_y;
    let world = origin+ray*t;
    let minor = grid_line(world.xz,view.options.w);
    let major = grid_line(world.xz,view.options.w*10.);
    let clip = view.vp*vec4(world,1.);
    let depth = clip.z/clip.w;
    let distance = length(world-view.eye_distance.xyz);
    let fade = 1.-smoothstep(view.eye_distance.w*5.,view.eye_distance.w*18.,distance);
    let grazing = smoothstep(0.01,0.08,abs(normalize(ray).y));
    var alpha = (minor*0.20+major*0.20)*fade*grazing;
    var color = vec3(0.32,0.30,0.27);
    let world_width = max(fwidth(world.xz),vec2(1e-5));
    let x_axis = 1.-smoothstep(0.7,1.7,abs(world.z)/world_width.y);
    let z_axis = 1.-smoothstep(0.7,1.7,abs(world.x)/world_width.x);
    if view.options.y>0.5 {
        color = mix(color,vec3(0.58,0.18,0.12),x_axis);
        color = mix(color,vec3(0.18,0.31,0.48),z_axis);
        alpha = max(alpha,max(x_axis,z_axis)*0.65*fade*grazing);
    }
    if view.options.x<0.5 { alpha = max(x_axis,z_axis)*0.65*fade*grazing*view.options.y; }
    if abs(ray.y)<1e-8 || t<0. || depth<0. || depth>1. || clip.w<=0. || alpha<0.005 { discard; }
    var out: GridOut; out.color=vec4(color,alpha);out.depth=depth;return out;
}
struct AxisOut { @builtin(position) position:vec4<f32>, @location(0) color:vec3<f32> }
@vertex fn axes(@builtin(vertex_index) vertex:u32,@builtin(instance_index) axis:u32)->AxisOut {
    let corners=array<vec2<f32>,6>(vec2(0.,-1.),vec2(1.,-1.),vec2(1.,1.),vec2(0.,-1.),vec2(1.,1.),vec2(0.,1.));
    let directions=array<vec3<f32>,3>(vec3(1.,0.,0.),vec3(0.,1.,0.),vec3(0.,0.,1.));
    let colors=array<vec3<f32>,3>(vec3(0.65,0.20,0.13),vec3(0.20,0.48,0.26),vec3(0.18,0.34,0.60));
    let is_gizmo=axis>=3u;let index=axis%3u;
    let basis=array<vec3<f32>,3>(view.gizmo_x.xyz,view.gizmo_y.xyz,view.gizmo_z.xyz);
    let origin=select(vec3(0.),view.gizmo_origin.xyz,is_gizmo);
    let extent=select(max(view.options.w*3.,3.),view.gizmo_x.w,is_gizmo);
    var a=view.vp*vec4(origin,1.);
    var b=view.vp*vec4(origin+select(directions[index],basis[index],is_gizmo)*extent,1.);
    var out:AxisOut;out.color=colors[index];
    if is_gizmo {out.color=mix(out.color,vec3(1.,0.72,0.25),select(0.,0.75,i32(index)==i32(view.gizmo_y.w)));}
    if (a.z<0. && b.z<0.) || (a.w<=0. && b.w<=0.) {out.position=vec4(2.,2.,1.,1.);return out;}
    if a.z<0. {a=mix(a,b,(-a.z)/(b.z-a.z));}
    if b.z<0. {b=mix(b,a,(-b.z)/(a.z-b.z));}
    let start=a.xy/max(a.w,1e-6);let end=b.xy/max(b.w,1e-6);
    let direction=(end-start)*view.region.zw;
    let n=vec2(-direction.y,direction.x)*inverseSqrt(max(dot(direction,direction),1e-10));
    if vertex>=6u {
        let points=array<vec2<f32>,3>(vec2(0.,0.),vec2(-8.,4.),vec2(-8.,-4.));
        let shape=points[vertex-6u]*view.options.z;
        let forward=direction*inverseSqrt(max(dot(direction,direction),1e-10));
        let offset=(forward*shape.x+n*shape.y)*2./view.region.zw;
        b.x+=offset.x*b.w;b.y+=offset.y*b.w;out.position=b;return out;
    }
    let corner=corners[vertex];var clip=mix(a,b,corner.x);
    clip.x+=n.x*corner.y*select(2.,3.,is_gizmo)*view.options.z/view.region.z*clip.w;
    clip.y+=n.y*corner.y*select(2.,3.,is_gizmo)*view.options.z/view.region.w*clip.w;
    out.position=clip;return out;
}
@fragment fn axis_color(input:AxisOut)->@location(0) vec4<f32>{return vec4(input.color,0.9);}
@fragment fn outline(@builtin(position) position:vec4<f32>)->@location(0) vec4<f32>{
    let p=vec2<i32>(position.xy);let lower=vec2<i32>(view.region.xy);let upper=lower+vec2<i32>(view.region.zw)-vec2(1);
    if textureLoad(mask,p,0).r>0.5 {discard;}
    let radius=i32(clamp(round(2.*view.options.z),1.,6.));var edge=0.;
    for(var y=-radius;y<=radius;y++){for(var x=-radius;x<=radius;x++){
        if x*x+y*y<=radius*radius {let q=p+vec2(x,y);if all(q>=lower)&&all(q<=upper){edge=max(edge,textureLoad(mask,q,0).r);}}
    }}
    if edge<0.5 {discard;}
    return vec4(0.95,0.43,0.10,1.);
}
