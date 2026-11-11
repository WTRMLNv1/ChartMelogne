use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LatencyHistogram {
    pub count: usize,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
    pub avg_ms: f64,
}

pub fn compute_histogram(mut samples: Vec<f64>) -> LatencyHistogram {
    if samples.is_empty() {
        return LatencyHistogram {
            count: 0,
            p50_ms: 0.0,
            p95_ms: 0.0,
            p99_ms: 0.0,
            max_ms: 0.0,
            avg_ms: 0.0,
        };
    }

    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let count = samples.len();
    let sum: f64 = samples.iter().sum();

    let percentile = |p: f64| -> f64 {
        let idx = ((p / 100.0) * count as f64).floor() as usize;
        let clamped = idx.min(count - 1);
        (samples[clamped] * 1000.0).round() / 1000.0
    };

    LatencyHistogram {
        count,
        p50_ms: percentile(50.0),
        p95_ms: percentile(95.0),
        p99_ms: percentile(99.0),
        max_ms: (samples[count - 1] * 1000.0).round() / 1000.0,
        avg_ms: ((sum / count as f64) * 1000.0).round() / 1000.0,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScenarioResult {
    pub scenario_name: String,
    pub duration_ms: f64,
    pub total_frames: usize,
    pub fps: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
    pub avg_ms: f64,
    pub dropped_frames: usize,
}

pub struct MetricsTracker {
    frame_durations: Vec<f64>,
}

impl MetricsTracker {
    pub fn new() -> Self {
        Self {
            frame_durations: Vec::with_capacity(2048),
        }
    }

    pub fn record_frame_ms(&mut self, dt_ms: f64) {
        self.frame_durations.push(dt_ms);
    }

    pub fn finish(&mut self, scenario_name: &str, total_duration_ms: f64) -> ScenarioResult {
        let mut sorted = self.frame_durations.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let count = sorted.len();

        let percentile = |p: f64| -> f64 {
            if count == 0 {
                return 0.0;
            }
            let idx = ((p / 100.0) * count as f64).floor() as usize;
            let clamped = idx.min(count - 1);
            (sorted[clamped] * 100.0).round() / 100.0
        };

        let dropped = self.frame_durations.iter().filter(|&&t| t > 20.0).count();
        let sum: f64 = self.frame_durations.iter().sum();
        let avg = if count > 0 { sum / count as f64 } else { 0.0 };
        let fps = if total_duration_ms > 0.0 {
            (count as f64) / (total_duration_ms / 1000.0)
        } else {
            0.0
        };

        ScenarioResult {
            scenario_name: scenario_name.to_string(),
            duration_ms: (total_duration_ms * 100.0).round() / 100.0,
            total_frames: count,
            fps: (fps * 10.0).round() / 10.0,
            p50_ms: percentile(50.0),
            p95_ms: percentile(95.0),
            p99_ms: percentile(99.0),
            max_ms: if count > 0 {
                (sorted[count - 1] * 100.0).round() / 100.0
            } else {
                0.0
            },
            avg_ms: (avg * 100.0).round() / 100.0,
            dropped_frames: dropped,
        }
    }
}

#[repr(C)]
struct ProcessMemoryCounters {
    cb: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
}

#[cfg(target_os = "windows")]
#[link(name = "psapi")]
extern "system" {
    fn GetCurrentProcess() -> *mut std::ffi::c_void;
    fn K32GetProcessMemoryInfo(
        process: *mut std::ffi::c_void,
        ppsmem_counters: *mut ProcessMemoryCounters,
        cb: u32,
    ) -> i32;
}

pub fn get_process_working_set_mb() -> f64 {
    #[cfg(target_os = "windows")]
    unsafe {
        let mut counters: ProcessMemoryCounters = std::mem::zeroed();
        counters.cb = std::mem::size_of::<ProcessMemoryCounters>() as u32;
        if K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) != 0 {
            return ((counters.working_set_size as f64) / (1024.0 * 1024.0) * 100.0).round()
                / 100.0;
        }
    }
    0.0
}
