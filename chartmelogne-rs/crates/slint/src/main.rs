use chartmelogne_core::types::ChartType;
use chartmelogne_slint::{run_chart_app, run_dashboard_app, ChartAppConfig};
use std::env;
use std::str::FromStr;

fn print_help() {
    println!(
        r#"ChartMelogne - High-Performance Financial Chart (Rust + Slint + wgpu 30)

USAGE:
    chartmelogne [OPTIONS]

OPTIONS:
    --dashboard                       Launch the multi-panel trading dashboard window
    --allow-zoom, --zoom              Enable zoom interactions (default)
    --no-zoom, --disable-zoom         Disable zoom interactions

    --allow-pan, --pan                Enable pan interactions (default)
    --no-pan, --disable-pan           Disable pan interactions

    --show-crosshair, --crosshair     Show cursor crosshair when hovering (default)
    --no-crosshair, --hide-crosshair  Hide cursor crosshair when hovering

    --show-axes, --axes               Show price and time axes (default)
    --no-axes, --hide-axes            Hide price and time axes

    --chart-type <TYPE>               Set chart type: candlestick (default), line, area
    --candlestick, --candles          Shorthand for --chart-type candlestick
    --line                            Shorthand for --chart-type line
    --area                            Shorthand for --chart-type area

    --show-volume, --volume           Show volume histogram (default)
    --no-volume, --hide-volume        Hide volume histogram

    --show-indicators, --indicators   Enable indicator display (default: false)
    --no-indicators                   Disable indicator display

    --bg, --bg-color <HEX>            Set chart background color (e.g. '#131722', '#000000')
    --up-color, --green-color <HEX>   Set bullish / up candle color (e.g. '#089981', '#00ff00')
    --down-color, --red-color <HEX>   Set bearish / down candle color (e.g. '#ef5350', '#ff0000')

    --bars, --visible-bars <N>        Initial number of visible bars (e.g. 100, 300)
    --from <INDEX>                    Initial starting bar index (e.g. 0, 500)

    -h, --help                        Print help information
"#
    );
}

fn parse_hex_color(hex: &str) -> Result<[f32; 4], String> {
    let s = hex.trim().trim_start_matches('#');
    match s.len() {
        3 => {
            let r = u8::from_str_radix(&s[0..1].repeat(2), 16).map_err(|e| e.to_string())?;
            let g = u8::from_str_radix(&s[1..2].repeat(2), 16).map_err(|e| e.to_string())?;
            let b = u8::from_str_radix(&s[2..3].repeat(2), 16).map_err(|e| e.to_string())?;
            Ok([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0])
        }
        6 => {
            let r = u8::from_str_radix(&s[0..2], 16).map_err(|e| e.to_string())?;
            let g = u8::from_str_radix(&s[2..4], 16).map_err(|e| e.to_string())?;
            let b = u8::from_str_radix(&s[4..6], 16).map_err(|e| e.to_string())?;
            Ok([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0])
        }
        8 => {
            let r = u8::from_str_radix(&s[0..2], 16).map_err(|e| e.to_string())?;
            let g = u8::from_str_radix(&s[2..4], 16).map_err(|e| e.to_string())?;
            let b = u8::from_str_radix(&s[4..6], 16).map_err(|e| e.to_string())?;
            let a = u8::from_str_radix(&s[6..8], 16).map_err(|e| e.to_string())?;
            Ok([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0])
        }
        _ => Err(format!(
            "Invalid hex color '{hex}', expected format #RGB, #RRGGBB, or #RRGGBBAA"
        )),
    }
}

fn parse_args() -> (ChartAppConfig, bool) {
    parse_args_iter(env::args().skip(1))
}

