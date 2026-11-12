use chartmelogne_core::types::ChartType;
use chartmelogne_slint::{run_chart_app, ChartAppConfig};
use std::env;
use std::str::FromStr;

fn print_help() {
    println!(
        r#"ChartMelogne - High-Performance Financial Chart (Rust + Slint + wgpu 30)

USAGE:
    chartmelogne [OPTIONS]

OPTIONS:
    --allow-zoom, --zoom              Enable zoom interactions (default)
    --no-zoom, --disable-zoom         Disable zoom interactions

    --show-axes, --axes               Show price and time axes (default)
    --no-axes, --hide-axes            Hide price and time axes

    --chart-type <TYPE>               Set chart type: candlestick (default)

    --show-volume, --volume           Show volume histogram (default)
    --no-volume, --hide-volume        Hide volume histogram

    --show-indicators, --indicators   Enable indicator display (default: false)
    --no-indicators                   Disable indicator display

    -h, --help                        Print help information
"#
    );
}

fn parse_args() -> ChartAppConfig {
    let mut config = ChartAppConfig::default();
    let args: Vec<String> = env::args().skip(1).collect();
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            "--allow-zoom" | "--zoom" => {
                config.allow_zoom = true;
            }
            "--no-zoom" | "--disable-zoom" => {
                config.allow_zoom = false;
            }
            arg if arg.starts_with("--allow-zoom=") => {
                let val = arg.trim_start_matches("--allow-zoom=");
                config.allow_zoom = val.parse().unwrap_or(true);
            }
            "--show-axes" | "--axes" => {
                config.show_axes = true;
            }
            "--no-axes" | "--hide-axes" => {
                config.show_axes = false;
            }
            arg if arg.starts_with("--show-axes=") => {
                let val = arg.trim_start_matches("--show-axes=");
                config.show_axes = val.parse().unwrap_or(true);
            }
            "--chart-type" | "--type" => {
                if i + 1 < args.len() {
                    i += 1;
                    match ChartType::from_str(&args[i]) {
                        Ok(ct) => config.chart_type = ct,
                        Err(e) => {
                            eprintln!("Error: {e}");
                            std::process::exit(1);
                        }
                    }
                } else {
                    eprintln!("Error: --chart-type requires a value (e.g. candlestick)");
                    std::process::exit(1);
                }
            }
            arg if arg.starts_with("--chart-type=") => {
                let val = arg.trim_start_matches("--chart-type=");
                match ChartType::from_str(val) {
                    Ok(ct) => config.chart_type = ct,
                    Err(e) => {
                        eprintln!("Error: {e}");
                        std::process::exit(1);
                    }
                }
            }
            "--show-volume" | "--volume" => {
                config.show_volume = true;
            }
            "--no-volume" | "--hide-volume" => {
                config.show_volume = false;
            }
            arg if arg.starts_with("--show-volume=") => {
                let val = arg.trim_start_matches("--show-volume=");
                config.show_volume = val.parse().unwrap_or(true);
            }
            "--show-indicators" | "--indicators" => {
                config.show_indicators = true;
            }
            "--no-indicators" => {
                config.show_indicators = false;
            }
            arg if arg.starts_with("--show-indicators=") => {
                let val = arg.trim_start_matches("--show-indicators=");
                config.show_indicators = val.parse().unwrap_or(false);
            }
            other => {
                eprintln!("Warning: unrecognized argument '{other}'. Use --help for available options.");
            }
        }
        i += 1;
    }

    config
}

fn main() -> Result<(), slint::PlatformError> {
    let config = parse_args();
    println!("Launching ChartMelogne (Rust + Slint + wgpu 30)...");
    println!(
        "Options: allow_zoom={}, show_axes={}, chart_type={}, show_volume={}, show_indicators={}",
        config.allow_zoom, config.show_axes, config.chart_type, config.show_volume, config.show_indicators
    );
    run_chart_app(config)
}
