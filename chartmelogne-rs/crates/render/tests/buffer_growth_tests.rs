mod common;

use chartmelogne_render::ChartRenderer;

#[test]
fn test_buffer_growth_doubling_and_preservation() {
    let (device, queue) = match common::create_test_device() {
        Some(dq) => dq,
        None => {
            eprintln!("Skipping test: no GPU adapter found");
            return;
        }
    };

    let mut renderer = ChartRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    assert_eq!(renderer.instance_capacity(), 1024);
    assert_eq!(renderer.instance_count(), 0);

    // Populate initial 1024 bars with known test values: bar i has [i, i+0.5, i-0.5, i+0.25]
    let mut initial_ohlc = Vec::with_capacity(1024 * 4);
    for i in 0..1024 {
        let f = i as f32;
        initial_ohlc.push(f);
        initial_ohlc.push(f + 0.5);
        initial_ohlc.push(f - 0.5);
        initial_ohlc.push(f + 0.25);
    }

    renderer.upload_all(&initial_ohlc, 1024);
    assert_eq!(renderer.instance_capacity(), 1024);
    assert_eq!(renderer.instance_count(), 1024);

    // Append bar at index 1024 (triggers growth past capacity)
    let new_bar_1024 = [1024.0, 1024.5, 1023.5, 1024.25];
    renderer.append(&new_bar_1024, 1024);

    // Capacity must have doubled to 2048
    assert_eq!(
        renderer.instance_capacity(),
        2048,
        "Capacity must double when appending past capacity"
    );
    assert_eq!(renderer.instance_count(), 1025);

    // Verify all previous bars and the newly appended bar were preserved
    let readback =
        common::read_buffer_f32(&device, &queue, renderer.instance_buffer(), 1025 * 4);
    for i in 0..1024 {
        let f = i as f32;
        assert_eq!(readback[i * 4], f, "Bar {} open mismatch", i);
        assert_eq!(readback[i * 4 + 1], f + 0.5, "Bar {} high mismatch", i);
        assert_eq!(readback[i * 4 + 2], f - 0.5, "Bar {} low mismatch", i);
        assert_eq!(readback[i * 4 + 3], f + 0.25, "Bar {} close mismatch", i);
    }
    assert_eq!(readback[1024 * 4..1025 * 4], new_bar_1024);

    // Append bar at index 2048 (triggers another doubling)
    let new_bar_2048 = [2048.0, 2048.5, 2047.5, 2048.25];
    renderer.append(&new_bar_2048, 2048);
    assert_eq!(renderer.instance_capacity(), 4096);
    assert_eq!(renderer.instance_count(), 2049);

    // Update last bar in-place
    let updated_bar_2048 = [2048.0, 2050.0, 2045.0, 2049.0];
    renderer.update_last(&updated_bar_2048, 2048);

    let readback_2049 =
        common::read_buffer_f32(&device, &queue, renderer.instance_buffer(), 2049 * 4);
    assert_eq!(readback_2049[2048 * 4..2049 * 4], updated_bar_2048);
}
