use chartmelogne_core::{Viewport, ViewportOptions};

#[test]
fn test_coordinate_transforms_map_forward_and_backward_identically() {
    let mut vp = Viewport::new(ViewportOptions::default());
    vp.set_size(1000.0, 500.0, 1.0);
    vp.set_total_bars(500);
    vp.from_index = 100.0;
    vp.bars_visible = 50.0;
    vp.y_min = 1000.0;
    vp.y_max = 2000.0;

    let test_bar = 125.5;
    let screen_x = vp.bar_index_to_screen_px(test_bar);
    assert_eq!(vp.screen_px_to_bar_index(screen_x), test_bar);

    let test_price = 1450.25;
    let screen_y = vp.price_to_screen_px(test_price);
    assert_eq!((vp.screen_px_to_price(screen_y) * 100.0).round() / 100.0, test_price);
}

#[test]
fn test_zoom_anchors_at_the_cursor_position() {
    let mut vp = Viewport::new(ViewportOptions::default());
    vp.set_size(1000.0, 500.0, 1.0);
    vp.set_total_bars(1000);
    vp.from_index = 100.0;
    vp.bars_visible = 100.0;

    let cursor_px = 400.0;
    let bar_at_cursor_before = vp.screen_px_to_bar_index(cursor_px);

    // Zoom in (fewer bars visible)
    vp.zoom_at(cursor_px, 0.8);

    let bar_at_cursor_after = vp.screen_px_to_bar_index(cursor_px);
    assert!(
        (bar_at_cursor_before - bar_at_cursor_after).abs() < 1e-6,
        "Bar at cursor should stay fixed during zoom: before={}, after={}",
        bar_at_cursor_before,
        bar_at_cursor_after
    );
}

#[test]
fn test_clamping_enforces_min_bars_visible_and_right_padding_limits() {
    let mut vp = Viewport::new(ViewportOptions {
        min_bars_visible: Some(10.0),
        right_padding_ratio: Some(0.5),
        y_padding_ratio: None,
    });
    vp.set_size(1000.0, 500.0, 1.0);
    vp.set_total_bars(100);

    // Try zooming way in beyond min_bars_visible
    vp.zoom_at(500.0, 0.0001);
    assert_eq!(vp.bars_visible, 10.0);

    // Try panning past right padding
    vp.pan_by_pixels(-100000.0);
    let max_allowed = 100.0 - 10.0 + 10.0 * 0.5; // totalBars - barsVisible + padding
    assert!(vp.from_index <= max_allowed + 1e-6);
}

#[test]
fn test_update_auto_y_applies_padding_ratio() {
    let mut vp = Viewport::new(ViewportOptions {
        y_padding_ratio: Some(0.05),
        ..Default::default()
    });
    vp.update_auto_y(100.0, 200.0, true);

    let expected_pad = 5.0;
    assert_eq!(vp.y_min, 100.0 - expected_pad);
    assert_eq!(vp.y_max, 200.0 + expected_pad);
}
