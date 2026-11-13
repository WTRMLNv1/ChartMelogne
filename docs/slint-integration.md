# Slint GUI Integration Guide (`chartmelogne-slint`)

This guide explains how to use `chartmelogne-slint` to embed an interactive, GPU-accelerated financial chart directly into any native desktop application using [Slint](https://slint.dev/) (1.18+) and `wgpu 30`.

`chartmelogne-slint` provides both:
1. **An Embeddable `ChartWidget`**: A reusable Slint `Rectangle` component that can be placed inside *any* Slint layout (dashboards, tabbed views, trading workstations) with optional axes, configurable chart types, optional volume histograms, and optional indicators.
2. **Pre-built Windows**:
   - `AppWindow`: Standalone full-window chart with a 3-dot dropdown menu.
   - `DashboardWindow`: Multi-panel trading workstation layout demonstrating `ChartWidget` embedded alongside asset watchlists, top market status headers, and a live order book panel.
3. **The `attach_chart_controller!` Macro**: A 1-line Rust macro that hooks up all panning, zooming, axis tick generation, crosshair snapping, and wgpu render passes to any window hosting `ChartWidget`.

---

## Architectural Overview

Slint handles the windowing, event routing, and vector overlays (crosshairs, price badges, toolbars, HUD text). The financial chart itself (candles, volume, grid, and area fill) is rendered by `ChartRenderer` into an offscreen `wgpu::Texture`, which Slint consumes directly with zero CPU copy overhead.

```
┌─────────────────────────────────────────────────────────────────┐
│ Custom Slint Window (e.g., DashboardWindow)                     │
│                                                                 │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │ Top Navigation / Market Bar                               │  │
│  └───────────────────────────────────────────────────────────┘  │
│  ┌───────────────┬─────────────────────────────┬─────────────┐  │
│  │ Watchlist     │ Embedded ChartWidget        │ Order Book  │  │
│  │ Panel         │                             │ Panel       │  │
│  │               │ 1. wgpu Candle/Grid Texture │             │  │
│  │ - BTC/USDT    │ 2. Top-Left HUD (OHLCV)     │ - Asks      │  │
│  │ - ETH/USDT    │ 3. Vector Crosshair Lines   │ - Spread    │  │
│  │ - SOL/USDT    │ 4. Optional Price/Time Axes │ - Bids      │  │
│  │               │                             │             │  │
│  └───────────────┴─────────────────────────────┴─────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Embeddable Component: `ChartWidget`

`ChartWidget` is defined in `crates/slint/ui/chart_widget.slint` and inherits `Rectangle`. It can be imported into any Slint file.

### Property Reference

| Property | Type | Default | Description |
|---|---|---|---|
| `chart_texture` | `image` | `null` | Target texture produced by `ChartRenderer` via `wgpu 30`. |
| `plot_width` | `length` | `auto` | Read-only alias to plot area width in logical pixels. |
| `plot_height` | `length` | `auto` | Read-only alias to plot area height in logical pixels. |
| **`show_axes`** | `bool` | `true` | **Optional.** If `false`, right price axis and bottom time axis collapse to `0px` and the plot occupies 100% of space. |
| **`chart_type`** | `string` | `"candlestick"` | **Optional / Configurable.** `"candlestick"`, `"line"`, or `"area"`. |
| **`show_volume`** | `bool` | `true` | **Optional.** Toggles volume histogram and HUD volume display. |
| **`show_indicators`**| `bool`| `false` | **Optional.** Toggles indicator overlays. |
| **`allow_zoom`** | `bool` | `true` | **Optional.** Locks or unlocks mouse wheel and pinch-to-zoom. |
| **`show_menu_button`**| `bool`| `true` | **Optional.** Toggles the built-in 3-dot options menu (turn off if your host window has its own toolbar). |
| **`show_hud`** | `bool` | `true` | **Optional.** Toggles top-left OHLCV badges on cursor hover. |
| `price_ticks` | `[PriceTickUi]` | `[]` | Dynamic price tick models (`y_px`, `label`). |
| `time_ticks` | `[TimeTickUi]` | `[]` | Dynamic time tick models (`x_px`, `label`). |
| `crosshair_active` | `bool` | `false` | True when cursor hovers over the plot. |
| `last_price_visible`| `bool` | `true` | Shows horizontal dashed line and badge for current candle. |

---

## How to Embed `ChartWidget` in Your Project

### 1. In Your `.slint` File

Import `ChartWidget` and drop it anywhere in your window layout:

```slint
import { ChartWidget, PriceTickUi, TimeTickUi } from "chartmelogne_slint/ui/chart_widget.slint";

export component MyTradingApp inherits Window {
    title: "My Custom App";
    min-width: 1024px;
    min-height: 600px;

    HorizontalLayout {
        // Left custom sidebar
        Rectangle {
            width: 200px;
            background: #161b22;
            Text { text: "Assets & Watchlist"; color: #ffffff; }
        }

        // Center: Embedded ChartMelogne Widget!
        chart := ChartWidget {
            horizontal-stretch: 1;
            vertical-stretch: 1;
            
            // Customize feature flags
            show_axes: true;                // or false for a clean sparkline
            chart_type: "candlestick";      // "candlestick", "line", or "area"
            show_volume: true;              // volume histogram
            show_menu_button: false;        // hide built-in menu, use custom buttons
        }
    }
}
```

### 2. In Your `main.rs` (Connecting the Controller in 1 Line)

Use the `attach_chart_controller!` macro to automatically attach all event handlers, auto-Y scaling, tick computations, and the wgpu 30 rendering loop:

```rust
use chartmelogne_slint::{attach_chart_controller, ChartAppConfig};

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let app = MyTradingApp::new()?;
    let config = ChartAppConfig::default();

    // 1-Line Controller Attachment:
    attach_chart_controller!(&app, &config);

    app.run()
}
```

---

## Multi-Panel Trading Dashboard (`DashboardWindow`)

`chartmelogne-slint` includes a complete multi-panel workstation layout in `DashboardWindow`.

### Running the Dashboard
You can run it directly using the CLI:
```bash
cargo run --bin chartmelogne -- --dashboard
```
Or in Rust:
```rust
use chartmelogne_slint::{run_dashboard_app, ChartAppConfig};

fn main() -> Result<(), slint::PlatformError> {
    run_dashboard_app(ChartAppConfig::default())
}
```

### Layout Breakdown
- **Top Navigation Bar**: Live instrument ticker (`BTC/USDT`), percentage change badge, pill buttons to toggle `Axes ON/OFF`, `Vol ON/OFF`, `Zoom Lock`, and chart type switcher (`Candles` / `Line` / `Area`).
- **Left Panel (200px)**: Market watchlist with real-time selection highlighting.
- **Center Panel**: The embedded `ChartWidget` stretching dynamically with window resizing.
- **Right Panel (220px)**: Live order book ladder with Ask/Bid depth and quick Buy/Sell actions.

---

## Live High-Frequency Data Streaming

To stream real-time price updates into an embedded `ChartWidget`:

```rust
// Inside your WebSocket / market data stream handler:
slint::invoke_from_event_loop(move || {
    // 1. Update CPU store
    let last_idx = store.count() - 1;
    store.update_last(new_high, new_low, new_close, new_volume);

    // 2. Direct GPU buffer update (16 bytes write, no historical re-upload)
    let base = store.base_price();
    let packed = [
        (store.get_open(last_idx) - base) as f32,
        (new_high - base) as f32,
        (new_low - base) as f32,
        (new_close - base) as f32,
    ];
    renderer.update_last(&packed, last_idx);

    let vol_packed = [new_volume as f32, if new_close >= store.get_open(last_idx) { 1.0 } else { 0.0 }];
    renderer.update_last_volume(&vol_packed, last_idx);

    // 3. Mark dirty and request redraw
    app.window().request_redraw();
});
```