fn parse_args_iter<I: IntoIterator<Item = String>>(iter: I) -> (ChartAppConfig, bool) {
    let mut config = ChartAppConfig::default();
    let mut dashboard = false;
    let args: Vec<String> = iter.into_iter().collect();
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            "--dashboard" => {
                dashboard = true;
            }
            "--no-zoom" | "--disable-zoom" => {
                config.allow_zoom = false;
            }
            arg if arg.starts_with("--allow-zoom=") => {
                let val = arg.trim_start_matches("--allow-zoom=");
                config.allow_zoom = val.parse().unwrap_or(true);
            }
            "--allow-pan" | "--pan" => {
                config.allow_pan = true;
            }
            "--no-pan" | "--disable-pan" => {
                config.allow_pan = false;
            }
            arg if arg.starts_with("--allow-pan=") => {
                let val = arg.trim_start_matches("--allow-pan=");
                config.allow_pan = val.parse().unwrap_or(true);
            }
            "--show-crosshair" | "--crosshair" => {
                config.show_crosshair = true;
            }
            "--no-crosshair" | "--hide-crosshair" => {
                config.show_crosshair = false;
            }
            arg if arg.starts_with("--show-crosshair=") => {
                let val = arg.trim_start_matches("--show-crosshair=");
                config.show_crosshair = val.parse().unwrap_or(true);
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
                    eprintln!("Error: --chart-type requires a value (e.g. candlestick, line, area)");
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
            "--candlestick" | "--candles" => {
                config.chart_type = ChartType::Candlestick;
            }
            "--line" => {
                config.chart_type = ChartType::Line;
            }
            "--area" => {
                config.chart_type = ChartType::Area;
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
            "--bg" | "--bg-color" | "--background" | "--background-color" => {
                if i + 1 < args.len() {
                    i += 1;
                    match parse_hex_color(&args[i]) {
                        Ok(col) => config.background_color = col,
                        Err(e) => {
                            eprintln!("Error: {e}");
                            std::process::exit(1);
                        }
                    }
                } else {
                    eprintln!("Error: --bg requires a hex color (e.g. #131722)");
                    std::process::exit(1);
                }
            }
            arg if arg.starts_with("--bg=") || arg.starts_with("--bg-color=") || arg.starts_with("--background=") || arg.starts_with("--background-color=") => {
                let val = arg.split_once('=').map(|(_, v)| v).unwrap_or("");
                match parse_hex_color(val) {
                    Ok(col) => config.background_color = col,
                    Err(e) => {
                        eprintln!("Error: {e}");
                        std::process::exit(1);
                    }
                }
            }
            "--up" | "--up-color" | "--green" | "--green-color" => {
                if i + 1 < args.len() {
                    i += 1;
                    match parse_hex_color(&args[i]) {
                        Ok(col) => config.up_color = col,
                        Err(e) => {
                            eprintln!("Error: {e}");
                            std::process::exit(1);
                        }
                    }
                } else {
                    eprintln!("Error: --up-color requires a hex color (e.g. #089981)");
                    std::process::exit(1);
                }
            }
            arg if arg.starts_with("--up=") || arg.starts_with("--up-color=") || arg.starts_with("--green=") || arg.starts_with("--green-color=") => {
                let val = arg.split_once('=').map(|(_, v)| v).unwrap_or("");
                match parse_hex_color(val) {
                    Ok(col) => config.up_color = col,
                    Err(e) => {
                        eprintln!("Error: {e}");
                        std::process::exit(1);
                    }
                }
            }
            "--down" | "--down-color" | "--red" | "--red-color" => {
                if i + 1 < args.len() {
                    i += 1;
                    match parse_hex_color(&args[i]) {
                        Ok(col) => config.down_color = col,
                        Err(e) => {
                            eprintln!("Error: {e}");
                            std::process::exit(1);
                        }
                    }
                } else {
                    eprintln!("Error: --down-color requires a hex color (e.g. #ef5350)");
                    std::process::exit(1);
                }
            }
            arg if arg.starts_with("--down=") || arg.starts_with("--down-color=") || arg.starts_with("--red=") || arg.starts_with("--red-color=") => {
                let val = arg.split_once('=').map(|(_, v)| v).unwrap_or("");
                match parse_hex_color(val) {
                    Ok(col) => config.down_color = col,
                    Err(e) => {
                        eprintln!("Error: {e}");
                        std::process::exit(1);
                    }
                }
            }
            "--bars" | "--visible-bars" => {
                if i + 1 < args.len() {
                    i += 1;
                    match args[i].parse::<f64>() {
                        Ok(b) if b > 0.0 => config.initial_bars_visible = Some(b),
                        _ => {
                            eprintln!("Error: --bars requires a positive number");
                            std::process::exit(1);
                        }
                    }
                } else {
                    eprintln!("Error: --bars requires a positive number");
                    std::process::exit(1);
                }
            }
            arg if arg.starts_with("--bars=") || arg.starts_with("--visible-bars=") => {
                let val = arg.split_once('=').map(|(_, v)| v).unwrap_or("");
                match val.parse::<f64>() {
                    Ok(b) if b > 0.0 => config.initial_bars_visible = Some(b),
                    _ => {
                        eprintln!("Error: --bars requires a positive number");
                        std::process::exit(1);
                    }
                }
            }
            "--from" => {
                if i + 1 < args.len() {
                    i += 1;
                    match args[i].parse::<f64>() {
                        Ok(f) if f >= 0.0 => config.initial_from = Some(f),
                        _ => {
                            eprintln!("Error: --from requires a non-negative number");
                            std::process::exit(1);
                        }
                    }
                } else {
                    eprintln!("Error: --from requires a non-negative number");
                    std::process::exit(1);
                }
            }
            arg if arg.starts_with("--from=") => {
                let val = arg.split_once('=').map(|(_, v)| v).unwrap_or("");
                match val.parse::<f64>() {
                    Ok(f) if f >= 0.0 => config.initial_from = Some(f),
                    _ => {
                        eprintln!("Error: --from requires a non-negative number");
                        std::process::exit(1);
                    }
                }
            }
            other => {
                eprintln!("Warning: unrecognized argument '{other}'. Use --help for available options.");
            }
        }
        i += 1;
    }

    (config, dashboard)
}

