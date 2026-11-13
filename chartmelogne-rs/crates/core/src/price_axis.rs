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

/// Derives the number of decimal places needed to represent the step without heap allocation.
pub fn get_precision(step: f64) -> usize {
    if step >= 1.0 || !step.is_finite() || step <= 0.0 {
        return 2;
    }
    let mut p = 0usize;
    let mut val = step;
    while p < 10 && (val - val.round()).abs() > 1e-5 {
        val *= 10.0;
        p += 1;
    }
    p
}

/// Generates monotonic, evenly-spaced price ticks within [y_min, y_max] into a reusable Vec.
pub fn generate_price_ticks_into(
    y_min: f64,
    y_max: f64,
    height_px: f64,
    options: &PriceTickOptions,
    ticks: &mut Vec<PriceTick>,
) {
    ticks.clear();
    if !y_min.is_finite() || !y_max.is_finite() || y_max <= y_min || height_px <= 0.0 {
        return;
    }

    let target_spacing = options.target_spacing_px.unwrap_or(50.0);
    let target_count = 2.0f64.max((height_px / target_spacing).floor());
    let raw_step = (y_max - y_min) / target_count;
    let step = get_nice_price_step(raw_step);

    let decimals = options
        .price_decimals
        .unwrap_or_else(|| get_precision(step));

    let start = (y_min / step).ceil() * step;
    let max_iterations = 200;
    let mut current = start;
    let mut iter = 0;

    let eps = step * 1e-6;
    let factor = 10.0f64.powi(decimals as i32);

    while current <= y_max + eps && iter < max_iterations {
        let rounded_price = (current * factor).round() / factor;
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
}

/// Generates monotonic, evenly-spaced price ticks within [y_min, y_max].
pub fn generate_price_ticks(
    y_min: f64,
    y_max: f64,
    height_px: f64,
    options: &PriceTickOptions,
) -> Vec<PriceTick> {
    let mut ticks = Vec::new();
    generate_price_ticks_into(y_min, y_max, height_px, options, &mut ticks);
    ticks
}
