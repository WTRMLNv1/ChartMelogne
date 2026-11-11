use crate::metrics::{compute_histogram, get_process_working_set_mb, LatencyHistogram};
use chartmelogne_core::{
    candle_store::CandleStore,
    synthetic::{generate_synthetic_candles, Mulberry32, SyntheticDataOptions},
    types::Bar,
};
use chartmelogne_render::{ChartRenderer, ViewParams};
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryBreakdown {
    pub store_columns_mb: f64,
    pub cpu_packed_buffer_mb: f64,
    pub gpu_vbo_mb: f64,
    pub total_memory_mb: f64,
    pub working_set_mb: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GateMetrics {
    pub synthetic_gen_ms: f64,
    pub set_data_cpu_packing_ms: f64,
    pub set_data_to_first_paint_ms: f64,
    pub live_tick_to_painted: LatencyHistogram,
    pub append_to_painted: LatencyHistogram,
    pub memory_at_450k: MemoryBreakdown,
}

pub struct GateResults {
    pub metrics: GateMetrics,
    pub store: CandleStore,
    pub renderer: ChartRenderer,
    pub view: ViewParams,
}

pub fn run_gates(device: &wgpu::Device, queue: &wgpu::Queue) -> GateResults {
    const BAR_COUNT: usize = 450_000;
    const PLOT_WIDTH: u32 = 935;
    const PLOT_HEIGHT: u32 = 572;

    println!("\n=== Running M0 Gate Benchmarks ({} Bars) ===", BAR_COUNT);

    // 1. Synthetic Data Generation (seed 1337)
    let t_gen_start = Instant::now();
    let options = SyntheticDataOptions {
        count: BAR_COUNT,
        start_price: 1000.0,
        start_time_ms: 1577870100000.0,
        interval_ms: 60_000.0,
        volatility: 0.002,
        seed: 1337,
    };
    let data = generate_synthetic_candles(options);
    let synthetic_gen_ms = (t_gen_start.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0;
    println!("1. Synthetic 450k Bars Generation: {:.2} ms", synthetic_gen_ms);

    // 2. CandleStore initialization & setData to first paint
    let mut store = CandleStore::new(BAR_COUNT, 330);
    let mut renderer = ChartRenderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.resize(PLOT_WIDTH, PLOT_HEIGHT);

    let bars_visible = 300.0f32;
    let from_index = (BAR_COUNT as f32 - bars_visible).max(0.0);
    let px_per_bar = (PLOT_WIDTH as f32) / bars_visible;

    let t_pack_start = Instant::now();
    store.set_data(&data, true).expect("Store set_data failed");
    let set_data_cpu_packing_ms =
        (t_pack_start.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0;

    let view = ViewParams {
        from_index,
        px_per_bar,
        y_min: 900.0,
        y_max: 1100.0,
        base_price: store.base_price() as f32,
        count: store.count(),
        dpr: 1.0,
        up_color: [0.03, 0.65, 0.55, 1.0],
        down_color: [0.95, 0.25, 0.33, 1.0],
        clear_color: Some(wgpu::Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        }),
    };

    // Full setData to First Paint: upload + render + GPU wait
    let t_first_paint_start = Instant::now();
    renderer.upload_all(&store.ohlc, store.count());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("First Paint Encoder"),
    });
    renderer.render(&mut encoder, &view);
    queue.submit(Some(encoder.finish()));
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let set_data_to_first_paint_ms =
        set_data_cpu_packing_ms + (t_first_paint_start.elapsed().as_secs_f64() * 1000.0);
    let set_data_to_first_paint_ms = (set_data_to_first_paint_ms * 100.0).round() / 100.0;
    println!(
        "2. setData to First Paint (CPU pack + upload + draw + GPU wait): {:.2} ms (CPU packing: {:.2} ms)",
        set_data_to_first_paint_ms, set_data_cpu_packing_ms
    );

    // Memory Breakdown
    let mem = store.get_memory_usage();
    let store_columns_mb =
        ((mem.store_bytes as f64) / (1024.0 * 1024.0) * 100.0).round() / 100.0;
    let cpu_packed_buffer_mb = 0.0;
    let gpu_vbo_mb =
        ((renderer.instance_capacity() * 16) as f64 / (1024.0 * 1024.0) * 100.0).round() / 100.0;
    let total_memory_mb = ((store_columns_mb + gpu_vbo_mb) * 100.0).round() / 100.0;
    let working_set_mb = get_process_working_set_mb();

    println!("   Memory at 450k bars:");
    println!("   - Store Columns: {:.2} MB", store_columns_mb);
    println!("   - GPU Instance VBO: {:.2} MB", gpu_vbo_mb);
    println!("   - Total Data Footprint: {:.2} MB", total_memory_mb);
    println!("   - Process Working Set: {:.2} MB", working_set_mb);

    // 3. Live Tick to Painted (500 samples with GPU wait)
    println!("3. Measuring Live Tick to Painted (500 samples)...");
    let mut rng = Mulberry32::new(42);
    let last_idx = store.count() - 1;
    let mut base_close = store.get_close(last_idx);
    let base_price = store.base_price();

    // Warmup 20 iterations
    for _ in 0..20 {
        base_close += 0.05;
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
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Warmup Tick Encoder"),
        });
        renderer.render(&mut encoder, &view);
        queue.submit(Some(encoder.finish()));
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    }

    let mut tick_latencies = Vec::with_capacity(500);
    for _ in 0..500 {
        let delta = (rng.next_f64() - 0.49) * 0.5;
        base_close += delta;
        let high = store.get_high(last_idx).max(base_close);
        let low = store.get_low(last_idx).min(base_close);

        let t0 = Instant::now();
        let bar = Bar {
            time: store.time[last_idx],
            open: store.get_open(last_idx),
            high,
            low,
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

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Live Tick Encoder"),
        });
        renderer.render(&mut encoder, &view);
        queue.submit(Some(encoder.finish()));
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();

        tick_latencies.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    let live_tick_hist = compute_histogram(tick_latencies);
    println!(
        "   Live Tick to Painted: p50={:.3} ms, p95={:.3} ms, p99={:.3} ms, max={:.3} ms, avg={:.3} ms",
        live_tick_hist.p50_ms,
        live_tick_hist.p95_ms,
        live_tick_hist.p99_ms,
        live_tick_hist.max_ms,
        live_tick_hist.avg_ms
    );

    // 4. Append Bar to Painted (200 samples with GPU wait)
    println!("4. Measuring Append Bar to Painted (200 samples)...");
    let mut last_time = store.time[store.count() - 1];
    let mut last_close = store.get_close(store.count() - 1);
    let mut append_latencies = Vec::with_capacity(200);

    for _ in 0..200 {
        last_time += 60_000.0;
        let o = last_close;
        let delta = (rng.next_f64() - 0.49) * 2.0;
        let c = o + delta;
        let h = o.max(c) + rng.next_f64();
        let l = o.min(c) - rng.next_f64();

        let t0 = Instant::now();
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

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Append Encoder"),
        });
        renderer.render(&mut encoder, &view);
        queue.submit(Some(encoder.finish()));
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();

        append_latencies.push(t0.elapsed().as_secs_f64() * 1000.0);
        last_close = c;
    }
    let append_hist = compute_histogram(append_latencies);
    println!(
        "   Append to Painted: p50={:.3} ms, p95={:.3} ms, p99={:.3} ms, max={:.3} ms, avg={:.3} ms",
        append_hist.p50_ms,
        append_hist.p95_ms,
        append_hist.p99_ms,
        append_hist.max_ms,
        append_hist.avg_ms
    );

    let memory_breakdown = MemoryBreakdown {
        store_columns_mb,
        cpu_packed_buffer_mb,
        gpu_vbo_mb,
        total_memory_mb,
        working_set_mb,
    };

    let metrics = GateMetrics {
        synthetic_gen_ms,
        set_data_cpu_packing_ms,
        set_data_to_first_paint_ms,
        live_tick_to_painted: live_tick_hist,
        append_to_painted: append_hist,
        memory_at_450k: memory_breakdown,
    };

    GateResults {
        metrics,
        store,
        renderer,
        view,
    }
}
