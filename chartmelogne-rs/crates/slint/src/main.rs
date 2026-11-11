use chartmelogne_slint::{run_chart_app, ChartAppConfig};

fn main() -> Result<(), slint::PlatformError> {
    println!("Launching ChartMelogne (Rust + Slint + wgpu 30)...");
    let config = ChartAppConfig::default();
    run_chart_app(config)
}
