mod gates;
mod metrics;
mod scenarios;

use gates::{run_gates, GateResults};
use scenarios::run_scenarios;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Debug)]
struct HardwareInfo {
    platform: String,
    arch: String,
    adapter_name: String,
    adapter_backend: String,
    adapter_device_type: String,
}

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Debug)]
struct DatasetInfo {
    bar_count: usize,
}

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Debug)]
struct MemorySummary {
    store_columns_mb: f64,
    cpu_packed_buffer_mb: f64,
    gpu_vbo_mb: f64,
    total_memory_mb: f64,
    process_working_set_mb: f64,
}

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Debug)]
struct BaselineRustReport {
    timestamp: String,
    hardware: HardwareInfo,
    dataset: DatasetInfo,
    metrics: serde_json::Value,
    gate_verification: serde_json::Value,
    scenarios: serde_json::Value,
}

fn create_gpu_device() -> (wgpu::Device, wgpu::Queue, wgpu::AdapterInfo) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        flags: wgpu::InstanceFlags::default(),
        memory_budget_thresholds: Default::default(),
        backend_options: Default::default(),
        display: None,
    });

    let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::VULKAN));
    let adapter = if !adapters.is_empty() {
        println!("Using Vulkan backend adapter: {:?}", adapters[0].get_info().name);
        adapters.into_iter().next().unwrap()
    } else {
        println!("Vulkan adapter not found, falling back to primary adapter...");
        let fallback_instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            flags: wgpu::InstanceFlags::default(),
            memory_budget_thresholds: Default::default(),
            backend_options: Default::default(),
            display: None,
        });
        let fallback_adapters =
            pollster::block_on(fallback_instance.enumerate_adapters(wgpu::Backends::all()));
        if fallback_adapters.is_empty() {
            panic!("No compatible GPU adapter found!");
        }
        fallback_adapters.into_iter().next().unwrap()
    };

    let adapter_info = adapter.get_info();

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Bench Device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
    }))
    .expect("Failed to create wgpu device");

    (device, queue, adapter_info)
}

