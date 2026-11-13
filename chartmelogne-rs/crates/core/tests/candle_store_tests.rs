use chartmelogne_core::candle_store::validate_candle_data;
use chartmelogne_core::synthetic::{generate_synthetic_candles, SyntheticDataOptions};
use chartmelogne_core::types::{
    Bar, ChartMarker, HostIndicator, MarkerDirection, DEFAULT_MARKER_DOWN_COLOR,
    DEFAULT_MARKER_UP_COLOR,
};
use chartmelogne_core::CandleStore;

#[test]
fn test_validates_synthetic_data_successfully() {
    let data = generate_synthetic_candles(SyntheticDataOptions {
        count: 10_000,
        seed: 123,
        ..Default::default()
    });
    let err = validate_candle_data(&data, data.time.len());
    assert_eq!(err, None, "Generated synthetic candles should be valid");
}

#[test]
fn test_detects_nan_values_in_ohlc() {
    let mut data = generate_synthetic_candles(SyntheticDataOptions {
        count: 10,
        ..Default::default()
    });
    data.high[3] = f64::NAN;
    let err = validate_candle_data(&data, 10);
    assert!(err.is_some());
    assert_eq!(err.unwrap().index, 3);
}

#[test]
fn test_detects_invalid_high_lt_max_open_close() {
    let mut data = generate_synthetic_candles(SyntheticDataOptions {
        count: 10,
        ..Default::default()
    });
    data.high[2] = data.open[2].min(data.close[2]) - 1.0;
    let err = validate_candle_data(&data, 10);
    assert!(err.is_some());
    assert_eq!(err.unwrap().index, 2);
}

#[test]
fn test_detects_non_monotonic_timestamps() {
    let mut data = generate_synthetic_candles(SyntheticDataOptions {
        count: 10,
        ..Default::default()
    });
    data.time[4] = data.time[3];
    let err = validate_candle_data(&data, 10);
    assert!(err.is_some());
    assert_eq!(err.unwrap().index, 4);
}

#[test]
fn test_set_data_correctly_populates_store_and_packed_ohlc_buffer() {
    let mut store = CandleStore::new(100, 330);
    let data = generate_synthetic_candles(SyntheticDataOptions {
        count: 500,
        start_price: 100.0,
        ..Default::default()
    });
    store.set_data(&data, true).unwrap();

    assert_eq!(store.count(), 500);
    assert!(store.capacity() >= 500);
    assert_eq!(store.base_price(), data.close[0]);

    // Check packed 16-byte OHLC buffer (4 floats per bar: open, high, low, close)
    let base = store.base_price();
    assert_eq!(store.ohlc[0], (data.open[0] - base) as f32);
    assert_eq!(store.ohlc[1], (data.high[0] - base) as f32);
    assert_eq!(store.ohlc[2], (data.low[0] - base) as f32);
    assert_eq!(store.ohlc[3], (data.close[0] - base) as f32);

    // Check last element packed
    let last_idx = 499;
    let offset = last_idx * 4;
    assert_eq!(store.ohlc[offset], (data.open[last_idx] - base) as f32);
    assert_eq!(store.ohlc[offset + 1], (data.high[last_idx] - base) as f32);
    assert_eq!(store.ohlc[offset + 2], (data.low[last_idx] - base) as f32);
    assert_eq!(store.ohlc[offset + 3], (data.close[last_idx] - base) as f32);
}

#[test]
fn test_append_and_update_last_mutate_ohlc_and_scratch_instance_in_o1() {
    let mut store = CandleStore::new(2, 330);
    store.append(Bar {
        time: 1000.0,
        open: 100.0,
        high: 105.0,
        low: 95.0,
        close: 102.0,
        volume: 10,
        flags: None,
    });
    assert_eq!(store.count(), 1);
    assert_eq!(store.base_price(), 102.0);
    assert_eq!(store.get_open(0) as f32, 100.0f32);
    assert_eq!(store.get_high(0) as f32, 105.0f32);

    store.update_last(Bar {
        time: 1000.0,
        open: 100.0,
        high: 108.0,
        low: 95.0,
        close: 107.0,
        volume: 15,
        flags: None,
    });
    assert_eq!(store.count(), 1);
    assert_eq!(store.get_high(0) as f32, 108.0f32);
    assert_eq!(store.get_close(0) as f32, 107.0f32);
    assert_eq!(store.scratch_instance[1], 108.0f32 - 102.0f32);

    store.append(Bar {
        time: 2000.0,
        open: 107.0,
        high: 110.0,
        low: 106.0,
        close: 109.0,
        volume: 20,
        flags: None,
    });
    assert_eq!(store.count(), 2);
    assert_eq!(store.get_close(1) as f32, 109.0f32);
    assert_eq!(store.scratch_instance[3], 109.0f32 - 102.0f32);
}

