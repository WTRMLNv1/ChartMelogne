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
| **`background_color`** | `color` | `#131722` | **Customizable Theme.** Chart background color (tints viewport and axes gutters). |
| **`up_color`** | `color` | `#089981` | **Customizable Theme.** Bullish (up) candle color, last price badge, and HUD badge. |
| **`down_color`** | `color` | `#ef5350` | **Customizable Theme.** Bearish (down) candle color, last price badge, and HUD badge. |
| **`show_axes`** | `bool` | `true` | **Optional.** If `false`, right price axis and bottom time axis collapse to `0px` and the plot occupies 100% of space. |
| **`chart_type`** | `string` | `"candlestick"` | **Optional / Configurable.** `"candlestick"`, `"line"`, or `"area"`. |
| **`show_volume`** | `bool` | `true` | **Optional.** Toggles volume histogram and HUD volume display. |
| **`show_indicators`**| `bool`| `false` | **Optional.** Toggles indicator overlays. |
| `indicators` | `[IndicatorUi]` | `[]` | Dynamic indicator models (`name`, `value`, `color`) displayed in TradingView style HUD. |
| **`show_markers`** | `bool` | `true` | **Optional.** Toggles visual trade / signal marker triangles. |
| **`allow_zoom`** | `bool` | `true` | **Optional.** Locks or unlocks mouse wheel and pinch-to-zoom. |
| **`allow_pan`** | `bool` | `true` | **Optional.** Locks or unlocks click-and-drag panning. |
| **`show_crosshair`** | `bool` | `true` | **Optional.** Toggles cursor crosshair lines and axis badges when hovering on chart. |
| **`show_menu_button`**| `bool`| `true` | **Optional.** Toggles the built-in 3-dot options menu (turn off if your host window has its own toolbar). |
| **`show_navigation_controls`**| `bool`| `true` | **Optional.** Toggles the bottom-left floating quick-navigation toolbar (zoom +/-, reset, latest, visible bars badge). |
| **`show_hud`** | `bool` | `true` | **Optional.** Toggles top-left OHLCV badges on cursor hover. |
| **`visible_bars`** | `float` | `300.0` | **Range / Zoom.** Current number of visible candlestick / bar intervals. Read/write from Slint or Rust. |
| **`visible_from`** | `float` | `0.0` | **Range / Navigation.** Leftmost visible bar index (float to support sub-bar smooth panning). |
| **`visible_to`** | `float` | `300.0` | **Range / Navigation.** Rightmost visible bar index (`visible_from + visible_bars`). |
| **`total_bars`** | `float` | `0.0` | Total number of loaded candles in the underlying data store. |
| **`visible_range_str`**| `string`| `"300 bars"` | Human-readable string badge of the current visible range. |
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
            
            // Custom Theme Colors (Optional)
            background_color: #0b0e14;      // e.g. custom dark/light background
            up_color: #00e676;              // custom vibrant green
            down_color: #ff1744;            // custom vibrant red

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
- **Interactive Range & Zoom/Pan Bar**:
  - **Presets**: `100b`, `250b`, `500b`, and `All` (zooms and frames the requested number of bars instantly).
  - **Zoom**: `+` (Zoom In 20%), `-` (Zoom Out 25%).
  - **Pan**: `◀` (Pan Left 25 bars), `▶` (Pan Right 25 bars).
  - **Quick Nav**: `Reset` (Fit to default 300 bars with Auto-Y), `Latest` (Scroll directly to most recent candle).
  - **Badge**: Dynamic pill showing current visible count (e.g. `250 bars`).
- **Floating Quick Navigation Bar**: An on-chart floating pill bar in the bottom-left with quick `+`, `-`, `◀`, `▶`, `Reset`, and `Latest` actions.
- **Left Panel (200px)**: Market watchlist with real-time selection highlighting.
- **Center Panel**: The embedded `ChartWidget` stretching dynamically with window resizing.
- **Right Panel (220px)**: Live order book ladder with Ask/Bid depth and quick Buy/Sell actions.

### Viewport & Navigation Callbacks

The `ChartWidget` exposes declarative callbacks that your host application or custom UI toolbar can invoke directly:

| Callback | Signature | Description |
|---|---|---|
| `zoom_in()` | `callback()` | Zooms in by 20% centered around the viewport midpoint. |
| `zoom_out()` | `callback()` | Zooms out by 25% centered around the viewport midpoint. |
| `reset_zoom()` | `callback()` | Resets zoom to default 300 bars and re-enables Auto-Y. |
| `scroll_to_end()` | `callback()` | Scrolls visible window to the newest bar. |
| `scroll_to_start()` | `callback()` | Scrolls visible window to the earliest bar (`from = 0.0`). |
| `pan_bars(delta)` | `callback(float)` | Pans the viewport horizontally by `delta` bars (+ moves right, - moves left). |
| `set_visible_bars(bars)`| `callback(float)` | Sets exact number of visible bars, keeping right edge anchored. |
| `set_visible_range(from, to)` | `callback(float, float)` | Sets explicit visible range `[from, to]`. |
| `set_from_index(from)` | `callback(float)` | Pans viewport so that leftmost visible bar is at `from`. |