fn main() -> Result<(), slint::PlatformError> {
    let (config, dashboard) = parse_args();
    if dashboard {
        println!("Launching ChartMelogne Dashboard (Embedded ChartWidget)...");
        run_dashboard_app(config)
    } else {
        println!("Launching ChartMelogne (Rust + Slint + wgpu 30)...");
        println!(
            "Options: allow_zoom={}, allow_pan={}, show_crosshair={}, show_axes={}, chart_type={}, show_volume={}, show_indicators={}, bg={:?}, up={:?}, down={:?}",
            config.allow_zoom, config.allow_pan, config.show_crosshair, config.show_axes, config.chart_type, config.show_volume, config.show_indicators,
            config.background_color, config.up_color, config.down_color
        );
        run_chart_app(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_color() {
        // 3-char hex
        let c = parse_hex_color("#fff").unwrap();
        assert_eq!(c, [1.0, 1.0, 1.0, 1.0]);

        // 6-char hex
        let c = parse_hex_color("#000000").unwrap();
        assert_eq!(c, [0.0, 0.0, 0.0, 1.0]);

        let c = parse_hex_color("131722").unwrap();
        assert!((c[0] - 19.0 / 255.0).abs() < 1e-4);
        assert!((c[1] - 23.0 / 255.0).abs() < 1e-4);
        assert!((c[2] - 34.0 / 255.0).abs() < 1e-4);
        assert_eq!(c[3], 1.0);

        // 8-char hex with alpha
        let c = parse_hex_color("#ff000080").unwrap();
        assert_eq!(c[0], 1.0);
        assert_eq!(c[1], 0.0);
        assert_eq!(c[2], 0.0);
        assert!((c[3] - 128.0 / 255.0).abs() < 1e-2);

        // Invalid
        assert!(parse_hex_color("not_a_color").is_err());
        assert!(parse_hex_color("#12").is_err());
    }

    #[test]
    fn test_crosshair_cli_args() {
        let (config, _) = parse_args_iter(vec!["--no-crosshair".to_string()]);
        assert!(!config.show_crosshair);

        let (config, _) = parse_args_iter(vec!["--hide-crosshair".to_string()]);
        assert!(!config.show_crosshair);

        let (config, _) = parse_args_iter(vec!["--show-crosshair=false".to_string()]);
        assert!(!config.show_crosshair);

        let (config, _) = parse_args_iter(vec!["--crosshair".to_string()]);
        assert!(config.show_crosshair);
    }

    #[test]
    fn test_bars_and_from_cli_args() {
        let (config, _) = parse_args_iter(vec!["--bars".to_string(), "150".to_string(), "--from".to_string(), "50".to_string()]);
        assert_eq!(config.initial_bars_visible, Some(150.0));
        assert_eq!(config.initial_from, Some(50.0));

        let (config, _) = parse_args_iter(vec!["--visible-bars=200".to_string(), "--from=100".to_string()]);
        assert_eq!(config.initial_bars_visible, Some(200.0));
        assert_eq!(config.initial_from, Some(100.0));
    }
}
