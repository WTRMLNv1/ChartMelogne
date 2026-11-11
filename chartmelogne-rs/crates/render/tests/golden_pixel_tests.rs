mod common;

use chartmelogne_core::{candle_store::CandleStore, types::CandleData};
use chartmelogne_render::{ChartRenderer, ViewParams};
use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize)]
struct FixtureCandles {
    time: Vec<f64>,
    open: Vec<f64>,
    high: Vec<f64>,
    low: Vec<f64>,
    close: Vec<f64>,
    volume: Vec<u32>,
}

#[derive(Debug, Deserialize)]
struct ViewportStateJson {
    #[serde(rename = "fromIndex")]
    from_index: f64,
    #[serde(rename = "barsVisible")]
    bars_visible: f64,
    #[serde(rename = "yMin")]
    y_min: f64,
    #[serde(rename = "yMax")]
    y_max: f64,
}

#[derive(Debug, Deserialize)]
struct ZoomLevelJson {
    #[serde(rename = "viewportState")]
    viewport_state: ViewportStateJson,
}

#[test]
fn test_golden_pixel_parity() {
    let (device, queue) = match common::create_test_device() {
        Some(dq) => dq,
        None => {
            eprintln!("Skipping test: no GPU adapter found");
            return;
        }
    };

    let fixtures_dir = common::get_fixtures_dir();
    let candles_path = fixtures_dir.join("golden-candles.json");
    let ticks_path = fixtures_dir.join("golden-viewport-ticks.json");
    let golden_png_dir = fixtures_dir.join("golden-png");

    let candles_json: FixtureCandles =
        serde_json::from_str(&fs::read_to_string(&candles_path).expect("Read golden-candles.json"))
            .expect("Parse golden-candles.json");

    let ticks_json: std::collections::BTreeMap<String, ZoomLevelJson> = serde_json::from_str(
        &fs::read_to_string(&ticks_path).expect("Read golden-viewport-ticks.json"),
    )
    .expect("Parse golden-viewport-ticks.json");

    let count = candles_json.time.len();
    let mut store = CandleStore::new(count, 330);
    let candle_data = CandleData {
        time: candles_json.time,
        open: candles_json.open,
        high: candles_json.high,
        low: candles_json.low,
        close: candles_json.close,
        volume: candles_json.volume,
        flags: None,
    };
    store.set_data(&candle_data, true).unwrap();

    let mut renderer = ChartRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.upload_all(&store.ohlc, store.count());

    let zoom_levels = [
        "zoomed_in_intraday",
        "medium_daily",
        "wide_multiweek",
        "full_zoom_out",
    ];
    let dpr_levels = [1.0f64, 1.5f64];

    let mut total_tests = 0;

    for &dpr in &dpr_levels {
        let css_w = 1000.0f64;
        let css_h = 600.0f64;
        let width_px = (css_w * dpr).round() as u32;
        let height_px = (css_h * dpr).round() as u32;
        let price_axis_width_px = (65.0 * dpr).round() as u32;
        let time_axis_height_px = (28.0 * dpr).round() as u32;
        let plot_width_px = width_px - price_axis_width_px;
        let plot_height_px = height_px - time_axis_height_px;

        renderer.resize(plot_width_px, plot_height_px);

        for &level_name in &zoom_levels {
            total_tests += 1;
            let level_info = ticks_json
                .get(level_name)
                .unwrap_or_else(|| panic!("Missing level {}", level_name));
            let state = &level_info.viewport_state;

            let px_per_bar = (plot_width_px as f32) / (state.bars_visible as f32);

            let view = ViewParams {
                from_index: state.from_index as f32,
                px_per_bar,
                y_min: state.y_min as f32,
                y_max: state.y_max as f32,
                base_price: store.base_price() as f32,
                count: store.count(),
                dpr: dpr as f32,
                up_color: [0.03, 0.65, 0.55, 1.0],
                down_color: [0.95, 0.25, 0.33, 1.0],
                clear_color: Some(wgpu::Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                }),
                grid_color: None,
                horizontal_grid_y: Vec::new(),
                vertical_grid_x: Vec::new(),
            };

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Golden Render Pass"),
            });
            renderer.render(&mut encoder, &view);
            queue.submit(Some(encoder.finish()));

            let rendered_img = common::read_texture_to_image(
                &device,
                &queue,
                renderer.texture(),
                plot_width_px,
                plot_height_px,
            );

            let png_name = if (dpr - 1.0).abs() < 1e-6 {
                format!("{}_dpr1.png", level_name)
            } else {
                format!("{}_dpr{}.png", level_name, dpr)
            };
            let golden_png_path = golden_png_dir.join(&png_name);
            let ref_dynamic = image::open(&golden_png_path)
                .unwrap_or_else(|e| panic!("Failed to load golden PNG {}: {:?}", golden_png_path.display(), e));
            let ref_img = ref_dynamic.to_rgba8();

            assert_eq!(
                rendered_img.width(),
                ref_img.width(),
                "Width mismatch for {}",
                png_name
            );
            assert_eq!(
                rendered_img.height(),
                ref_img.height(),
                "Height mismatch for {}",
                png_name
            );

            let total_pixels = (plot_width_px * plot_height_px) as usize;
            let mut diff_count = 0usize;
            let mut max_diff_channel = 0u8;

            let mut diff_img = image::RgbaImage::new(plot_width_px, plot_height_px);

            for y in 0..plot_height_px {
                for x in 0..plot_width_px {
                    let p_act = rendered_img.get_pixel(x, y);
                    let p_ref = ref_img.get_pixel(x, y);

                    let dr = (p_act[0] as i16 - p_ref[0] as i16).abs() as u8;
                    let dg = (p_act[1] as i16 - p_ref[1] as i16).abs() as u8;
                    let db = (p_act[2] as i16 - p_ref[2] as i16).abs() as u8;
                    let da = (p_act[3] as i16 - p_ref[3] as i16).abs() as u8;

                    let max_d = dr.max(dg).max(db).max(da);
                    if max_d > 0 {
                        diff_count += 1;
                        if max_d > max_diff_channel {
                            max_diff_channel = max_d;
                        }
                        // Highlight diff in magenta
                        diff_img.put_pixel(x, y, image::Rgba([255, 0, 255, 255]));
                    } else {
                        // Matching pixels shown dimmed
                        diff_img.put_pixel(
                            x,
                            y,
                            image::Rgba([p_act[0] / 3, p_act[1] / 3, p_act[2] / 3, p_act[3] / 2]),
                        );
                    }
                }
            }

            println!(
                "Golden check: {} ({}x{}) -> differing pixels: {} / {} ({:.4}%), max channel delta: {}",
                png_name,
                plot_width_px,
                plot_height_px,
                diff_count,
                total_pixels,
                (diff_count as f64 / total_pixels as f64) * 100.0,
                max_diff_channel
            );

            if diff_count > 0 {
                let target_dir = fixtures_dir.join("diff");
                let _ = fs::create_dir_all(&target_dir);
                let diff_path = target_dir.join(format!("diff_{}", png_name));
                let _ = diff_img.save(&diff_path);
                eprintln!("Diff image saved to: {}", diff_path.display());
            }

            // Per R2 spec: candles are opaque and unantialiased, so expect zero or near-zero differing pixels
            // (floating point rasterization edge differences across GPU backends should be <= 0.05% of pixels if any)
            let diff_ratio = (diff_count as f64) / (total_pixels as f64);
            assert!(
                diff_ratio <= 0.001,
                "Differing pixel ratio {:.4}% exceeds tolerance for {}",
                diff_ratio * 100.0,
                png_name
            );
        }
    }

    assert_eq!(total_tests, 8);
}
