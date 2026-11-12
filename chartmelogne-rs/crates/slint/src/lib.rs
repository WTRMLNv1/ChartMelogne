use std::cell::RefCell;
use std::rc::Rc;

use chartmelogne_core::{
    candle_store::CandleStore,
    format::{format_volume, to_fixed},
    price_axis::{generate_price_ticks, PriceTickOptions},
    synthetic::{generate_synthetic_candles, SyntheticDataOptions},
    time_axis::{TimeAxis, TimeAxisOptions},
    types::{CandleData, ChartType, ViewportMode},
    viewport::Viewport,
};
use chartmelogne_render::{ChartRenderer, ViewParams};
use slint::wgpu_30::wgpu;
use slint::{ComponentHandle, ModelRc, VecModel};

slint::include_modules!();

pub struct ChartAppConfig {
    pub candle_data: CandleData,
    pub utc_offset_minutes: i32,
    pub allow_zoom: bool,
    pub show_axes: bool,
    pub chart_type: ChartType,
    pub show_volume: bool,
    pub show_indicators: bool,
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
        Self {
            candle_data,
            utc_offset_minutes: 330,
            allow_zoom: true,
            show_axes: true,
            chart_type: ChartType::Candlestick,
            show_volume: true,
            show_indicators: false,
        }
    }
}

struct ChartAppState {
    store: CandleStore,
    viewport: Viewport,
    time_axis: TimeAxis,
    last_drag_x: f32,
    last_drag_y: f32,
    is_dragging: bool,
    crosshair_active: bool,
    crosshair_screen_x: f32,
    crosshair_screen_y: f32,
    allow_zoom: bool,
    show_axes: bool,
    chart_type: ChartType,
    show_volume: bool,
    show_indicators: bool,
}

