struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) coord: vec2<f32>,
};

// Horus uses the same uniforms as thebookofshaders.com and shadertoy.com
struct Uniforms {
    mouse: vec2<f32>, // pixel mouse coords
    resolution: vec2<f32>, // pixel resolution
    time: f32, // time since program start
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // This is the same shader from thebookofshaders.com/03
    // Keep in mind that because this is WGSL, the Y axis is flipped vs GLSL
    let normalized = in.position.xy / uniforms.resolution;
    return vec4<f32>(normalized.rg, 0., 1.0);
}