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
    @location(0) pos_info: vec4<f32>, // x (bar_index), y_rel (price - base), dir (0.0=Up, 1.0=Down), size (px)
    @location(1) color: vec4<f32>,
) -> VertexOutput {
    var out: VertexOutput;

    let bar_idx = pos_info.x;
    let y_rel = pos_info.y;
    let dir = pos_info.z;
    let size = pos_info.w * u.dpr;

    // Discard invalid / NaN markers
    if (bar_idx != bar_idx || y_rel != y_rel) {
        out.position = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        out.color = vec4<f32>(0.0);
        return out;
    }

    // Anchor position in pixel coordinates
    let anchor_x = (bar_idx + 0.5 - u.from_index) * u.px_per_bar;
    let anchor_y = (y_rel - u.y_offset) * u.y_scale;

    // Geometry of triangle:
    // Up triangle (dir < 0.5): tip pointing UP (+y), base below (-y)
    // Down triangle (dir >= 0.5): tip pointing DOWN (-y), base above (+y)
    var offset = vec2<f32>(0.0, 0.0);
    let half_w = size * 0.60;
    let tip_h = size * 0.70;
    let base_h = size * 0.40;

    if (dir < 0.5) {
        // Up triangle (pointing up towards +y)
        switch (vertex_index) {
            case 0u: { offset = vec2<f32>(0.0, tip_h); }       // Top tip
            case 1u: { offset = vec2<f32>(-half_w, -base_h); } // Bottom left
            default: { offset = vec2<f32>(half_w, -base_h); }  // Bottom right
        }
    } else {
        // Down triangle (pointing down towards -y)
        switch (vertex_index) {
            case 0u: { offset = vec2<f32>(0.0, -tip_h); }      // Bottom tip
            case 1u: { offset = vec2<f32>(half_w, base_h); }   // Top right
            default: { offset = vec2<f32>(-half_w, base_h); }  // Top left
        }
    }

    let cur_pos = vec2<f32>(anchor_x, anchor_y) + offset;
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
