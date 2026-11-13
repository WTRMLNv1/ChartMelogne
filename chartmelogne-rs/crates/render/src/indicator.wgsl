struct IndicatorUniforms {
    viewport_px: vec2<f32>,
    px_per_bar: f32,
    from_index: f32,
    y_scale: f32,
    y_offset: f32,
    dpr: f32,
    line_width: f32,
    color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: IndicatorUniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_indicator(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
    @location(0) curr: f32,
    @location(1) next: f32,
) -> VertexOutput {
    var out: VertexOutput;

    // Discard missing / warmup segments where either endpoint is NaN
    if (curr != curr || next != next) {
        out.position = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        out.color = vec4<f32>(0.0);
        return out;
    }

    let bar_idx = f32(instance_index);
    let x0 = (bar_idx + 0.5 - u.from_index) * u.px_per_bar;
    let y0 = (curr - u.y_offset) * u.y_scale;
    let p0 = vec2<f32>(x0, y0);

    let x1 = (bar_idx + 1.5 - u.from_index) * u.px_per_bar;
    let y1 = (next - u.y_offset) * u.y_scale;
    let p1 = vec2<f32>(x1, y1);

    // Quad line expansion along normal
    let delta = p1 - p0;
    let len = length(delta);
    let dir = select(vec2<f32>(1.0, 0.0), delta / len, len > 0.0001);
    let norm = vec2<f32>(-dir.y, dir.x);
    let hw = max(1.0, u.line_width * u.dpr * 0.5);

    var t: f32;
    var side: f32;
    switch (vertex_index) {
        case 0u: { t = 0.0; side =  1.0; }
        case 1u: { t = 0.0; side = -1.0; }
        case 2u: { t = 1.0; side = -1.0; }
        case 3u: { t = 0.0; side =  1.0; }
        case 4u: { t = 1.0; side = -1.0; }
        default: { t = 1.0; side =  1.0; }
    }
    let cur_pos = mix(p0, p1, t) + norm * (side * hw);
    out.color = u.color;

    // Map [0..viewport_px] to clip space [-1, 1]
    let clip_x = (cur_pos.x / u.viewport_px.x) * 2.0 - 1.0;
    let clip_y = (cur_pos.y / u.viewport_px.y) * 2.0 - 1.0;

    out.position = vec4<f32>(clip_x, clip_y, 0.0, 1.0);
    return out;
}

@fragment
fn fs_indicator(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