fn get_results_path() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/bench -> crates -> chartmelogne-rs -> repo_root -> bench/results/baseline-rust.json
    manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("bench/results/baseline-rust.json")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(|s| s.as_str()).unwrap_or("headless");

    if mode == "window" {
        println!("\nOpening ChartMelogne Slint Window Mode (450,000 candles)...");
        chartmelogne_slint::run_chart_app(chartmelogne_slint::ChartAppConfig::default())
            .expect("Slint window failed to run");
        return;
    }

    if mode != "headless" {
        eprintln!("Unknown mode '{}'. Usage: cargo run -p bench --release -- [headless|window]", mode);
        std::process::exit(1);
    }

    println!("================================================================");
    println!("        ChartMelogne Rust M0/R3 Benchmark Runner                ");
    println!("================================================================");

    let (device, queue, adapter_info) = create_gpu_device();
    println!("GPU: {} ({:?}, {:?})", adapter_info.name, adapter_info.backend, adapter_info.device_type);

    // 1. Run Gates
    let GateResults {
        metrics,
        mut store,
        mut renderer,
        view,
    } = run_gates(&device, &queue);

    // 2. Run Scenarios (10s each)
    let scenario_results = run_scenarios(&device, &queue, &mut store, &mut renderer, &view);

    // 3. Gate Verification
    println!("\n================================================================");
    println!("                    GATE VERIFICATION                           ");
    println!("================================================================");

    let set_data_passed = metrics.set_data_to_first_paint_ms < 150.0;
    let live_tick_passed = metrics.live_tick_to_painted.p95_ms < 4.0;
    let append_passed = metrics.append_to_painted.p95_ms < 1.0;
    let memory_passed = metrics.memory_at_450k.total_memory_mb < 60.0;

    let pan_vsync_passed = scenario_results.continuous_pan.p95_ms < 16.6;
    let zoom_vsync_passed = scenario_results.zoom_sweep.p95_ms < 16.6;

    println!(
        "1. Memory Gate (< 60.0 MB):        {:.2} MB  [{}]",
        metrics.memory_at_450k.total_memory_mb,
        if memory_passed { "PASSED" } else { "FAILED" }
    );
    println!(
        "2. setData Gate (< 150.0 ms):      {:.2} ms  [{}]",
        metrics.set_data_to_first_paint_ms,
        if set_data_passed { "PASSED" } else { "FAILED" }
    );
    println!(
        "3. Live Tick Gate (< 4.0 ms):      p95 = {:.3} ms  [{}]",
        metrics.live_tick_to_painted.p95_ms,
        if live_tick_passed { "PASSED" } else { "FAILED" }
    );
    println!(
        "4. Append Bar Gate (< 1.0 ms):     p95 = {:.3} ms  [{}]",
        metrics.append_to_painted.p95_ms,
        if append_passed { "PASSED" } else { "FAILED" }
    );
    println!(
        "5. Pan 60fps budget (< 16.6 ms):    p95 = {:.2} ms  [{}]",
        scenario_results.continuous_pan.p95_ms,
        if pan_vsync_passed { "PASSED" } else { "FAILED" }
    );
    println!(
        "6. Zoom 60fps budget (< 16.6 ms):   p95 = {:.2} ms  [{}]",
        scenario_results.zoom_sweep.p95_ms,
        if zoom_vsync_passed { "PASSED" } else { "FAILED" }
    );

    let all_gates_passed = set_data_passed && live_tick_passed && append_passed && memory_passed;

    let report = serde_json::json!({
        "timestamp": chrono_iso_now(),
        "hardware": {
            "platform": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "adapterName": adapter_info.name,
            "adapterBackend": format!("{:?}", adapter_info.backend),
            "adapterDeviceType": format!("{:?}", adapter_info.device_type),
        },
        "dataset": {
            "barCount": 450_000,
        },
        "metrics": {
            "syntheticGenMs": metrics.synthetic_gen_ms,
            "setDataToFirstPaintWithFinishMs": metrics.set_data_to_first_paint_ms,
            "setDataCpuPackingMs": metrics.set_data_cpu_packing_ms,
            "liveTickToPaintedWithFinishMs": metrics.live_tick_to_painted.p95_ms,
            "appendWithFinishMs": metrics.append_to_painted.p95_ms,
            "liveTickHistogram": metrics.live_tick_to_painted,
            "appendHistogram": metrics.append_to_painted,
            "memoryAt450k": {
                "storeColumnsMb": metrics.memory_at_450k.store_columns_mb,
                "cpuPackedBufferMb": metrics.memory_at_450k.cpu_packed_buffer_mb,
                "gpuVboMb": metrics.memory_at_450k.gpu_vbo_mb,
                "totalMemoryMb": metrics.memory_at_450k.total_memory_mb,
                "processWorkingSetMb": metrics.memory_at_450k.working_set_mb,
            }
        },
        "gateVerification": {
            "setDataTarget150ms": if set_data_passed { "PASSED" } else { "FAILED" },
            "liveTickTarget4ms": if live_tick_passed { "PASSED" } else { "FAILED" },
            "appendTarget1ms": if append_passed { "PASSED" } else { "FAILED" },
            "memoryTarget60mb": if memory_passed { "PASSED" } else { "FAILED" },
            "panVsyncTarget16ms": if pan_vsync_passed { "PASSED" } else { "FAILED" },
            "zoomVsyncTarget16ms": if zoom_vsync_passed { "PASSED" } else { "FAILED" }
        },
        "scenarios": {
            "continuous_pan": scenario_results.continuous_pan,
            "zoom_sweep": scenario_results.zoom_sweep,
            "live_tick_100hz": scenario_results.live_tick_100hz,
            "append_bar_1hz": scenario_results.append_bar_1hz,
        }
    });

    let results_path = get_results_path();
    if let Some(parent) = results_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json_str = serde_json::to_string_pretty(&report).expect("Serialize report");
    fs::write(&results_path, json_str).expect("Write baseline-rust.json");
    println!("\nReport successfully saved to: {}", results_path.display());

    if !all_gates_passed {
        eprintln!("\nOne or more gates FAILED!");
        std::process::exit(1);
    } else {
        println!("\nALL M0 GATES PASSED SUCCESSFULLY!");
    }
}

fn chrono_iso_now() -> String {
    // Generate ISO 8601 UTC timestamp string without extra crate dependencies
    use std::time::SystemTime;
    let now = SystemTime::now();
    let duration = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    // Days since unix epoch
    let days = secs / 86400;
    let rem_secs = secs % 86400;
    let hours = rem_secs / 3600;
    let mins = (rem_secs % 3600) / 60;
    let s = rem_secs % 60;

    // Approximate year, month, day calculation
    let mut y = 1970i64;
    let mut d = days as i64;
    loop {
        let leap = if (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0) {
            1
        } else {
            0
        };
        let days_in_year = 365 + leap;
        if d < days_in_year {
            let month_days = [
                31,
                28 + leap,
                31,
                30,
                31,
                30,
                31,
                31,
                30,
                31,
                30,
                31,
            ];
            let mut m = 0;
            for (idx, &md) in month_days.iter().enumerate() {
                if d < md {
                    m = idx + 1;
                    break;
                }
                d -= md;
            }
            let day = d + 1;
            return format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, day, hours, mins, s);
        }
        d -= days_in_year;
        y += 1;
    }
}
