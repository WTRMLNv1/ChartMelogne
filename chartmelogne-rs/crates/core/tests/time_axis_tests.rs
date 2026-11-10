use chartmelogne_core::types::TimeZoomLevel;
use chartmelogne_core::{TimeAxis, TimeAxisOptions};

#[test]
fn test_detects_day_boundaries_for_375_bar_indian_market_sessions() {
    let time_axis = TimeAxis::new(TimeAxisOptions {
        utc_offset_minutes: Some(330),
        ..Default::default()
    });

    // Construct 2 consecutive trading days: Day 1 (375 bars) and Day 2 (375 bars)
    // Day 1: 2026-01-05 09:15 IST = 03:45 UTC = 1767584700000 ms
    // let's compute UTC epoch ms for 2026-01-05 03:45:00 UTC
    // 2026 is not a leap year. Days since 1970-01-01:
    // 56 years = 56 * 365 + 14 (leap years: 1972..2024 = 14) = 20454 days + 4 days in Jan = 20458 days
    // 20458 * 86400 + 3 * 3600 + 45 * 60 = 1767584700 seconds -> 1767584700000 ms
    let day1_start_utc = 1767584700000.0;
    // Day 2: 2026-01-06 09:15 IST = 03:45 UTC (24 hours = 86,400,000 ms later)
    let day2_start_utc = day1_start_utc + 86_400_000.0;

    let mut times = vec![0.0; 750];
    for i in 0..375 {
        times[i] = day1_start_utc + (i as f64) * 60_000.0;
    }
    for i in 0..375 {
        times[375 + i] = day2_start_utc + (i as f64) * 60_000.0;
    }

    assert_eq!(time_axis.is_day_boundary(&times, 0, None), true, "Bar 0 is day start");
    assert_eq!(time_axis.is_day_boundary(&times, 1, None), false, "Bar 1 is intraday");
    assert_eq!(time_axis.is_day_boundary(&times, 374, None), false, "Bar 374 is end of day 1");
    assert_eq!(time_axis.is_day_boundary(&times, 375, None), true, "Bar 375 is start of day 2");
    assert_eq!(time_axis.is_day_boundary(&times, 376, None), false, "Bar 376 is intraday day 2");
}

#[test]
fn test_detects_day_boundaries_for_360_bar_sessions_and_partial_half_days() {
    let time_axis = TimeAxis::new(TimeAxisOptions {
        utc_offset_minutes: Some(330),
        ..Default::default()
    });

    // Day 1 (360 bars)
    let day1_start = 1773114300000.0;
    // Day 2 (partial session: 60 bars)
    let day2_start = day1_start + 86_400_000.0;

    let mut times = vec![0.0; 420];
    for i in 0..360 {
        times[i] = day1_start + (i as f64) * 60_000.0;
    }
    for i in 0..60 {
        times[360 + i] = day2_start + (i as f64) * 60_000.0;
    }

    assert_eq!(time_axis.is_day_boundary(&times, 0, None), true);
    assert_eq!(time_axis.is_day_boundary(&times, 359, None), false);
    assert_eq!(time_axis.is_day_boundary(&times, 360, None), true, "Partial day start detected");
    assert_eq!(time_axis.is_day_boundary(&times, 419, None), false);
}

#[test]
fn test_adapts_label_format_to_zoom_levels() {
    let mut time_axis = TimeAxis::new(TimeAxisOptions {
        utc_offset_minutes: Some(330),
        ..Default::default()
    });

    let day1_start_utc = 1767584700000.0; // 2026-01-05 09:15 IST
    let times = vec![
        day1_start_utc,                     // 09:15 IST (Day start)
        day1_start_utc + 45.0 * 60_000.0,   // 10:00 IST (Intraday)
    ];

    // Intraday
    assert_eq!(time_axis.format_label(&times, 0, TimeZoomLevel::Intraday), "5 Jan");
    assert_eq!(time_axis.format_label(&times, 1, TimeZoomLevel::Intraday), "10:00");

    // Daily
    assert_eq!(time_axis.format_label(&times, 0, TimeZoomLevel::Daily), "Jan '26");

    // Monthly
    assert_eq!(time_axis.format_label(&times, 0, TimeZoomLevel::Monthly), "Jan 2026");
}

#[test]
fn test_generates_valid_ticks_across_visible_range_with_minimum_spacing() {
    let mut time_axis = TimeAxis::new(TimeAxisOptions {
        utc_offset_minutes: Some(330),
        min_tick_spacing_px: Some(80.0),
    });
    let start_ms = 1767584700000.0;
    let count = 375;
    let mut times = vec![0.0; count];
    for i in 0..count {
        times[i] = start_ms + (i as f64) * 60_000.0;
    }

    let ticks = time_axis.generate_ticks(&times, count, 0.0, 100.0, 800.0, None, None);
    assert!(ticks.len() >= 2, "Expected at least 2 ticks, got {}", ticks.len());

    // Monotonic bar_index
    for i in 1..ticks.len() {
        assert!(ticks[i].bar_index > ticks[i - 1].bar_index);
    }
}