All callbacks automatically clamp within valid ranges, update Auto-Y scaling, sync `visible_bars`, `visible_from`, `visible_to`, and `visible_range_str`, and trigger a redraw.

---

## Host-Provided Indicators & TradingView HUD Overlay

`ChartMelogne` does not compute indicators internally; technical analysis calculations are cleanly owned by your host application. The host simply passes pre-computed line values with custom RGBA line colors, and `ChartMelogne` handles GPU instanced polyline rendering and the live TradingView-style HUD overlay.

### 1. Defining Indicators in `ChartAppConfig`

```rust
use chartmelogne_core::types::HostIndicator;
use chartmelogne_core::synthetic::{compute_ema, compute_sma};
use chartmelogne_slint::{attach_chart_controller, ChartAppConfig};

fn main() -> Result<(), slint::PlatformError> {
    let app = MyTradingApp::new()?;

    // Compute indicators in your host app or external TA library
    let closes: Vec<f64> = /* historical close prices */;
    let ema_values = compute_ema(&closes, 20);
    let sma_values = compute_sma(&closes, 50);

    // Create HostIndicator instances with custom RGBA colors
    let ema20 = HostIndicator::new("EMA 20", [0.941, 0.725, 0.043, 1.0], ema_values); // Gold
    let sma50 = HostIndicator::new("SMA 50", [0.612, 0.153, 0.690, 1.0], sma_values); // Purple

    let mut config = ChartAppConfig::default();
    config.indicators = vec![ema20, sma50];

    // Connect controller to the Slint app
    attach_chart_controller!(&app, &config);

    app.run()
}
```

### 2. TradingView-Style Indicator HUD

When `show_hud` and `show_indicators` are enabled (or indicators are present):
- The top-left HUD stacks indicators beneath the OHLCV status line.
- Each indicator row displays:
  1. A colored bullet dot matching the indicator line's exact RGBA color.
  2. The indicator's display name (e.g. `EMA 20`).
  3. The indicator's value at the hovered cursor bar (or the most recent bar when idle), highlighted in the indicator's color.
  4. Automatic `n/a` display during warmup periods where the host indicator value is `None` or `NaN`.

### 3. Warmup & Gap Handling
Indicators often have warmup periods (e.g., the first 20 bars of an EMA 20). If `None` or `f64::NAN` is supplied for any candle index, the GPU shader (`indicator.wgsl`) automatically clips the corresponding line segment outside the viewport with zero overhead, cleanly resuming rendering as soon as valid numerical values begin.

---

## Signal & Trade Markers (Up & Down Triangles)

`ChartMelogne` supports drawing visual markers—green upward-pointing triangles for buys/signals, and red downward-pointing triangles for sells.

- **Placement**: Upwards triangles are placed a little bit below the **LOW** of the candle (pointing up toward the low wick). Downwards triangles are placed a little bit above the **HIGH** of the candle (pointing down toward the high wick).
- **Stacking**: If multiple triangles are added on the same candle, they automatically stack vertically with neat spacing between them.

### Host API: Pre-Mapped Index or Auto-Mapped Timestamp

```rust
use chartmelogne_core::types::ChartMarker;

// 1. Placed below candle LOW / above candle HIGH:
let buy = ChartMarker::up(50).with_text("BUY");
let sell = ChartMarker::down(120).with_text("SELL");

// 2. Multiple markers on the same candle stack automatically:
let second_buy = ChartMarker::up(50).with_text("BUY 2"); // Stacks below the first buy!

// 3. Auto-mapped by timestamp (binary-searched to nearest candle):
let ts_marker = ChartMarker::up_at_time(1700000000.0);

// 4. Pass markers into ChartAppConfig:
let mut config = ChartAppConfig::default();
config.markers = vec![buy, second_buy, sell, ts_marker];
```

---

## Theme & Color Customization

`ChartMelogne` lets host applications customize the background color, bullish (green) candle color, and bearish (red) candle color to seamlessly match any dark or light application theme.

### Configuring in Rust (`ChartAppConfig`)

```rust
let mut config = ChartAppConfig::default();

// RGBA float arrays [R, G, B, A] normalized in 0.0..1.0
config.background_color = [0.05, 0.05, 0.05, 1.0]; // Pitch dark background
config.up_color = [0.0, 0.9, 0.4, 1.0];             // Neon green up candles
config.down_color = [1.0, 0.2, 0.3, 1.0];           // Neon red down candles

attach_chart_controller!(&app, &config);
```

### CLI Arguments

When running the standalone chart or dashboard, pass custom hex colors via command line flags:
```bash
cargo run --bin chartmelogne -- --bg "#000000" --up-color "#00ff00" --down-color "#ff0000"
```

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
