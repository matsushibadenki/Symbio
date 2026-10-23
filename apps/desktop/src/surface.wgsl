struct Camera { vp: mat4x4<f32> }
@group(0) @binding(0) var<uniform> camera: Camera;
struct Out { @builtin(position) clip:vec4<f32>, @location(0) world:vec3<f32>, @location(1) color:vec3<f32> }
@vertex fn vs(@location(0) position:vec3<f32>, @location(1) color:vec3<f32>)->Out {
 var o:Out;o.clip=camera.vp*vec4(position,1.);o.world=position;o.color=color;return o;
}
@fragment fn fs(i:Out)->@location(0) vec4<f32> {
 let n=normalize(cross(dpdx(i.world),dpdy(i.world)));
 let light=0.55+0.45*abs(dot(n,normalize(vec3(-0.4,0.85,0.6))));
 return vec4(i.color*light,1.);
}
@fragment fn line(i:Out)->@location(0) vec4<f32> {return vec4(i.color,1.);}
