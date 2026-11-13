struct MarkerUniforms {
    viewport_px: vec2<f32>,
    px_per_bar: f32,
    from_index: f32,
    y_scale: f32,
    y_offset: f32,
    dpr: f32,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> u: MarkerUniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_marker(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) pos_info: vec4<f32>,   // x (bar_index), y_rel (low for Up, high for Down), dir (0.0=Up, 1.0=Down), size (px)
    @location(1) stack_info: vec4<f32>, // stack_index, 0.0, 0.0, 0.0
    @location(2) color: vec4<f32>,
) -> VertexOutput {
    var out: VertexOutput;

    let bar_idx = pos_info.x;
    let y_rel = pos_info.y;
    let dir = pos_info.z;
    let size = pos_info.w * u.dpr;
    let stack_idx = stack_info.x;

    // Discard invalid / NaN markers
    if (bar_idx != bar_idx || y_rel != y_rel) {
        out.position = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        out.color = vec4<f32>(0.0);
        return out;
    }

    // Anchor position in pixel coordinates (exact candle low or high)
    let anchor_x = (bar_idx + 0.5 - u.from_index) * u.px_per_bar;
    let anchor_y = (y_rel - u.y_offset) * u.y_scale;

    // Triangle sizing
    let margin = 4.0 * u.dpr;
    let spacing = 2.5 * u.dpr;
    let half_w = size * 0.55;
    let h = size * 0.90;

    var cur_pos = vec2<f32>(anchor_x, anchor_y);

    if (dir < 0.5) {
        // Upwards triangle: placed below candle LOW, pointing UP
        // Top tip is closest to the candle low wick
        let top_y = anchor_y - margin - stack_idx * (h + spacing);
        let base_y = top_y - h;

        switch (vertex_index) {
            case 0u: { cur_pos = vec2<f32>(anchor_x, top_y); }            // Top tip
            case 1u: { cur_pos = vec2<f32>(anchor_x - half_w, base_y); }  // Bottom left
            default: { cur_pos = vec2<f32>(anchor_x + half_w, base_y); }  // Bottom right
        }
    } else {
        // Downwards triangle: placed above candle HIGH, pointing DOWN
        // Bottom tip is closest to the candle high wick
        let tip_y = anchor_y + margin + stack_idx * (h + spacing);
        let base_y = tip_y + h;

        switch (vertex_index) {
            case 0u: { cur_pos = vec2<f32>(anchor_x, tip_y); }            // Bottom tip
            case 1u: { cur_pos = vec2<f32>(anchor_x + half_w, base_y); }  // Top right
            default: { cur_pos = vec2<f32>(anchor_x - half_w, base_y); }  // Top left
        }
    }

    out.color = color;

    // Map [0..viewport_px] to clip space [-1, 1]
    let clip_x = (cur_pos.x / u.viewport_px.x) * 2.0 - 1.0;
    let clip_y = (cur_pos.y / u.viewport_px.y) * 2.0 - 1.0;

    out.position = vec4<f32>(clip_x, clip_y, 0.0, 1.0);
    return out;
}

@fragment
fn fs_marker(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
