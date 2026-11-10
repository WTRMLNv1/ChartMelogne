/// Formats a floating-point number with a fixed number of decimal places,
/// strictly matching ECMAScript `Number.prototype.toFixed`.
///
/// In JS ECMA-262, for `x >= 0`, ties round up towards +infinity
/// (`n / 10^f - x` close to zero, picking the larger integer `n` on tie).
pub fn to_fixed(val: f64, decimals: usize) -> String {
    if val.is_nan() {
        return "NaN".to_string();
    }
    if val.is_infinite() {
        return if val < 0.0 {
            "-Infinity".to_string()
        } else {
            "Infinity".to_string()
        };
    }

    let is_negative = val < 0.0;
    let abs_val = if is_negative { -val } else { val };

    if abs_val >= 1e21 {
        // Fall back for extreme values
        return format!("{:.precision$}", val, precision = decimals);
    }

    if abs_val == 0.0 {
        if decimals == 0 {
            return "0".to_string();
        }
        let mut s = String::with_capacity(2 + decimals);
        s.push_str("0.");
        for _ in 0..decimals {
            s.push('0');
        }
        return s;
    }

    // Extract IEEE-754 binary64 components
    let bits = abs_val.to_bits();
    let exp_bits = ((bits >> 52) & 0x7FF) as i32;
    let mantissa_bits = bits & 0x000F_FFFF_FFFF_FFFF;

    let (m, e) = if exp_bits == 0 {
        // Subnormal
        (mantissa_bits as u128, -1022 - 52)
    } else {
        // Normal
        ((mantissa_bits | (1 << 52)) as u128, exp_bits - 1023 - 52)
    };

    // Calculate 5^decimals
    let mut pow5: u128 = 1;
    for _ in 0..decimals {
        pow5 = match pow5.checked_mul(5) {
            Some(v) => v,
            None => return format!("{:.precision$}", val, precision = decimals),
        };
    }

    let num = match m.checked_mul(pow5) {
        Some(v) => v,
        None => return format!("{:.precision$}", val, precision = decimals),
    };

    let p = e + decimals as i32;
    let n: u128 = if p >= 0 {
        if p >= 128 {
            return format!("{:.precision$}", val, precision = decimals);
        }
        match num.checked_shl(p as u32) {
            Some(v) => v,
            None => return format!("{:.precision$}", val, precision = decimals),
        }
    } else {
        let shift = (-p) as u32;
        if shift >= 128 {
            0
        } else {
            let half = 1u128 << (shift - 1);
            (num + half) >> shift
        }
    };

    let n_str = n.to_string();
    let mut result = String::with_capacity(n_str.len() + decimals + 2);
    if is_negative {
        result.push('-');
    }

    if decimals == 0 {
        result.push_str(&n_str);
    } else if n_str.len() <= decimals {
        result.push_str("0.");
        for _ in 0..(decimals - n_str.len()) {
            result.push('0');
        }
        result.push_str(&n_str);
    } else {
        let dot_pos = n_str.len() - decimals;
        result.push_str(&n_str[..dot_pos]);
        result.push('.');
        result.push_str(&n_str[dot_pos..]);
    }

    result
}

/// Formats a volume integer with comma grouping (e.g. 1,234,567).
pub fn format_volume(volume: u32) -> String {
    let s = volume.to_string();
    let len = s.len();
    if len <= 3 {
        return s;
    }
    let mut out = String::with_capacity(len + len / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_fixed_exact_ties() {
        assert_eq!(to_fixed(0.125, 2), "0.13");
        assert_eq!(to_fixed(2.5, 0), "3");
        assert_eq!(to_fixed(1.005, 2), "1.00");
        assert_eq!(to_fixed(0.5, 0), "1");
        assert_eq!(to_fixed(1.5, 0), "2");
        assert_eq!(to_fixed(2.675, 2), "2.67");
    }

    #[test]
    fn test_format_volume() {
        assert_eq!(format_volume(0), "0");
        assert_eq!(format_volume(123), "123");
        assert_eq!(format_volume(1234), "1,234");
        assert_eq!(format_volume(1234567), "1,234,567");
    }
}
