pub use chartmelogne_core::{
    candle_store::CandleStore,
    format::{format_volume, to_fixed},
    price_axis::{generate_price_ticks_into, PriceTickOptions},
    synthetic::{compute_ema, compute_sma, generate_synthetic_candles, SyntheticDataOptions},
    time_axis::{TimeAxis, TimeAxisOptions},
    types::{CandleData, ChartType, HostIndicator, ViewportMode},
    viewport::Viewport,
};
pub use chartmelogne_render::{ChartRenderer, ViewParams};
pub use slint::wgpu_30::wgpu;
pub use slint::{ComponentHandle, ModelRc, VecModel};

slint::include_modules!();

pub struct ChartAppConfig {
    pub candle_data: CandleData,
    pub utc_offset_minutes: i32,
    pub allow_zoom: bool,
    pub show_axes: bool,
    pub chart_type: ChartType,
    pub show_volume: bool,
    pub show_indicators: bool,
    pub indicators: Vec<HostIndicator>,
}

impl Default for ChartAppConfig {
    fn default() -> Self {
        let options = SyntheticDataOptions {
            count: 450_000,
            start_price: 1000.0,
            seed: 1337,
            ..Default::default()
        };
        let candle_data = generate_synthetic_candles(options);

        // Host provides pre-calculated indicators
        let ema20 = compute_ema(&candle_data.close, 20);
        let sma50 = compute_sma(&candle_data.close, 50);

        let indicators = vec![
            HostIndicator::new("EMA 20", [0.941, 0.725, 0.043, 1.0], ema20),
            HostIndicator::new("SMA 50", [0.612, 0.153, 0.690, 1.0], sma50),
        ];

        Self {
            candle_data,
            utc_offset_minutes: 330,
            allow_zoom: true,
            show_axes: true,
            chart_type: ChartType::Candlestick,
            show_volume: true,
            show_indicators: true,
            indicators,
        }
    }
}

pub struct ChartAppState {
    pub store: CandleStore,
    pub viewport: Viewport,
    pub time_axis: TimeAxis,
    pub last_drag_x: f32,
    pub last_drag_y: f32,
    pub is_dragging: bool,
    pub crosshair_active: bool,
    pub crosshair_screen_x: f32,
    pub crosshair_screen_y: f32,
    pub allow_zoom: bool,
    pub show_axes: bool,
    pub chart_type: ChartType,
    pub show_volume: bool,
    pub show_indicators: bool,
    pub chart_dirty: bool,
}

