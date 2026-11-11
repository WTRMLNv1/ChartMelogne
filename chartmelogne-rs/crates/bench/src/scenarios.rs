use crate::metrics::{MetricsTracker, ScenarioResult};
use chartmelogne_core::{
    candle_store::CandleStore,
    synthetic::Mulberry32,
    types::Bar,
};
use chartmelogne_render::{ChartRenderer, ViewParams};
use std::time::{Duration, Instant};

pub struct ScenarioSuiteResult {
    pub continuous_pan: ScenarioResult,
    pub zoom_sweep: ScenarioResult,
    pub live_tick_100hz: ScenarioResult,
    pub append_bar_1hz: ScenarioResult,
}

const TARGET_FRAME_DURATION: Duration = Duration::from_micros(16_667); // 60 FPS target
const SCENARIO_DURATION_MS: f64 = 10_000.0; // 10 seconds

pub fn run_scenarios(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    store: &mut CandleStore,
    renderer: &mut ChartRenderer,
    base_view: &ViewParams,
) -> ScenarioSuiteResult {
    println!("\n=== Running 10-Second Offscreen Scenarios (450,000 Bars) ===");

    // 1. Continuous Pan (10s)
    println!("1. Running scenario: continuous_pan (10s)...");
    let mut tracker = MetricsTracker::new();
    let mut view = base_view.clone();
    let initial_from = (store.count() as f32 - 300.0).max(0.0);
    view.from_index = initial_from;
    view.px_per_bar = 935.0 / 300.0;

    let start_pan = Instant::now();
    while start_pan.elapsed().as_secs_f64() * 1000.0 < SCENARIO_DURATION_MS {
        let frame_start = Instant::now();
        let elapsed_ms = start_pan.elapsed().as_secs_f64() * 1000.0;

        let offset = (elapsed_ms * 0.003).sin() * 500.0;
        view.from_index = (initial_from as f64 + offset).max(0.0) as f32;

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Pan Frame Encoder"),
        });
        renderer.render(&mut encoder, &view);
        queue.submit(Some(encoder.finish()));
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();

        let render_time = frame_start.elapsed();
        tracker.record_frame_ms(render_time.as_secs_f64() * 1000.0);

        if render_time < TARGET_FRAME_DURATION {
            std::thread::sleep(TARGET_FRAME_DURATION - render_time);
        }
    }
    let pan_result = tracker.finish("continuous_pan", start_pan.elapsed().as_secs_f64() * 1000.0);
    println!(
        "   continuous_pan: {} frames, {:.1} fps, p50={:.2} ms, p95={:.2} ms, max={:.2} ms, dropped={}",
        pan_result.total_frames,
        pan_result.fps,
        pan_result.p50_ms,
        pan_result.p95_ms,
        pan_result.max_ms,
        pan_result.dropped_frames
    );

    // 2. Zoom Sweep (100 to 450,000 bars) (10s)
    println!("2. Running scenario: zoom_sweep (10s)...");
    let mut tracker = MetricsTracker::new();
    let min_bars = 100.0f64;
    let max_bars = store.count().min(450_000) as f64;
    let log_min = min_bars.ln();
    let log_max = max_bars.ln();

    let start_zoom = Instant::now();
    while start_zoom.elapsed().as_secs_f64() * 1000.0 < SCENARIO_DURATION_MS {
        let frame_start = Instant::now();
        let elapsed_ms = start_zoom.elapsed().as_secs_f64() * 1000.0;

        let t = ((elapsed_ms * 0.002).sin() + 1.0) * 0.5;
        let bars_visible = (log_min + t * (log_max - log_min)).exp() as f32;
        view.px_per_bar = 935.0 / bars_visible;

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Zoom Frame Encoder"),
        });
        renderer.render(&mut encoder, &view);
        queue.submit(Some(encoder.finish()));
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();

        let render_time = frame_start.elapsed();
        tracker.record_frame_ms(render_time.as_secs_f64() * 1000.0);

        if render_time < TARGET_FRAME_DURATION {
            std::thread::sleep(TARGET_FRAME_DURATION - render_time);
        }
    }
    let zoom_result = tracker.finish("zoom_sweep", start_zoom.elapsed().as_secs_f64() * 1000.0);
    println!(
        "   zoom_sweep: {} frames, {:.1} fps, p50={:.2} ms, p95={:.2} ms, max={:.2} ms, dropped={}",
        zoom_result.total_frames,
        zoom_result.fps,
        zoom_result.p50_ms,
        zoom_result.p95_ms,
        zoom_result.max_ms,
        zoom_result.dropped_frames
    );

    // 3. Live Tick 100Hz (10s)
    println!("3. Running scenario: live_tick_100hz (10s)...");
    let mut tracker = MetricsTracker::new();
    let mut rng = Mulberry32::new(999);
    let last_idx = store.count() - 1;
    let mut base_close = store.get_close(last_idx);
    let base_price = store.base_price();
    let mut tick_count = 0usize;

    view.px_per_bar = 935.0 / 300.0;
    view.from_index = (store.count() as f32 - 300.0).max(0.0);

    let start_tick = Instant::now();
    while start_tick.elapsed().as_secs_f64() * 1000.0 < SCENARIO_DURATION_MS {
        let frame_start = Instant::now();
        let elapsed_ms = start_tick.elapsed().as_secs_f64() * 1000.0;

        let expected_ticks = (elapsed_ms / 10.0).floor() as usize;
        while tick_count < expected_ticks {
            tick_count += 1;
            let delta = (rng.next_f64() - 0.49) * 0.5;
            base_close += delta;
            let bar = Bar {
                time: store.time[last_idx],
                open: store.get_open(last_idx),
                high: store.get_high(last_idx).max(base_close),
                low: store.get_low(last_idx).min(base_close),
                close: base_close,
                volume: store.volume[last_idx] + 1,
                flags: None,
            };
            store.update_last(bar);
            let bar_f32 = [
                (bar.open - base_price) as f32,
                (bar.high - base_price) as f32,
                (bar.low - base_price) as f32,
                (bar.close - base_price) as f32,
            ];
            renderer.update_last(&bar_f32, last_idx);
        }

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Live Tick Frame Encoder"),
        });
        renderer.render(&mut encoder, &view);
        queue.submit(Some(encoder.finish()));
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();

        let render_time = frame_start.elapsed();
        tracker.record_frame_ms(render_time.as_secs_f64() * 1000.0);

        if render_time < TARGET_FRAME_DURATION {
            std::thread::sleep(TARGET_FRAME_DURATION - render_time);
        }
    }
    let tick_result = tracker.finish("live_tick_100hz", start_tick.elapsed().as_secs_f64() * 1000.0);
    println!(
        "   live_tick_100hz: {} frames, {:.1} fps, p50={:.2} ms, p95={:.2} ms, max={:.2} ms, dropped={}",
        tick_result.total_frames,
        tick_result.fps,
        tick_result.p50_ms,
        tick_result.p95_ms,
        tick_result.max_ms,
        tick_result.dropped_frames
    );

    // 4. Append Bar 1Hz (10s)
    println!("4. Running scenario: append_bar_1hz (10s)...");
    let mut tracker = MetricsTracker::new();
    let mut last_time = store.time[store.count() - 1];
    let mut last_close = store.get_close(store.count() - 1);
    let mut appends_done = 0usize;

    let start_append = Instant::now();
    while start_append.elapsed().as_secs_f64() * 1000.0 < SCENARIO_DURATION_MS {
        let frame_start = Instant::now();
        let elapsed_ms = start_append.elapsed().as_secs_f64() * 1000.0;

        let expected_appends = (elapsed_ms / 1000.0).floor() as usize;
        while appends_done < expected_appends {
            appends_done += 1;
            last_time += 60_000.0;
            let o = last_close;
            let delta = (rng.next_f64() - 0.49) * 2.0;
            let c = o + delta;
            let h = o.max(c) + rng.next_f64();
            let l = o.min(c) - rng.next_f64();

            let bar = Bar {
                time: last_time,
                open: o,
                high: h,
                low: l,
                close: c,
                volume: 1000,
                flags: None,
            };
            let append_idx = store.count();
            store.append(bar);
            let bar_f32 = [
                (bar.open - base_price) as f32,
                (bar.high - base_price) as f32,
                (bar.low - base_price) as f32,
                (bar.close - base_price) as f32,
            ];
            renderer.append(&bar_f32, append_idx);
            last_close = c;
        }

        view.count = store.count();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Append Frame Encoder"),
        });
        renderer.render(&mut encoder, &view);
        queue.submit(Some(encoder.finish()));
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();

        let render_time = frame_start.elapsed();
        tracker.record_frame_ms(render_time.as_secs_f64() * 1000.0);

        if render_time < TARGET_FRAME_DURATION {
            std::thread::sleep(TARGET_FRAME_DURATION - render_time);
        }
    }
    let append_result = tracker.finish("append_bar_1hz", start_append.elapsed().as_secs_f64() * 1000.0);
    println!(
        "   append_bar_1hz: {} frames, {:.1} fps, p50={:.2} ms, p95={:.2} ms, max={:.2} ms, dropped={}",
        append_result.total_frames,
        append_result.fps,
        append_result.p50_ms,
        append_result.p95_ms,
        append_result.max_ms,
        append_result.dropped_frames
    );

    ScenarioSuiteResult {
        continuous_pan: pan_result,
        zoom_sweep: zoom_result,
        live_tick_100hz: tick_result,
        append_bar_1hz: append_result,
    }
}
