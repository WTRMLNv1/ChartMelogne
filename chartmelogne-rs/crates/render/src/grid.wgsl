struct GridUniforms {
    viewport_px: vec2<f32>,
    color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: GridUniforms;

struct VertexInput {
    @location(0) position: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_grid(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let clip_x = (in.position.x / u.viewport_px.x) * 2.0 - 1.0;
    let clip_y = (in.position.y / u.viewport_px.y) * 2.0 - 1.0;
    out.position = vec4<f32>(clip_x, clip_y, 0.0, 1.0);
    out.color = u.color;
    return out;
}

@fragment
fn fs_grid(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
