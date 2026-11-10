struct Uniforms {
    viewport_px: vec2<f32>,
    px_per_bar: f32,
    from_index: f32,
    y_scale: f32,
    y_offset: f32,
    bars_per_node: f32,
    dpr: f32,
    up_color: vec4<f32>,
    down_color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

// Unit geometry: x = side (-0.5..0.5), y = anchor (0.0..1.0), z = part (0=wick, 1=body)
struct VertexInput {
    @location(0) unit_vertex: vec3<f32>,
};

// Instanced attributes: 16 bytes per candle (open, high, low, close relative to basePrice)
struct InstanceInput {
    @location(1) ohlc: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(
    in: VertexInput,
    instance: InstanceInput,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let is_up = f32(instance.ohlc.w >= instance.ohlc.x);
    out.color = mix(u.down_color, u.up_color, is_up);

    // Compute bar index directly from instance_index
    let bar_index = f32(instance_index);

    // Candle horizontal center
    var center_x = (bar_index + 0.5 * u.bars_per_node - u.from_index) * u.px_per_bar;
    center_x = floor(center_x) + 0.5;

    // Body and wick dimensions in device pixels
    var body_width = clamp(u.px_per_bar * 0.7, 1.0, 30.0 * u.dpr);
    var wick_width = max(1.0, floor(max(1.0, 0.1 * body_width) + 0.5));

    // Below 3 px per bar, drop bodies and only draw 1px wicks
    if (u.px_per_bar < 3.0 * u.dpr) {
        body_width = 0.0;
        wick_width = 1.0;
    }

    let o = instance.ohlc.x;
    let h = instance.ohlc.y;
    let l = instance.ohlc.z;
    let c = instance.ohlc.w;

    // Wick y range
    let y_low = (l - u.y_offset) * u.y_scale;
    let y_high = (h - u.y_offset) * u.y_scale;
    let cur_y_wick = mix(y_low, y_high, in.unit_vertex.y);

    // Body y range
    let body_min = min(o, c);
    let body_max = max(o, c);
    var y_bottom = (body_min - u.y_offset) * u.y_scale;
    var y_top = (body_max - u.y_offset) * u.y_scale;

    // Minimum body height of 1 device pixel
    if (y_top - y_bottom < 1.0) {
        let mid = (y_bottom + y_top) * 0.5;
        y_bottom = mid - 0.5;
        y_top = mid + 0.5;
    }
    let cur_y_body = mix(y_bottom, y_top, in.unit_vertex.y);

    // Select wick vs body
    let cur_y = mix(cur_y_wick, cur_y_body, in.unit_vertex.z);
    let cur_width = mix(wick_width, body_width, in.unit_vertex.z);

    let cur_x = center_x + in.unit_vertex.x * cur_width;

    // Map device pixels [0..width, 0..height] to clip space [-1, 1]
    let clip_x = (cur_x / u.viewport_px.x) * 2.0 - 1.0;
    let clip_y = (cur_y / u.viewport_px.y) * 2.0 - 1.0;

    out.position = vec4<f32>(clip_x, clip_y, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
