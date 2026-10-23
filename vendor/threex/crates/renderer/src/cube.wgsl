struct Camera { vp: mat4x4<f32> }
@group(0) @binding(0) var<uniform> camera: Camera;
struct Out {
    @invariant @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) world: vec3<f32>,
}
@vertex fn vs(@location(0) p: vec3<f32>, @location(1) a: vec4<f32>, @location(2) b: vec4<f32>, @location(3) c: vec4<f32>, @location(4) d: vec4<f32>, @location(5) color: vec4<f32>) -> Out {
    let world = mat4x4<f32>(a,b,c,d) * vec4<f32>(p,1.0);
    var out: Out;
    out.position = camera.vp * world;
    out.world = world.xyz;
    out.color = color.rgb;
    return out;
}
@fragment fn fs(input: Out) -> @location(0) vec4<f32> {
    // Screen derivatives produce a geometric face normal after all parent transforms,
    // including non-uniform scale. No additional vertex/instance buffers are required.
    let cross_normal = cross(dpdy(input.world), dpdx(input.world));
    let normal = cross_normal * inverseSqrt(max(dot(cross_normal, cross_normal), 1e-20));
    let key = max(dot(normal, normalize(vec3<f32>(-0.55, 0.8, 0.65))), 0.0);
    let fill = max(dot(normal, normalize(vec3<f32>(0.8, 0.2, -0.5))), 0.0);
    let sky = 0.22 + 0.10 * (normal.y * 0.5 + 0.5);
    let light = vec3<f32>(sky, sky * 0.98, sky * 0.95) + vec3<f32>(0.78, 0.76, 0.72) * key
        + vec3<f32>(0.17, 0.17, 0.16) * fill;
    return vec4<f32>(input.color * light, 1.0);
}

@fragment fn selection_mask() -> @location(0) f32 { return 1.; }
