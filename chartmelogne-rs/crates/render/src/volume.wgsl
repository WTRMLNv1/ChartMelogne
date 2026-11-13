struct VolumeUniforms {
    viewport_px: vec2<f32>,
    px_per_bar: f32,
    from_index: f32,
    max_volume: f32,
    height_ratio: f32,
    bar_width: f32,
    max_h: f32,
    up_color: vec4<f32>,
    down_color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: VolumeUniforms;

// Unit quad: x in [-0.5, 0.5], y in [0.0, 1.0]
struct VertexInput {
    @location(0) position: vec2<f32>,
};

// Instanced volume data: [volume, is_up]
struct InstanceInput {
    @location(1) data: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_volume(
    in: VertexInput,
    instance: InstanceInput,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let bar_index = f32(instance_index);
    var center_x = (bar_index + 0.5 - u.from_index) * u.px_per_bar;
    center_x = floor(center_x) + 0.5;

    let cur_x = center_x + in.position.x * u.bar_width;

    let norm_vol = clamp(instance.data.x / max(u.max_volume, 1.0), 0.0, 1.0);
    let bar_height = max(1.0, norm_vol * u.max_h);

    let cur_y = in.position.y * bar_height;

    // Bottom is at clip_y = -1.0
    let clip_x = (cur_x / u.viewport_px.x) * 2.0 - 1.0;
    let clip_y = (cur_y / u.viewport_px.y) * 2.0 - 1.0;

    out.position = vec4<f32>(clip_x, clip_y, 0.0, 1.0);
    out.color = mix(u.down_color, u.up_color, instance.data.y);
    return out;
}

@fragment
fn fs_volume(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
