use std::fs;
use chartmelogne_core::format::{format_volume, to_fixed};
use chartmelogne_core::price_axis::generate_price_ticks;
use chartmelogne_core::synthetic::{generate_synthetic_candles, Mulberry32, SyntheticDataOptions};
use chartmelogne_core::types::{CandleData, PriceTick, TimeTick, ViewportState};
use chartmelogne_core::{CandleStore, TimeAxis, TimeAxisOptions, Viewport, ViewportOptions};
use serde::Deserialize;

#[derive(Deserialize)]
struct GoldenCandlesFile {
    time: Vec<f64>,
    open: Vec<f64>,
    high: Vec<f64>,
    low: Vec<f64>,
    close: Vec<f64>,
    volume: Vec<u32>,
}

#[derive(Deserialize)]
struct ZoomLevelFixture {
    #[serde(rename = "fromIndex")]
    from_index: f64,
    #[serde(rename = "barsVisible")]
    bars_visible: f64,
    #[serde(rename = "viewportState")]
    viewport_state: ViewportState,
    #[serde(rename = "visibleRange")]
    visible_range: VisibleRangeFixture,
    #[serde(rename = "minMax")]
    min_max: MinMaxFixture,
    #[serde(rename = "priceTicks")]
    price_ticks: Vec<PriceTick>,
    #[serde(rename = "timeTicks")]
    time_ticks: Vec<TimeTick>,
}

#[derive(Deserialize)]
struct VisibleRangeFixture {
    first: usize,
    last: usize,
}

#[derive(Deserialize)]
struct MinMaxFixture {
    min: f64,
    max: f64,
}