/// Macro that connects full chart interaction (panning, wheel zoom, tick generation,
/// cursor crosshair, host-provided indicators, and wgpu 30 rendering loop) to any Slint window
/// or component exposing the ChartWidget property and callback interface.
#[macro_export]
macro_rules! attach_chart_controller {
    ($ui_expr:expr, $config_expr:expr) => {{
        use std::cell::RefCell;
        use std::rc::Rc;
        use $crate::{
            format_volume, generate_price_ticks_into, to_fixed,
            CandleStore, ChartAppState, ChartRenderer, IndicatorUi, ModelRc,
            PriceTickOptions, PriceTickUi, TimeAxis, TimeAxisOptions,
            TimeTickUi, VecModel, ViewParams, Viewport, ViewportMode,
        };
        use $crate::wgpu;

        let ui = $ui_expr;
        let config = $config_expr;

        ui.set_allow_zoom(config.allow_zoom);
        ui.set_show_axes(config.show_axes);
        ui.set_chart_type(config.chart_type.to_string().into());
        ui.set_show_volume(config.show_volume);
        ui.set_show_indicators(config.show_indicators);

        let count = config.candle_data.time.len();
        let mut store = CandleStore::new(count, config.utc_offset_minutes);
        store
            .set_data(&config.candle_data, true)
            .expect("CandleStore set_data failed");

        for ind in &config.indicators {
            store.add_indicator(ind.clone());
        }

        let mut viewport = Viewport::new(chartmelogne_core::viewport::ViewportOptions::default());
        viewport.set_total_bars(count);
        let initial_bars_visible = 300.0;
        let initial_from = (count as f64 - initial_bars_visible).max(0.0);
        viewport.set_visible_range(initial_from, initial_from + initial_bars_visible);

        let (min, max) = store.get_visible_min_max(viewport.from_index, viewport.from_index + viewport.bars_visible);
        viewport.update_auto_y(min, max, true);

        let time_axis = TimeAxis::new(TimeAxisOptions {
            utc_offset_minutes: Some(config.utc_offset_minutes),
            ..Default::default()
        });

        let state = Rc::new(RefCell::new(ChartAppState {
            store,
            viewport,
            time_axis,
            last_drag_x: 0.0,
            last_drag_y: 0.0,
            is_dragging: false,
            crosshair_active: false,
            crosshair_screen_x: 0.0,
            crosshair_screen_y: 0.0,
            allow_zoom: config.allow_zoom,
            show_axes: config.show_axes,
            chart_type: config.chart_type,
            show_volume: config.show_volume,
            show_indicators: config.show_indicators,
            chart_dirty: true,
        }));

        let renderer_slot: Rc<RefCell<Option<ChartRenderer>>> = Rc::new(RefCell::new(None));

        // 1. Drag Start
        {
            let st_rc = Rc::clone(&state);
            ui.on_drag_started(move |x, y| {
                let mut st = st_rc.borrow_mut();
                st.is_dragging = true;
                st.last_drag_x = x;
                st.last_drag_y = y;
                st.crosshair_active = true;
                st.crosshair_screen_x = x;
                st.crosshair_screen_y = y;
            });
        }

        // 2. Dragged (Pan)
        {
            let st_rc = Rc::clone(&state);
            let ui_handle = ui.as_weak();
            ui.on_dragged(move |x, y| {
                let mut st = st_rc.borrow_mut();
                if st.is_dragging {
                    let dx = x - st.last_drag_x;
                    let dy = y - st.last_drag_y;
                    st.last_drag_x = x;
                    st.last_drag_y = y;
                    st.crosshair_active = true;
                    st.crosshair_screen_x = x;
                    st.crosshair_screen_y = y;

                    if let Some(ui) = ui_handle.upgrade() {
                        let dpr = ui.window().scale_factor();
                        st.viewport.pan_by_pixels((dx as f64) * (dpr as f64));

                        if st.viewport.y_mode == ViewportMode::Manual {
                            st.viewport.pan_y_by_pixels((dy as f64) * (dpr as f64));
                        }

                        st.chart_dirty = true;
                        ui.window().request_redraw();
                    }
                }
            });
        }

        // 3. Drag Ended
        {
            let st_rc = Rc::clone(&state);
            ui.on_drag_ended(move || {
                let mut st = st_rc.borrow_mut();
                st.is_dragging = false;
            });
        }

        // 4. Scrolled (Zoom)
        {
            let st_rc = Rc::clone(&state);
            let ui_handle = ui.as_weak();
            ui.on_scrolled(move |delta_y, cursor_x| {
                let mut st = st_rc.borrow_mut();
                if !st.allow_zoom {
                    return;
                }
                if let Some(ui) = ui_handle.upgrade() {
                    let dpr = ui.window().scale_factor();
                    let clamped_delta = (delta_y as f64).clamp(-120.0, 120.0);
                    let zoom_factor = (-clamped_delta * 0.0018).exp();
                    let cursor_px = (cursor_x as f64) * (dpr as f64);
                    st.viewport.zoom_at(cursor_px, zoom_factor);
                    st.chart_dirty = true;
                    ui.window().request_redraw();
                }
            });
        }

        // 5. Price Axis Scrolled (Vertical Zoom)
        {
            let st_rc = Rc::clone(&state);
            let ui_handle = ui.as_weak();
            ui.on_price_axis_scrolled(move |delta_y, cursor_y| {
                let mut st = st_rc.borrow_mut();
                if !st.allow_zoom {
                    return;
                }
                if let Some(ui) = ui_handle.upgrade() {
                    let dpr = ui.window().scale_factor();
                    let clamped_delta = (delta_y as f64).clamp(-120.0, 120.0);
                    let zoom_factor = (-clamped_delta * 0.0018).exp();
                    let cursor_py = (cursor_y as f64) * (dpr as f64);

                    st.viewport.y_mode = ViewportMode::Manual;
                    ui.set_auto_y_mode(false);
                    st.viewport.zoom_y_at(cursor_py, zoom_factor);
                    st.chart_dirty = true;
                    ui.window().request_redraw();
                }
            });
        }

        // 6. Toggle Auto Y Callback
        {
            let st_rc = Rc::clone(&state);
            let ui_handle = ui.as_weak();
            ui.on_toggle_auto_y(move || {
                let mut st = st_rc.borrow_mut();
                st.viewport.y_mode = if st.viewport.y_mode == ViewportMode::Auto {
                    ViewportMode::Manual
                } else {
                    ViewportMode::Auto
                };
                if let Some(ui) = ui_handle.upgrade() {
                    let is_auto = st.viewport.y_mode == ViewportMode::Auto;
                    ui.set_auto_y_mode(is_auto);
                    if is_auto {
                        let (min_p, max_p) = st.store.get_visible_min_max(st.viewport.from_index, st.viewport.from_index + st.viewport.bars_visible);
                        st.viewport.update_auto_y(min_p, max_p, true);
                    }
                    st.chart_dirty = true;
                    ui.window().request_redraw();
                }
            });
        }

        // 7. Toggle Allow Zoom Callback
        {
            let st_rc = Rc::clone(&state);
            ui.on_toggle_allow_zoom(move |val| {
                let mut st = st_rc.borrow_mut();
                st.allow_zoom = val;
            });
        }

        // 8. Toggle Show Axes Callback
        {
            let st_rc = Rc::clone(&state);
            let ui_handle = ui.as_weak();
            ui.on_toggle_show_axes(move |val| {
                let mut st = st_rc.borrow_mut();
                st.show_axes = val;
                st.chart_dirty = true;
                if let Some(ui) = ui_handle.upgrade() {
                    ui.window().request_redraw();
                }
            });
        }

        // 9. Toggle Chart Type Callback
        {
            let st_rc = Rc::clone(&state);
            let ui_handle = ui.as_weak();
            ui.on_toggle_chart_type(move |val| {
                let mut st = st_rc.borrow_mut();
                use std::str::FromStr;
                if let Ok(ct) = chartmelogne_core::types::ChartType::from_str(val.as_str()) {
                    st.chart_type = ct;
                    st.chart_dirty = true;
                }
                if let Some(ui) = ui_handle.upgrade() {
                    ui.window().request_redraw();
                }
            });
        }

        // 10. Toggle Show Volume Callback
        {
            let st_rc = Rc::clone(&state);
            let ui_handle = ui.as_weak();
            ui.on_toggle_show_volume(move |val| {
                let mut st = st_rc.borrow_mut();
                st.show_volume = val;
                st.chart_dirty = true;
                if let Some(ui) = ui_handle.upgrade() {
                    ui.window().request_redraw();
                }
            });
        }

        // 11. Toggle Show Indicators Callback
        {
            let st_rc = Rc::clone(&state);
            let ui_handle = ui.as_weak();
            ui.on_toggle_show_indicators(move |val| {
                let mut st = st_rc.borrow_mut();
                st.show_indicators = val;
                st.chart_dirty = true;
                if let Some(ui) = ui_handle.upgrade() {
                    ui.window().request_redraw();
                }
            });
        }

        // 12. Pointer Moved (Crosshair & Hover)
        {
            let st_rc = Rc::clone(&state);
            let ui_handle = ui.as_weak();
            ui.on_pointer_moved(move |x, y| {
                let mut st = st_rc.borrow_mut();
                st.crosshair_active = true;
                st.crosshair_screen_x = x;
                st.crosshair_screen_y = y;

                if let Some(ui) = ui_handle.upgrade() {
                    ui.window().request_redraw();
                }
            });
        }

        // 13. Pointer Left
        {
            let st_rc = Rc::clone(&state);
            let ui_handle = ui.as_weak();
            ui.on_pointer_left(move || {
                let mut st = st_rc.borrow_mut();
                st.crosshair_active = false;
                if let Some(ui) = ui_handle.upgrade() {
                    ui.set_crosshair_active(false);
                    ui.set_hud_active(false);
                    ui.window().request_redraw();
                }
            });
        }

        // --- WGPU RENDERING NOTIFIER ---
        {
            let st_rc = Rc::clone(&state);
            let r_rc = Rc::clone(&renderer_slot);
            let ui_handle = ui.as_weak();

            if let Err(e) = ui.window().set_rendering_notifier(move |rendering_state, graphics_api| {
                let slint::GraphicsAPI::WGPU30 { device, queue, .. } = graphics_api else {
                    return;
                };

                match rendering_state {
                    slint::RenderingState::RenderingSetup => {
                        let mut renderer = ChartRenderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
                        let st = st_rc.borrow();
                        renderer.upload_all(&st.store.ohlc, st.store.count());
                        renderer.upload_volume(&st.store.volume_instances, st.store.count());

                        for ind in &st.store.indicators {
                            let rel_vals = st.store.get_indicator_relative_f32(ind);
                            renderer.upload_indicator(
                                &ind.id,
                                ind.color,
                                ind.line_width,
                                &rel_vals,
                                ind.values.len(),
                            );
                        }

                        let mut r_slot = r_rc.borrow_mut();
                        *r_slot = Some(renderer);
                    }

                    slint::RenderingState::BeforeRendering => {
                        let Some(ui) = ui_handle.upgrade() else { return };

                        let dpr = ui.window().scale_factor();
                        let logical_w = ui.get_plot_width().max(64.0);
                        let logical_h = ui.get_plot_height().max(64.0);
                        let phys_w = ((logical_w as f64) * (dpr as f64)).round() as u32;
                        let phys_h = ((logical_h as f64) * (dpr as f64)).round() as u32;

                        let (view_params, needs_resize, should_render) = {
                            let mut st = st_rc.borrow_mut();
                            let (cur_w, cur_h) = {
                                let r_slot = r_rc.borrow();
                                r_slot.as_ref().map(|r| (r.plot_width(), r.plot_height())).unwrap_or((0, 0))
                            };
                            let needs_resize = cur_w != phys_w || cur_h != phys_h;
                            let should_render = st.chart_dirty || needs_resize;
                            st.chart_dirty = false;

                            let ChartAppState {
                                ref mut store,
                                ref mut viewport,
                                ref mut time_axis,
                                crosshair_active,
                                crosshair_screen_x,
                                crosshair_screen_y,
                                allow_zoom,
                                show_axes,
                                chart_type,
                                show_volume,
                                show_indicators,
                                ..
                            } = *st;

                            if should_render {
                                viewport.set_size(phys_w as f64, phys_h as f64, dpr as f64);
                                let (min_p, max_p) = store.get_visible_min_max(viewport.from_index, viewport.from_index + viewport.bars_visible);
                                viewport.update_auto_y(min_p, max_p, true);
                            }

                            let y_min = viewport.y_min;
                            let y_max = viewport.y_max;
                            let y_span = y_max - y_min;
                            let y_scale = if y_span > 0.0 { (phys_h as f64) / y_span } else { 1.0 };
                            let px_per_bar = viewport.px_per_bar();
                            let from_index = viewport.from_index;
                            let bars_visible = viewport.bars_visible;

                            let mut grid_y = Vec::new();
                            let mut grid_x = Vec::new();

                            // 1 & 2. Generate Ticks and Grid (if show_axes and should_render)
                            if show_axes && should_render {
                                let mut price_ticks = Vec::new();
                                generate_price_ticks_into(y_min, y_max, phys_h as f64, &PriceTickOptions::default(), &mut price_ticks);
                                let mut ui_price_ticks = Vec::with_capacity(price_ticks.len());
                                grid_y.reserve(price_ticks.len());

                                for tick in &price_ticks {
                                    let y_phys = (phys_h as f64) - (tick.price - y_min) * y_scale;
                                    if y_phys >= 0.0 && y_phys <= (phys_h as f64) {
                                        grid_y.push(y_phys as f32);
                                        let y_logical = (y_phys / (dpr as f64)) as f32;
                                        ui_price_ticks.push(PriceTickUi {
                                            y_px: y_logical,
                                            label: tick.label.clone().into(),
                                        });
                                    }
                                }

                                let time_ticks = time_axis.generate_ticks(
                                    &store.time,
                                    store.count(),
                                    from_index,
                                    bars_visible,
                                    phys_w as f64,
                                    Some(&store.day_starts),
                                    Some(store.day_count),
                                );
                                let mut ui_time_ticks = Vec::with_capacity(time_ticks.len());
                                grid_x.reserve(time_ticks.len());

                                for tick in time_ticks {
                                    let x_phys = (tick.bar_index as f64 + 0.5 - from_index) * px_per_bar;
                                    if x_phys >= 0.0 && x_phys <= (phys_w as f64) {
                                        grid_x.push(x_phys as f32);
                                        let x_logical = (x_phys / (dpr as f64)) as f32;
                                        ui_time_ticks.push(TimeTickUi {
                                            x_px: x_logical,
                                            label: tick.label.clone().into(),
                                        });
                                    }
                                }

                                ui.set_price_ticks(ModelRc::new(VecModel::from(ui_price_ticks)));
                                ui.set_time_ticks(ModelRc::new(VecModel::from(ui_time_ticks)));
                            }

                            // 3. Last Price line and badge
                            if show_axes && should_render && store.count() > 0 {
                                let last_idx = store.count() - 1;
                                let last_close = store.get_close(last_idx);
                                let last_open = store.get_open(last_idx);
                                let is_up = last_close >= last_open;

                                let last_y_phys = (phys_h as f64) - (last_close - y_min) * y_scale;
                                let last_y_logical = (last_y_phys / (dpr as f64)) as f32;

                                ui.set_last_price_visible(true);
                                ui.set_last_price_y(last_y_logical);
                                ui.set_last_price_str(to_fixed(last_close, 2).into());
                                ui.set_last_price_color(if is_up {
                                    slint::Color::from_argb_u8(255, 8, 153, 129)
                                } else {
                                    slint::Color::from_argb_u8(255, 242, 54, 69)
                                });
                            } else if !show_axes {
                                ui.set_last_price_visible(false);
                            }

                            // 4. Crosshair, OHLC HUD, and TradingView Style Indicator List
                            let active_idx = if crosshair_active && logical_w > 0.0 && logical_h > 0.0 {
                                let cx_phys = (crosshair_screen_x as f64) * (dpr as f64);
                                let cy_phys = (crosshair_screen_y as f64) * (dpr as f64);

                                if cx_phys >= 0.0 && cx_phys <= (phys_w as f64) && cy_phys >= 0.0 && cy_phys <= (phys_h as f64) {
                                    let hovered_idx = (from_index + cx_phys / px_per_bar - 0.5)
                                        .round()
                                        .max(0.0)
                                        .min((store.count() - 1) as f64) as usize;

                                    let snapped_x_phys = (hovered_idx as f64 + 0.5 - from_index) * px_per_bar;
                                    let snapped_x_logical = (snapped_x_phys / (dpr as f64)) as f32;

                                    let hover_price = y_min + ((phys_h as f64) - cy_phys) / y_scale;
                                    let hover_time = store.time[hovered_idx];
                                    let parts = time_axis.get_local_parts(hover_time);
                                    let time_badge_str = format!("{} {:02}:{:02}", parts.day, parts.hours, parts.minutes);

                                    ui.set_crosshair_active(true);
                                    ui.set_crosshair_x(crosshair_screen_x);
                                    ui.set_crosshair_y(crosshair_screen_y);
                                    ui.set_crosshair_snapped_x(snapped_x_logical);
                                    ui.set_crosshair_price(to_fixed(hover_price, 2).into());
                                    ui.set_crosshair_time(time_badge_str.into());

                                    let o = store.get_open(hovered_idx);
                                    let h = store.get_high(hovered_idx);
                                    let l = store.get_low(hovered_idx);
                                    let c = store.get_close(hovered_idx);
                                    let v = store.volume[hovered_idx];
                                    let is_up = c >= o;

                                    ui.set_hud_active(true);
                                    ui.set_hud_open(to_fixed(o, 2).into());
                                    ui.set_hud_high(to_fixed(h, 2).into());
                                    ui.set_hud_low(to_fixed(l, 2).into());
                                    ui.set_hud_close(to_fixed(c, 2).into());
                                    ui.set_hud_volume(format_volume(v).into());
                                    ui.set_hud_ohlc_color(if is_up {
                                        slint::Color::from_argb_u8(255, 8, 153, 129)
                                    } else {
                                        slint::Color::from_argb_u8(255, 242, 54, 69)
                                    });

                                    Some(hovered_idx)
                                } else {
                                    None
                                }
                            } else {
                                None
                            };

                            // Update TradingView Style Indicator HUD items
                            if show_indicators && !store.indicators.is_empty() {
                                let target_idx = active_idx.unwrap_or_else(|| {
                                    if store.count() > 0 { store.count() - 1 } else { 0 }
                                });
                                let mut ui_indicators = Vec::with_capacity(store.indicators.len());
                                for ind in &store.indicators {
                                    if !ind.visible {
                                        continue;
                                    }
                                    let val_str = match ind.values.get(target_idx) {
                                        Some(Some(v)) => to_fixed(*v, 2),
                                        _ => "n/a".to_string(),
                                    };
                                    let c = slint::Color::from_argb_u8(
                                        (ind.color[3] * 255.0).round() as u8,
                                        (ind.color[0] * 255.0).round() as u8,
                                        (ind.color[1] * 255.0).round() as u8,
                                        (ind.color[2] * 255.0).round() as u8,
                                    );
                                    ui_indicators.push(IndicatorUi {
                                        name: ind.name.clone().into(),
                                        value: val_str.into(),
                                        color: c,
                                    });
                                }
                                ui.set_indicators(ModelRc::new(VecModel::from(ui_indicators)));
                            }

                            let max_vol = if show_volume && should_render {
                                store.get_visible_max_volume(from_index, from_index + bars_visible) as f32
                            } else {
                                1.0
                            };

                            let view_params = ViewParams {
                                from_index: from_index as f32,
                                px_per_bar: px_per_bar as f32,
                                y_min: y_min as f32,
                                y_max: y_max as f32,
                                base_price: store.base_price() as f32,
                                count: store.count(),
                                dpr: dpr as f32,
                                up_color: [0.03, 0.65, 0.55, 1.0],
                                down_color: [0.95, 0.25, 0.33, 1.0],
                                clear_color: Some(wgpu::Color {
                                    r: 0.0745,
                                    g: 0.0902,
                                    b: 0.1333,
                                    a: 1.0,
                                }),
                                grid_color: Some([0.1412, 0.1529, 0.1961, 1.0]),
                                horizontal_grid_y: grid_y,
                                vertical_grid_x: grid_x,
                                show_volume,
                                volume_max: max_vol,
                                volume_height_ratio: 0.20,
                                volume_up_color: [0.03, 0.65, 0.55, 0.35],
                                volume_down_color: [0.95, 0.25, 0.33, 0.35],
                                allow_zoom,
                                show_axes,
                                chart_type,
                                show_indicators,
                            };

                            (view_params, needs_resize, should_render)
                        };

                        let mut r_slot = r_rc.borrow_mut();
                        let Some(ref mut renderer) = *r_slot else { return };

                        if needs_resize {
                            renderer.resize(phys_w, phys_h);
                            match slint::Image::try_from(renderer.texture().clone()) {
                                Ok(img) => ui.set_chart_texture(img),
                                Err(e) => eprintln!("Error creating Slint Image from wgpu::Texture: {:?}", e),
                            }
                        }

                        if should_render {
                            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("Frame Command Encoder"),
                            });
                            renderer.render(&mut encoder, &view_params);
                            queue.submit(Some(encoder.finish()));
                        }
                    }

                    slint::RenderingState::AfterRendering => {}
                    slint::RenderingState::RenderingTeardown => {
                        let mut r_slot = r_rc.borrow_mut();
                        *r_slot = None;
                    }
                    _ => {}
                }
            }) {
                eprintln!("Failed to set rendering notifier: {:?}", e);
            }
        }
    }};
}

pub fn run_chart_app(config: ChartAppConfig) -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    attach_chart_controller!(&ui, &config);
    ui.run()
}

pub fn run_dashboard_app(config: ChartAppConfig) -> Result<(), slint::PlatformError> {
    let ui = DashboardWindow::new()?;
    attach_chart_controller!(&ui, &config);
    ui.run()
}
