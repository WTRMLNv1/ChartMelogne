# Slint GUI Integration Guide (`chartmelogne-slint`)

This guide explains how `chartmelogne-slint` integrates `chartmelogne-core` and `chartmelogne-render` into a native desktop application using [Slint](https://slint.dev/) (version 1.18+) and `wgpu 30`.

It also provides the exact patterns required to embed the GPU chart into your own custom Slint desktop application.

---

## Architectural Overview

Slint handles the windowing, event handling, input routing, and vector UI elements (crosshairs, price badges, toolbars, HUD overlays). The financial chart itself (candles, volume, grid, and area fill) is rendered by `ChartRenderer` into an offscreen `wgpu::Texture`, which Slint consumes directly without CPU memory copying.

```
┌─────────────────────────────────────────────────────────────────┐
│ Slint AppWindow                                                 │
│                                                                 │
│  ┌───────────────────────┐                                      │
│  │ Top Toolbar           │ (Timeframe buttons, ChartType, Menu) │
│  └───────────────────────┘                                      │
│  ┌──────────────────────────────────────────────┬─────────────┐ │
│  │ Plot Viewport (TouchArea)                    │ Price Axis  │ │
│  │                                              │             │ │
│  │  1. Slint Image (wgpu::Texture)              │ Price Ticks │ │
│  │  2. Top-Left HUD (OHLCV text badges)         │ Last Price  │ │
│  │  3. Vector Crosshair (Vertical/Horizontal)   │ Hover Badge │ │
│  │                                              │             │ │
│  ├──────────────────────────────────────────────┴─────────────┤ │
│  │ Time Axis (Time Ticks, Snapped Date Badge)                 │ │
│  └────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

---

## Zero-Copy Texture Sharing

Slint's `wgpu_30` backend feature exposes the underlying `wgpu::Device` and `wgpu::Queue` directly to user code via `set_rendering_notifier`.

When the offscreen `wgpu::Texture` is created or resized, it is converted into a `slint::Image` via:

```rust
let slint_image = slint::Image::try_from(renderer.texture().clone())?;
ui.set_chart_texture(slint_image);
```

Inside Slint's internal renderer, this texture is bound directly to Slint's composite pass. No pixel readbacks (`map_async` or `copy_texture_to_buffer`) occur.

---

## Slint Window Definition (`appwindow.slint`)

The declarative Slint hierarchy organizes the viewport and overlay layers:

```slint
export struct PriceTickUi {
    y_px: length,
    label: string,
}

export struct TimeTickUi {
    x_px: length,
    label: string,
}

export component AppWindow inherits Window {
    title: "ChartMelogne - Desktop Chart";
    min-width: 900px;
    min-height: 550px;
    background: #131722;

    // Viewport image & dimensions
    in-out property <image> chart_texture;
    in-out property <length> plot_width <=> plot_area.width;
    in-out property <length> plot_height <=> plot_area.height;

    // Ticks & Badges
    in-out property <[PriceTickUi]> price_ticks: [];
    in-out property <[TimeTickUi]> time_ticks: [];

    // Crosshair & HUD
    in-out property <bool> crosshair_active: false;
    in-out property <length> crosshair_x: 0px;
    in-out property <length> crosshair_y: 0px;
    in-out property <length> crosshair_snapped_x: 0px;
    in-out property <string> crosshair_price: "";
    in-out property <string> crosshair_time: "";

    // Event callbacks exposed to Rust
    callback dragged(length, length);
    callback drag_started(length, length);
    callback drag_ended();
    callback scrolled(length, length, length);
    callback pointer_moved(length, length);
    callback pointer_left();

    // Layout
    HorizontalLayout {
        plot_area := Rectangle {
            clip: true;

            // 1. GPU Rendered Chart Texture
            Image {
                width: 100%;
                height: 100%;
                source: root.chart_texture;
            }

            // 2. Vector Crosshair Overlays
            if root.crosshair_active : Rectangle {
                // Horizontal line
                Rectangle {
                    y: root.crosshair_y;
                    width: 100%;
                    height: 1px;
                    background: #787b86;
                }
                // Vertical snapped line
                Rectangle {
                    x: root.crosshair_snapped_x;
                    height: 100%;
                    width: 1px;
                    background: #787b86;
                }
            }

            // 3. User Gesture Capture
            TouchArea {
                mouse-cursor: crosshair;
                moved => {
                    root.pointer_moved(self.mouse-x, self.mouse-y);
                    if (self.pressed) {
                        root.dragged(self.mouse-x, self.mouse-y);
                    }
                }
                pointer-event(event) => {
                    if (event.button == PointerEventButton.left) {
                        if (event.kind == PointerEventKind.down) {
                            root.drag_started(self.mouse-x, self.mouse-y);
                        } else if (event.kind == PointerEventKind.up || event.kind == PointerEventKind.cancel) {
                            root.drag_ended();
                        }
                    }
                }
                scroll-event(event) => {
                    root.scrolled(self.mouse-x, self.mouse-y, event.delta-y);
                    accept
                }
            }
        }

        // 4. Right Price Axis Column
        Rectangle {
            width: 65px;
            background: #131722;
            // Iterate over root.price_ticks and render labels
        }
    }
}
```

---

## Slint Rendering Lifecycle (`set_rendering_notifier`)

The synchronization between Slint's draw loop and `wgpu` occurs inside `ui.window().set_rendering_notifier(...)`.

It manages 3 core lifecycle events:

### 1. `slint::RenderingState::RenderingSetup`
Called once when the window's GPU surface is created or recreated.
- Obtains the shared `wgpu::Device` and `wgpu::Queue`.
- Instantiates `ChartRenderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm)`.
- Performs the initial `upload_all` for OHLC and volume instances.

```rust
slint::RenderingState::RenderingSetup => {
    let mut renderer = ChartRenderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
    let st = state.borrow();
    renderer.upload_all(&st.store.ohlc, st.store.count());
    renderer.upload_volume(&st.store.volume_instances, st.store.count());
    *renderer_slot.borrow_mut() = Some(renderer);
}
```

### 2. `slint::RenderingState::BeforeRendering`
Executed immediately before Slint renders its vector elements.
- **Physical Pixel Sizing**: Queries logical widget dimensions and DPR:
  ```rust
  let dpr = ui.window().scale_factor();
  let phys_w = ((logical_w as f64) * (dpr as f64)).round() as u32;
  let phys_h = ((logical_h as f64) * (dpr as f64)).round() as u32;
  ```
- **Texture Resize Handling**: If physical dimensions change, `renderer.resize(phys_w, phys_h)` is called and the updated `slint::Image` is assigned to `ui.set_chart_texture`.
- **Decoupled Dirty Flag Execution**:
  - Hovering the mouse does **not** re-render the GPU chart texture.
  - Only panning, zooming, resizing, or data updates set `chart_dirty = true`.
  - When `chart_dirty` is true, auto-Y bounds, price ticks, and time ticks are recomputed, and a GPU command encoder executes `renderer.render(&mut encoder, &view_params)`.
  - Mouse hover simply repositions the lightweight vector crosshair and HUD text properties.

```rust
slint::RenderingState::BeforeRendering => {
    let Some(ui) = ui_handle.upgrade() else { return };
    let mut r_slot = renderer_slot.borrow_mut();
    let Some(ref mut renderer) = *r_slot else { return };

    if needs_resize {
        renderer.resize(phys_w, phys_h);
        if let Ok(img) = slint::Image::try_from(renderer.texture().clone()) {
            ui.set_chart_texture(img);
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
```

### 3. `slint::RenderingState::RenderingTeardown`
Called when the Slint window is closing or the GPU context is invalidated. Drops the renderer to release GPU buffers.

```rust
slint::RenderingState::RenderingTeardown => {
    *renderer_slot.borrow_mut() = None;
}
```

---

## Event Routing & Gesture Handling

### 1. Panning (Dragging)
Dragging horizontally translates the visible bar window. Dragging vertically (in manual Y mode) pans the price scale:

```rust
ui.on_dragged({
    let state = Rc::clone(&state);
    let ui_handle = ui.as_weak();
    move |x, y| {
        let mut st = state.borrow_mut();
        if st.is_dragging {
            let dx = x - st.last_drag_x;
            st.last_drag_x = x;
            st.last_drag_y = y;

            if let Some(ui) = ui_handle.upgrade() {
                let dpr = ui.window().scale_factor();
                st.viewport.pan_by_pixels((dx as f64) * (dpr as f64));
                st.chart_dirty = true;
                ui.window().request_redraw();
            }
        }
    }
});
```

### 2. Zooming (Mouse Wheel)
The mouse wheel zooms around the cursor's bar index:

```rust
ui.on_scrolled({
    let state = Rc::clone(&state);
    let ui_handle = ui.as_weak();
    move |x, _y, delta_y| {
        let mut st = state.borrow_mut();
        if !st.allow_zoom { return; }

        let factor = if delta_y > 0.0 { 0.85 } else { 1.15 };
        if let Some(ui) = ui_handle.upgrade() {
            let dpr = ui.window().scale_factor();
            st.viewport.zoom_at_pixel((x as f64) * (dpr as f64), factor);
            st.chart_dirty = true;
            ui.window().request_redraw();
        }
    }
});
```

### 3. High-Performance Hover & Crosshair Snapping
Hover events calculate the nearest bar index and price without triggering a GPU re-render:

```rust
ui.on_pointer_moved({
    let state = Rc::clone(&state);
    let ui_handle = ui.as_weak();
    move |x, y| {
        let mut st = state.borrow_mut();
        st.crosshair_active = true;
        st.crosshair_screen_x = x;
        st.crosshair_screen_y = y;

        // Request Slint redraw ONLY for the UI overlay layer
        if let Some(ui) = ui_handle.upgrade() {
            ui.window().request_redraw();
        }
    }
});
```

---

## Live Streaming Updates Pattern

To stream real-time price updates into a running Slint chart:

```rust
// In your WebSocket or async feed handler:
slint::invoke_from_event_loop(move || {
    let mut st = state.borrow_mut();
    
    // 1. Update CPU store
    let last_idx = st.store.count() - 1;
    st.store.update_last(new_high, new_low, new_close, new_volume);

    // 2. Direct GPU update (16-byte write)
    if let Some(ref mut renderer) = *renderer_slot.borrow_mut() {
        let base = st.store.base_price();
        let packed = [
            (st.store.get_open(last_idx) - base) as f32,
            (new_high - base) as f32,
            (new_low - base) as f32,
            (new_close - base) as f32,
        ];
        renderer.update_last(&packed, last_idx);

        let vol_packed = [new_volume as f32, if new_close >= st.store.get_open(last_idx) { 1.0 } else { 0.0 }];
        renderer.update_last_volume(&vol_packed, last_idx);
    }

    // 3. Mark dirty and trigger frame
    st.chart_dirty = true;
    ui.window().request_redraw();
});
```
