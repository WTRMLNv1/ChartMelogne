use crate::format::to_fixed;
use crate::types::PriceTick;

pub struct PriceTickOptions {
    pub price_decimals: Option<usize>,
    pub tick_size: Option<f64>,
    pub target_spacing_px: Option<f64>, // default 50px
}

impl Default for PriceTickOptions {
    fn default() -> Self {
        Self {
            price_decimals: None,
            tick_size: None,
            target_spacing_px: Some(50.0),
        }
    }
}

/// Computes the "nice number" step in {1, 2, 5} x 10^k closest to raw_step.
pub fn get_nice_price_step(raw_step: f64) -> f64 {
    if raw_step <= 0.0 || !raw_step.is_finite() {
        return 1.0;
    }

    let exp = raw_step.log10().floor();
    let frac = raw_step / 10.0f64.powf(exp);

    let nice_frac = if frac < 1.5 {
        1.0
    } else if frac < 3.5 {
        2.0
    } else if frac < 7.5 {
        5.0
    } else {
        10.0
    };

    nice_frac * 10.0f64.powf(exp)
}

/// Derives the number of decimal places needed to represent the step.
pub fn get_precision(step: f64) -> usize {
    if step >= 1.0 {
        return 2;
    }
    let s = step.to_string();
    if let Some(dot) = s.find('.') {
        s.len() - dot - 1
    } else {
        0
    }
}

/// Generates monotonic, evenly-spaced price ticks within [y_min, y_max].
pub fn generate_price_ticks(
    y_min: f64,
    y_max: f64,
    height_px: f64,
    options: &PriceTickOptions,
) -> Vec<PriceTick> {
    if !y_min.is_finite() || !y_max.is_finite() || y_max <= y_min || height_px <= 0.0 {
        return Vec::new();
    }

    let target_spacing = options.target_spacing_px.unwrap_or(50.0);
    let target_count = 2.0f64.max((height_px / target_spacing).floor());
    let raw_step = (y_max - y_min) / target_count;
    let step = get_nice_price_step(raw_step);

    let decimals = options
        .price_decimals
        .unwrap_or_else(|| get_precision(step));

    let start = (y_min / step).ceil() * step;
    let mut ticks = Vec::new();

    let max_iterations = 200;
    let mut current = start;
    let mut iter = 0;

    let eps = step * 1e-6;

    while current <= y_max + eps && iter < max_iterations {
        let fixed_str = to_fixed(current, decimals);
        let rounded_price: f64 = fixed_str.parse().unwrap_or(current);
        let normalized_price = if rounded_price.abs() < eps {
            0.0
        } else {
            rounded_price
        };

        ticks.push(PriceTick {
            price: normalized_price,
            label: to_fixed(normalized_price, decimals),
        });

        current += step;
        iter += 1;
    }

    ticks
}