#[derive(Deserialize)]
struct GoldenPrngFormattingFile {
    prng_seed: u32,
    prng_first_64: Vec<f64>,
    first_3_bars: Vec<BarFixture>,
    last_3_bars: Vec<BarFixture>,
    to_fixed: Vec<ToFixedFixture>,
    crosshair_time_badges: Vec<TimeBadgeFixture>,
    hud_ohlcv: Vec<HudOhlcvFixture>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct BarFixture {
    time: f64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: u32,
    flags: Option<u8>,
}

#[derive(Deserialize)]
struct ToFixedFixture {
    value: f64,
    formatted: Vec<String>,
}

#[derive(Deserialize)]
struct TimeBadgeFixture {
    #[serde(rename = "barIndex")]
    bar_index: usize,
    #[serde(rename = "timeMs")]
    time_ms: f64,
    label: String,
}

#[derive(Deserialize)]
struct HudOhlcvFixture {
    #[serde(rename = "barIndex")]
    bar_index: usize,
    open: String,
    high: String,
    low: String,
    close: String,
    volume: String,
}

#[test]
fn test_golden_prng_and_formatting() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../bench/fixtures/golden-prng-formatting.json"
    );
    let content = fs::read_to_string(path).expect("Failed to read golden-prng-formatting.json");
    let fixture: GoldenPrngFormattingFile =
        serde_json::from_str(&content).expect("Failed to parse golden-prng-formatting.json");

    // 1. Verify first 64 PRNG outputs within 1 ulp (1e-15)
    let mut rng = Mulberry32::new(fixture.prng_seed);
    for (i, &expected) in fixture.prng_first_64.iter().enumerate() {
        let actual = rng.next_f64();
        assert!(
            (actual - expected).abs() <= 1e-15,
            "PRNG output mismatch at index {}: got {}, expected {}",
            i, actual, expected
        );
    }

    // 2. Verify synthetic candles and CandleStore match first 3 and last 3 bars
    let candles = generate_synthetic_candles(SyntheticDataOptions {
        count: 10_000,
        seed: fixture.prng_seed,
        start_price: 2500.0,
        ..Default::default()
    });
    let mut store = CandleStore::new(10_000, 330);
    store.set_data(&candles, true).unwrap();

    for (i, expected) in fixture.first_3_bars.iter().enumerate() {
        let bar = store.get_bar(i);
        assert_eq!(bar.time, expected.time, "Bar {} time mismatch", i);
        assert!((bar.open - expected.open).abs() < 1e-6, "Bar {} open mismatch: got {}, expected {}", i, bar.open, expected.open);
        assert!((bar.high - expected.high).abs() < 1e-6, "Bar {} high mismatch: got {}, expected {}", i, bar.high, expected.high);
        assert!((bar.low - expected.low).abs() < 1e-6, "Bar {} low mismatch: got {}, expected {}", i, bar.low, expected.low);
        assert!((bar.close - expected.close).abs() < 1e-6, "Bar {} close mismatch: got {}, expected {}", i, bar.close, expected.close);
        assert_eq!(bar.volume, expected.volume, "Bar {} volume mismatch", i);
    }

    for (k, expected) in fixture.last_3_bars.iter().enumerate() {
        let i = 10_000 - 3 + k;
        let bar = store.get_bar(i);
        assert_eq!(bar.time, expected.time, "Bar {} time mismatch", i);
        assert!((bar.open - expected.open).abs() < 1e-6, "Bar {} open mismatch: got {}, expected {}", i, bar.open, expected.open);
        assert!((bar.high - expected.high).abs() < 1e-6, "Bar {} high mismatch: got {}, expected {}", i, bar.high, expected.high);
        assert!((bar.low - expected.low).abs() < 1e-6, "Bar {} low mismatch: got {}, expected {}", i, bar.low, expected.low);
        assert!((bar.close - expected.close).abs() < 1e-6, "Bar {} close mismatch: got {}, expected {}", i, bar.close, expected.close);
        assert_eq!(bar.volume, expected.volume, "Bar {} volume mismatch", i);
    }

    // 3. Verify to_fixed against all 20 values across decimals 0..4
    for tf in &fixture.to_fixed {
        for (decimals, expected) in tf.formatted.iter().enumerate() {
            let actual = to_fixed(tf.value, decimals);
            assert_eq!(
                &actual, expected,
                "to_fixed mismatch for value {} with {} decimals: got {}, expected {}",
                tf.value, decimals, actual, expected
            );
        }
    }

    // 4. Verify crosshair time badges
    let time_axis = TimeAxis::new(TimeAxisOptions {
        utc_offset_minutes: Some(330),
        ..Default::default()
    });
    for badge in &fixture.crosshair_time_badges {
        let parts = time_axis.get_local_parts(badge.time_ms);
        let label = format!("{} {:02}:{:02}", parts.day, parts.hours, parts.minutes);
        assert_eq!(
            label, badge.label,
            "Crosshair time badge mismatch for bar index {}: got {}, expected {}",
            badge.bar_index, label, badge.label
        );
    }

    // 5. Verify HUD OHLCV
    let mut store = CandleStore::new(10_000, 330);
    store.set_data(&candles, true).unwrap();

    for hud in &fixture.hud_ohlcv {
        let i = hud.bar_index;
        let o_str = to_fixed(store.get_open(i), 2);
        let h_str = to_fixed(store.get_high(i), 2);
        let l_str = to_fixed(store.get_low(i), 2);
        let c_str = to_fixed(store.get_close(i), 2);
        let v_str = format_volume(store.volume[i]);

        assert_eq!(o_str, hud.open, "HUD open mismatch for bar {}", i);
        assert_eq!(h_str, hud.high, "HUD high mismatch for bar {}", i);
        assert_eq!(l_str, hud.low, "HUD low mismatch for bar {}", i);
        assert_eq!(c_str, hud.close, "HUD close mismatch for bar {}", i);
        assert_eq!(v_str, hud.volume, "HUD volume mismatch for bar {}", i);
    }
}