#[test]
fn test_get_visible_min_max_returns_bounds_for_visible_range() {
    let mut store = CandleStore::new(10, 330);
    let data = generate_synthetic_candles(SyntheticDataOptions {
        count: 50,
        ..Default::default()
    });
    store.set_data(&data, true).unwrap();

    let (min, max) = store.get_visible_min_max(10.0, 20.0);
    let mut expected_min = f64::INFINITY;
    let mut expected_max = f64::NEG_INFINITY;
    for i in 10..=20 {
        let l = store.get_low(i);
        let h = store.get_high(i);
        if l < expected_min {
            expected_min = l;
        }
        if h > expected_max {
            expected_max = h;
        }
    }
    assert_eq!(min, expected_min);
    assert_eq!(max, expected_max);
}

#[test]
fn test_crosshair_and_tooltip_ohlc_precision_rs_130000() {
    let base_prices = [100.0, 2500.0, 25000.0, 130000.0];

    for base_price in base_prices {
        const COUNT: usize = 1000;
        let data = generate_synthetic_candles(SyntheticDataOptions {
            count: COUNT,
            start_price: base_price,
            volatility: 0.002,
            seed: 42,
            ..Default::default()
        });

        let mut store = CandleStore::new(COUNT, 330);
        store.set_data(&data, true).unwrap();

        for i in 0..COUNT {
            let o = store.get_open(i);
            let h = store.get_high(i);
            let l = store.get_low(i);
            let c = store.get_close(i);

            // Sub-paisa resolution: max allowed error < 0.05
            assert!(
                (o - data.open[i]).abs() < 0.05,
                "Open price error exceeds tolerance at base_price={}, idx={}",
                base_price,
                i
            );
            assert!(
                (h - data.high[i]).abs() < 0.05,
                "High price error exceeds tolerance at base_price={}, idx={}",
                base_price,
                i
            );
            assert!(
                (l - data.low[i]).abs() < 0.05,
                "Low price error exceeds tolerance at base_price={}, idx={}",
                base_price,
                i
            );
            assert!(
                (c - data.close[i]).abs() < 0.05,
                "Close price error exceeds tolerance at base_price={}, idx={}",
                base_price,
                i
            );

            let bar = store.get_bar(i);
            assert_eq!(bar.time, data.time[i]);
            assert_eq!(bar.volume, data.volume[i]);
        }
    }
}