pub fn run_chart_app(config: ChartAppConfig) -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;

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
    }));

    let renderer_slot: Rc<RefCell<Option<ChartRenderer>>> = Rc::new(RefCell::new(None));

    // --- EVENT CALLBACKS ---

    // 1. Drag Start
    {
        let st_rc = Rc::clone(&state);
        ui.on_drag_started(move |x, y| {
            let mut st = st_rc.borrow_mut();
            st.is_dragging = true;
            st.last_drag_x = x;
            st.last_drag_y = y;
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

                if let Some(ui) = ui_handle.upgrade() {
                    let dpr = ui.window().scale_factor();
                    st.viewport.pan_by_pixels((dx as f64) * (dpr as f64));
                    if st.viewport.y_mode == ViewportMode::Manual {
                        st.viewport.pan_y_by_pixels((dy as f64) * (dpr as f64));
                    }
                    ui.window().request_redraw();
                }
            }
        });
    }

    // Drag Ended
    {
        let st_rc = Rc::clone(&state);
        ui.on_drag_ended(move || {
            let mut st = st_rc.borrow_mut();
            st.is_dragging = false;
        });
    }

    // 3. Scrolled (Horizontal Zoom)
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
                let clamped_delta = (delta_y as f64).clamp(-100.0, 100.0);
                let zoom_factor = (-clamped_delta * 0.0018).exp();
                let cursor_px = (cursor_x as f64) * (dpr as f64);
                st.viewport.zoom_at(cursor_px, zoom_factor);
                ui.window().request_redraw();
            }
        });
    }

    // 4. Price Axis Scrolled (Vertical Zoom on Y-Axis, turns off Auto mode)
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
                st.viewport.y_mode = ViewportMode::Manual;
                ui.set_auto_y_mode(false);

                let clamped_delta = (delta_y as f64).clamp(-100.0, 100.0);
                let zoom_factor = (-clamped_delta * 0.0018).exp();
                let cursor_py = (cursor_y as f64) * (dpr as f64);
                st.viewport.zoom_y_at(cursor_py, zoom_factor);
                ui.window().request_redraw();
            }
        });
    }

    // 5. Toggle Auto Mode Button
    {
        let st_rc = Rc::clone(&state);
        let ui_handle = ui.as_weak();
        ui.on_toggle_auto_y(move || {
            let mut st = st_rc.borrow_mut();
            if let Some(ui) = ui_handle.upgrade() {
                if st.viewport.y_mode == ViewportMode::Auto {
                    st.viewport.y_mode = ViewportMode::Manual;
                    ui.set_auto_y_mode(false);
                } else {
                    st.viewport.y_mode = ViewportMode::Auto;
                    ui.set_auto_y_mode(true);
                    let (min_p, max_p) = st.store.get_visible_min_max(st.viewport.from_index, st.viewport.from_index + st.viewport.bars_visible);
                    st.viewport.update_auto_y(min_p, max_p, true);
                }
                ui.window().request_redraw();
            }
        });
    }

    // 6. Toggle Allow Zoom Callback
    {
        let st_rc = Rc::clone(&state);
        let ui_handle = ui.as_weak();
        ui.on_toggle_allow_zoom(move |val| {
            let mut st = st_rc.borrow_mut();
            st.allow_zoom = val;
            if let Some(ui) = ui_handle.upgrade() {
                ui.window().request_redraw();
            }
        });
    }

    // 7. Toggle Show Axes Callback
    {
        let st_rc = Rc::clone(&state);
        let ui_handle = ui.as_weak();
        ui.on_toggle_show_axes(move |val| {
            let mut st = st_rc.borrow_mut();
            st.show_axes = val;
            if let Some(ui) = ui_handle.upgrade() {
                ui.window().request_redraw();
            }
        });
    }

    // 8. Toggle Chart Type Callback
    {
        let st_rc = Rc::clone(&state);
        let ui_handle = ui.as_weak();
        ui.on_toggle_chart_type(move |val| {
            let mut st = st_rc.borrow_mut();
            use std::str::FromStr;
            if let Ok(ct) = chartmelogne_core::types::ChartType::from_str(val.as_str()) {
                st.chart_type = ct;
            }
            if let Some(ui) = ui_handle.upgrade() {
                ui.window().request_redraw();
            }
        });
    }

    // 9. Toggle Show Volume Callback
    {
        let st_rc = Rc::clone(&state);
        let ui_handle = ui.as_weak();
        ui.on_toggle_show_volume(move |val| {
            let mut st = st_rc.borrow_mut();
            st.show_volume = val;
            if let Some(ui) = ui_handle.upgrade() {
                ui.window().request_redraw();
            }
        });
    }

    // 10. Toggle Show Indicators Callback
    {
        let st_rc = Rc::clone(&state);
        let ui_handle = ui.as_weak();
        ui.on_toggle_show_indicators(move |val| {
            let mut st = st_rc.borrow_mut();
            st.show_indicators = val;
            if let Some(ui) = ui_handle.upgrade() {
                ui.window().request_redraw();
            }
        });
    }

    // 11. Pointer Moved (Crosshair & Hover)
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

    // 5. Pointer Left
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

                    let (view_params, needs_resize) = {
                        let mut st = st_rc.borrow_mut();
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

                        // Update Viewport size & Auto-Y
                        viewport.set_size(phys_w as f64, phys_h as f64, dpr as f64);
                        let (min_p, max_p) = store.get_visible_min_max(viewport.from_index, viewport.from_index + viewport.bars_visible);
                        viewport.update_auto_y(min_p, max_p, true);

                        let y_min = viewport.y_min;
                        let y_max = viewport.y_max;
                        let y_span = y_max - y_min;
                        let y_scale = if y_span > 0.0 { (phys_h as f64) / y_span } else { 1.0 };
                        let px_per_bar = viewport.px_per_bar();
                        let from_index = viewport.from_index;
                        let bars_visible = viewport.bars_visible;

                        // 1 & 2. Generate Ticks and Grid (if show_axes)
                        let mut ui_price_ticks = Vec::new();
                        let mut grid_y = Vec::new();
                        let mut ui_time_ticks = Vec::new();
                        let mut grid_x = Vec::new();

                        if show_axes {
                            let price_ticks = generate_price_ticks(y_min, y_max, phys_h as f64, &PriceTickOptions::default());
                            ui_price_ticks.reserve(price_ticks.len());
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
                            ui_time_ticks.reserve(time_ticks.len());
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
                        }

                        ui.set_price_ticks(ModelRc::new(VecModel::from(ui_price_ticks)));
                        ui.set_time_ticks(ModelRc::new(VecModel::from(ui_time_ticks)));

                        // 3. Last Price line and badge
                        if show_axes && store.count() > 0 {
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
                                slint::Color::from_argb_u8(255, 8, 153, 129) // #089981
                            } else {
                                slint::Color::from_argb_u8(255, 242, 54, 69) // #f23645
                            });
                        } else {
                            ui.set_last_price_visible(false);
                        }

                        // 4. Crosshair and HUD
                        if crosshair_active && logical_w > 0.0 && logical_h > 0.0 {
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

                                // HUD Values
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
                            }
                        }

                        let max_vol = store.get_visible_max_volume(from_index, from_index + bars_visible) as f32;

                        let view_params = ViewParams {
                            from_index: from_index as f32,
                            px_per_bar: px_per_bar as f32,
                            y_min: y_min as f32,
                            y_max: y_max as f32,
                            base_price: store.base_price() as f32,
                            count: store.count(),
                            dpr: dpr as f32,
                            up_color: [0.03, 0.65, 0.55, 1.0],     // #089981
                            down_color: [0.95, 0.25, 0.33, 1.0],   // #f23645
                            clear_color: Some(wgpu::Color {
                                r: 0.0745,
                                g: 0.0902,
                                b: 0.1333,
                                a: 1.0,
                            }), // #131722
                            grid_color: Some([0.1412, 0.1529, 0.1961, 1.0]), // #242732
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

                        (view_params, (phys_w, phys_h))
                    };

                    let mut r_slot = r_rc.borrow_mut();
                    let Some(ref mut renderer) = *r_slot else { return };

                    let (phys_w, phys_h) = needs_resize;
                    let needs_rebind = renderer.plot_width() != phys_w || renderer.plot_height() != phys_h;
                    if needs_rebind {
                        renderer.resize(phys_w, phys_h);
                        match slint::Image::try_from(renderer.texture().clone()) {
                            Ok(img) => ui.set_chart_texture(img),
                            Err(e) => eprintln!("Error creating Slint Image from wgpu::Texture: {:?}", e),
                        }
                    }

                    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("Frame Command Encoder"),
                    });
                    renderer.render(&mut encoder, &view_params);
                    queue.submit(Some(encoder.finish()));
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

    ui.run()
}