#[test]
fn test_golden_viewport_and_ticks_across_all_four_zoom_levels() {
    let candles_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../bench/fixtures/golden-candles.json"
    );
    let candles_content = fs::read_to_string(candles_path).expect("Failed to read golden-candles.json");
    let golden_candles: GoldenCandlesFile =
        serde_json::from_str(&candles_content).expect("Failed to parse golden-candles.json");

    let candle_data = CandleData {
        time: golden_candles.time,
        open: golden_candles.open,
        high: golden_candles.high,
        low: golden_candles.low,
        close: golden_candles.close,
        volume: golden_candles.volume,
        flags: None,
    };

    let mut store = CandleStore::new(10_000, 330);
    store.set_data(&candle_data, true).unwrap();

    let ticks_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../bench/fixtures/golden-viewport-ticks.json"
    );
    let ticks_content = fs::read_to_string(ticks_path).expect("Failed to read golden-viewport-ticks.json");
    let golden_levels: std::collections::HashMap<String, ZoomLevelFixture> =
        serde_json::from_str(&ticks_content).expect("Failed to parse golden-viewport-ticks.json");

    let level_names = [
        "zoomed_in_intraday",
        "medium_daily",
        "wide_multiweek",
        "full_zoom_out",
    ];

    let plot_width = 935.0;
    let plot_height = 572.0;

    for name in level_names {
        let expected = golden_levels.get(name).unwrap_or_else(|| panic!("Missing fixture for {}", name));

        let mut vp = Viewport::new(ViewportOptions::default());
        vp.set_size(plot_width, plot_height, 1.0);
        vp.set_total_bars(store.count());
        vp.set_visible_range(expected.from_index, expected.from_index + expected.bars_visible);

        let (min, max) = store.get_visible_min_max(vp.from_index, vp.from_index + vp.bars_visible);
        vp.update_auto_y(min, max, true);

        // 1. Verify Viewport state
        assert_eq!(vp.from_index, expected.viewport_state.from_index, "{}: from_index mismatch", name);
        assert_eq!(vp.bars_visible, expected.viewport_state.bars_visible, "{}: bars_visible mismatch", name);
        assert!(
            (vp.y_min - expected.viewport_state.y_min).abs() < 1e-9,
            "{}: y_min mismatch (got {}, expected {})",
            name,
            vp.y_min,
            expected.viewport_state.y_min
        );
        assert!(
            (vp.y_max - expected.viewport_state.y_max).abs() < 1e-9,
            "{}: y_max mismatch (got {}, expected {})",
            name,
            vp.y_max,
            expected.viewport_state.y_max
        );

        // 2. Verify visible range
        let first_visible = 0.max((vp.from_index - 2.0).floor() as usize);
        let last_visible = (store.count() - 1).min((vp.from_index + vp.bars_visible + 2.0).ceil() as usize);
        assert_eq!(first_visible, expected.visible_range.first, "{}: first visible mismatch", name);
        assert_eq!(last_visible, expected.visible_range.last, "{}: last visible mismatch", name);

        // 3. Verify min/max
        assert!(
            (min - expected.min_max.min).abs() < 1e-9,
            "{}: min mismatch (got {}, expected {})",
            name,
            min,
            expected.min_max.min
        );
        assert!(
            (max - expected.min_max.max).abs() < 1e-9,
            "{}: max mismatch (got {}, expected {})",
            name,
            max,
            expected.min_max.max
        );

        // 4. Verify price ticks
        let price_ticks = generate_price_ticks(
            vp.y_min,
            vp.y_max,
            plot_height,
            &Default::default(),
        );
        assert_eq!(
            price_ticks.len(),
            expected.price_ticks.len(),
            "{}: price tick count mismatch",
            name
        );
        for (i, pt) in price_ticks.iter().enumerate() {
            let exp_pt = &expected.price_ticks[i];
            assert!(
                (pt.price - exp_pt.price).abs() < 1e-6,
                "{}: price tick {} price mismatch: got {}, expected {}",
                name,
                i,
                pt.price,
                exp_pt.price
            );
            assert_eq!(
                pt.label, exp_pt.label,
                "{}: price tick {} label mismatch: got {}, expected {}",
                name, i, pt.label, exp_pt.label
            );
        }

        // 5. Verify time ticks
        let mut time_axis = TimeAxis::new(TimeAxisOptions {
            utc_offset_minutes: Some(330),
            ..Default::default()
        });
        let time_ticks = time_axis.generate_ticks(
            &store.time,
            store.count(),
            vp.from_index,
            vp.bars_visible,
            plot_width,
            Some(&store.day_starts[..store.day_count]),
            Some(store.day_count),
        );

        assert_eq!(
            time_ticks.len(),
            expected.time_ticks.len(),
            "{}: time tick count mismatch (got {}, expected {})",
            name,
            time_ticks.len(),
            expected.time_ticks.len()
        );
        for (i, tt) in time_ticks.iter().enumerate() {
            let exp_tt = &expected.time_ticks[i];
            assert_eq!(
                tt.bar_index, exp_tt.bar_index,
                "{}: time tick {} bar_index mismatch",
                name, i
            );
            assert_eq!(
                tt.time_ms, exp_tt.time_ms,
                "{}: time tick {} time_ms mismatch",
                name, i
            );
            assert_eq!(
                tt.label, exp_tt.label,
                "{}: time tick {} label mismatch: got {}, expected {}",
                name, i, tt.label, exp_tt.label
            );
            assert_eq!(
                tt.is_major, exp_tt.is_major,
                "{}: time tick {} is_major mismatch",
                name, i
            );
        }
    }
}