#[test]
fn test_block_accelerated_get_visible_min_max_matches_naive_scan() {
    fn naive_scan(store: &CandleStore, from_index: f64, to_index: f64) -> (f64, f64) {
        if store.count() == 0 {
            return (0.0, 1.0);
        }
        let start = 0.max(from_index.floor() as usize);
        let end = store.count().min(to_index.ceil() as usize);
        if start >= end {
            let idx = start.clamp(0, store.count() - 1);
            return (store.get_low(idx), store.get_high(idx));
        }
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for i in start..end {
            if (store.flags[i] & 1) != 0 {
                continue;
            }
            let l = store.get_low(i);
            let h = store.get_high(i);
            if l < min {
                min = l;
            }
            if h > max {
                max = h;
            }
        }
        (min, max)
    }

    const COUNT: usize = 10_000;
    let mut store = CandleStore::new(COUNT, 330);
    let data = generate_synthetic_candles(SyntheticDataOptions {
        count: COUNT,
        seed: 777,
        ..Default::default()
    });
    store.set_data(&data, true).unwrap();

    let mut test_ranges = vec![
        (0.0, COUNT as f64, "Full dataset range"),
        (5.0, 45.0, "Tiny sub-block range inside block 0"),
        (250.0, 260.0, "Range crossing block boundary (255/256)"),
        (100.0, 2000.0, "Multi-block range with head and tail partials"),
        (512.0, 1024.0, "Exact multiple of block size (aligned)"),
        (513.0, 1023.0, "Unaligned multi-block range"),
    ];

    let mut seed: u64 = 42;
    for _ in 0..50 {
        seed = (seed.wrapping_mul(16807)) % 2147483647;
        let r0 = ((seed - 1) as f64) / 2147483646.0;
        seed = (seed.wrapping_mul(16807)) % 2147483647;
        let r1 = ((seed - 1) as f64) / 2147483646.0;

        let idx_a = (r0 * (COUNT as f64)).floor();
        let idx_b = (r1 * (COUNT as f64)).floor();
        let from = idx_a.min(idx_b);
        let to = idx_a.max(idx_b) + 1.0;
        test_ranges.push((from, to, "Random range"));
    }

    for (from, to, desc) in test_ranges {
        let naive = naive_scan(&store, from, to);
        let fast = store.get_visible_min_max(from, to);

        assert_eq!(fast.0, naive.0, "Min mismatch for {}", desc);
        assert_eq!(fast.1, naive.1, "Max mismatch for {}", desc);
    }

    // Verify after append
    let last_time = store.time[COUNT - 1];
    store.append(Bar {
        time: last_time + 60_000.0,
        open: 100.0,
        high: 99999.0, // new all-time high
        low: 0.01,    // new all-time low
        close: 100.0,
        volume: 100,
        flags: None,
    });
    let after_append = store.get_visible_min_max(0.0, store.count() as f64);
    assert!((after_append.1 - 99999.0).abs() < 0.05);
    assert!((after_append.0 - 0.01).abs() < 0.05);

    // Verify after update_last
    let last_idx = store.count() - 1;
    store.update_last(Bar {
        time: store.time[last_idx],
        open: 100.0,
        high: 88888.0, // updated downward
        low: 0.5,
        close: 100.0,
        volume: 100,
        flags: None,
    });
    let after_update = store.get_visible_min_max(0.0, store.count() as f64);
    assert!((after_update.1 - 88888.0).abs() < 0.05);
    assert!((after_update.0 - 0.5).abs() < 0.05);
}

#[test]
fn test_host_indicator_registration_and_relative_conversion() {
    let mut store = CandleStore::new(100, 0);
    let data = generate_synthetic_candles(SyntheticDataOptions {
        count: 100,
        start_price: 500.0,
        seed: 42,
        ..Default::default()
    });
    store.set_data(&data, true).unwrap();

    let sma10 = chartmelogne_core::synthetic::compute_sma(&data.close, 10);
    assert_eq!(sma10.len(), 100);
    assert!(sma10[0].is_none());
    assert!(sma10[8].is_none());
    assert!(sma10[9].is_some());

    let ind = HostIndicator::new("SMA 10", [1.0, 0.5, 0.0, 1.0], sma10);
    store.add_indicator(ind.clone());
    assert_eq!(store.indicators.len(), 1);
    assert_eq!(store.indicators[0].name, "SMA 10");

    let rel = store.get_indicator_relative_f32(&ind);
    assert_eq!(rel.len(), 100);
    assert!(rel[0].is_nan());
    assert!(rel[8].is_nan());
    assert!(!rel[9].is_nan());

    let expected_rel = (ind.values[9].unwrap() - store.base_price()) as f32;
    assert!((rel[9] - expected_rel).abs() < 1e-5);

    // Remove indicator
    assert!(store.remove_indicator("SMA 10"));
    assert_eq!(store.indicators.len(), 0);
}

