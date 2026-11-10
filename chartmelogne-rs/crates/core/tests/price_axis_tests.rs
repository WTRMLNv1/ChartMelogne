use chartmelogne_core::{generate_price_ticks, get_nice_price_step, PriceTickOptions};

#[test]
fn test_nice_steps_follow_1_2_5_x_10_pow_k_pattern() {
    assert_eq!(get_nice_price_step(0.0008), 0.001);
    assert_eq!(get_nice_price_step(0.0014), 0.001);
    assert_eq!(get_nice_price_step(0.0028), 0.002);
    assert_eq!(get_nice_price_step(0.006), 0.005);
    assert_eq!(get_nice_price_step(1.2), 1.0);
    assert_eq!(get_nice_price_step(3.1), 2.0);
    assert_eq!(get_nice_price_step(4.8), 5.0);
    assert_eq!(get_nice_price_step(8.9), 10.0);
    assert_eq!(get_nice_price_step(23.0), 20.0);
    assert_eq!(get_nice_price_step(78.0), 100.0);
}

#[test]
fn test_generates_monotonic_evenly_spaced_ticks_for_ranges() {
    let test_ranges = [
        (0.0001, 0.0008, 400.0),
        (1.12, 1.19, 500.0),
        (450.0, 550.0, 600.0),
        (18500.0, 19200.0, 700.0),
        (1_000_000.0, 8_000_000.0, 800.0),
    ];

    for (min, max, height) in test_ranges {
        let ticks = generate_price_ticks(
            min,
            max,
            height,
            &PriceTickOptions {
                target_spacing_px: Some(50.0),
                ..Default::default()
            },
        );
        assert!(
            ticks.len() >= 2,
            "Expected at least 2 ticks for [{}, {}], got {}",
            min,
            max,
            ticks.len()
        );

        let expected_count = (height / 50.0).floor() as usize;
        assert!(
            ticks.len() <= expected_count * 2 + 2,
            "Tick count {} out of bounds for height {}",
            ticks.len(),
            height
        );

        let mut prev = ticks[0].price;
        let step = if ticks.len() > 1 {
            ticks[1].price - ticks[0].price
        } else {
            0.0
        };
        assert!(step > 0.0, "Step must be positive");

        for t in &ticks[1..] {
            let cur = t.price;
            assert!(
                cur > prev,
                "Ticks must be strictly increasing: {} > {}",
                cur,
                prev
            );
            let current_diff = cur - prev;
            assert!(
                (current_diff - step).abs() < step * 1e-4,
                "Ticks must be evenly spaced: diff={}, step={}",
                current_diff,
                step
            );
            prev = cur;
        }
    }
}

#[test]
fn test_formats_decimals_cleanly_based_on_step_size() {
    let ticks = generate_price_ticks(10.1, 10.5, 400.0, &PriceTickOptions::default());
    for t in ticks {
        let parsed: f64 = t.label.parse().unwrap();
        assert_eq!(parsed, t.price);
    }
}