#[test]
fn test_marker_registration_and_gpu_instance_conversion() {
    let mut store = CandleStore::new(50, 0);
    let data = generate_synthetic_candles(SyntheticDataOptions {
        count: 50,
        start_price: 1000.0,
        seed: 99,
        ..Default::default()
    });
    store.set_data(&data, true).unwrap();
    let base = store.base_price();

    // Up marker (Buy) defaults to green, placed below candle LOW
    let m_up = ChartMarker::up(10).with_text("BUY");
    assert_eq!(m_up.direction, MarkerDirection::Up);
    assert_eq!(m_up.effective_color(), DEFAULT_MARKER_UP_COLOR);

    // Second Up marker on candle 10 (should stack below the first)
    let m_up_2 = ChartMarker::up(10).with_text("BUY 2");

    // Down marker (Sell) defaults to red, placed above candle HIGH
    let m_down = ChartMarker::down(25).with_text("SELL");
    assert_eq!(m_down.direction, MarkerDirection::Down);
    assert_eq!(m_down.effective_color(), DEFAULT_MARKER_DOWN_COLOR);

    // Custom color marker
    let custom_color = [0.0, 1.0, 1.0, 1.0];
    let m_custom = ChartMarker::up(40).with_color(custom_color);
    assert_eq!(m_custom.effective_color(), custom_color);

    // Time-based marker (auto-mapped to closest candle index)
    let time_15 = data.time[15];
    let m_time = ChartMarker::up_at_time(time_15);
    assert_eq!(m_time.candle_index, None);
    assert_eq!(m_time.time, Some(time_15));
    assert_eq!(store.resolve_marker_bar_index(&m_time), 15);

    // Time-based marker between candle 20 and 21 (closer to 20)
    let time_between = data.time[20] + 5.0; // step is 60s
    let m_time_approx = ChartMarker::down_at_time(time_between);
    assert_eq!(store.resolve_marker_bar_index(&m_time_approx), 20);

    store.add_marker(m_up);
    store.add_marker(m_up_2);
    store.add_marker(m_down);
    store.add_marker(m_custom);
    store.add_marker(m_time);

    assert_eq!(store.markers().len(), 5);

    let gpu_instances = store.get_markers_gpu_instances();
    assert_eq!(gpu_instances.len(), 5);

    // Verify first marker (Up / Green placed at candle 10 LOW, stack 0)
    let low_10 = store.get_low(10);
    assert_eq!(gpu_instances[0].x, 10.0);
    assert!((gpu_instances[0].y_rel - (low_10 - base) as f32).abs() < 1e-5);
    assert_eq!(gpu_instances[0].dir, 0.0); // 0.0 is Up
    assert_eq!(gpu_instances[0].stack_index, 0.0);
    assert_eq!(gpu_instances[0].color, DEFAULT_MARKER_UP_COLOR);

    // Verify second marker (Up on candle 10 LOW, stacked at index 1)
    assert_eq!(gpu_instances[1].x, 10.0);
    assert!((gpu_instances[1].y_rel - (low_10 - base) as f32).abs() < 1e-5);
    assert_eq!(gpu_instances[1].dir, 0.0);
    assert_eq!(gpu_instances[1].stack_index, 1.0); // Stacked!
    assert_eq!(gpu_instances[1].color, DEFAULT_MARKER_UP_COLOR);

    // Verify third marker (Down / Red placed at candle 25 HIGH, stack 0)
    let high_25 = store.get_high(25);
    assert_eq!(gpu_instances[2].x, 25.0);
    assert!((gpu_instances[2].y_rel - (high_25 - base) as f32).abs() < 1e-5);
    assert_eq!(gpu_instances[2].dir, 1.0); // 1.0 is Down
    assert_eq!(gpu_instances[2].stack_index, 0.0);
    assert_eq!(gpu_instances[2].color, DEFAULT_MARKER_DOWN_COLOR);

    // Verify fourth marker (Custom color)
    assert_eq!(gpu_instances[3].color, custom_color);

    // Verify fifth marker (Auto-mapped from timestamp to bar 15, anchored at bar 15 LOW)
    let low_15 = store.get_low(15);
    assert_eq!(gpu_instances[4].x, 15.0);
    assert!((gpu_instances[4].y_rel - (low_15 - base) as f32).abs() < 1e-5);
    assert_eq!(gpu_instances[4].dir, 0.0);
    assert_eq!(gpu_instances[4].stack_index, 0.0);
    assert_eq!(gpu_instances[4].color, DEFAULT_MARKER_UP_COLOR);

    // Clear markers
    store.clear_markers();
    assert_eq!(store.markers().len(), 0);
    assert_eq!(store.get_markers_gpu_instances().len(), 0);
}
