# Getting Started with ChartMelogne Rust

This guide explains how to add ChartMelogne to an existing Rust project, select the appropriate crate for your use case, and implement working charting examples.

---

## 1. Adding Dependencies to `Cargo.toml`

Depending on your requirements, add the crates you need:

### Option A: Pure Math, Store & Viewport (No GPU / No UI)
For backtesting engines, data analysis, or custom renderers:

```toml
[dependencies]
chartmelogne-core = { path = "path/to/chartmelogne-rs/crates/core" }
```

### Option B: GPU Rendering (`wgpu 30`)
For native games, custom windowing frameworks (e.g. Winit, Egui, Iced), or offscreen image export:

```toml
[dependencies]
chartmelogne-core = { path = "path/to/chartmelogne-rs/crates/core" }
chartmelogne-render = { path = "path/to/chartmelogne-rs/crates/render" }
wgpu = "30"
pollster = "0.4"
```

### Option C: Complete Desktop GUI with Slint
For a turnkey, interactive desktop charting application:

```toml
[dependencies]
chartmelogne-core = { path = "path/to/chartmelogne-rs/crates/core" }
chartmelogne-render = { path = "path/to/chartmelogne-rs/crates/render" }
chartmelogne-slint = { path = "path/to/chartmelogne-rs/crates/slint" }
slint = { version = "~1.18", features = ["backend-winit", "renderer-femtovg-wgpu", "unstable-wgpu-30"] }
```

### Recommended Compiler Optimization Profile
In your workspace root `Cargo.toml`, enable Link-Time Optimization (LTO) to allow aggressive cross-crate inlining between data storage and shader preparation:

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
```

For maximum performance on modern processors, build with native CPU flags (enables AVX2/FMA for fast float calculations over 450k bars):
```bash
# On Linux / macOS
RUSTFLAGS="-C target-cpu=native" cargo build --release

# On Windows PowerShell
$env:RUSTFLAGS="-C target-cpu=native"; cargo build --release
```

---

## 2. Quickstart Examples

### Example 1: Standalone Data Store & Coordinate Math (`chartmelogne-core`)

Use `CandleStore` and `Viewport` without any GPU or windowing dependencies:

```rust
use chartmelogne_core::{
    candle_store::CandleStore,
    synthetic::{generate_synthetic_candles, SyntheticDataOptions},
    types::Bar,
    viewport::{Viewport, ViewportOptions},
};

fn main() {
    // 1. Generate or load market data (450,000 bars)
    let candles = generate_synthetic_candles(SyntheticDataOptions {
        count: 450_000,
        start_price: 100.0,
        volatility: 0.002,
        seed: 1337,
        ..Default::default()
    });

    // 2. Initialize high-performance Columnar Store
    // 330 = IST timezone (+05:30) for civil market day boundaries
    let mut store = CandleStore::new(candles.time.len(), 330);
    store.set_data(&candles, true).expect("Valid candle data");

    println!("Loaded {} candles. Base price: {}", store.count(), store.base_price());

    // 3. Initialize Viewport projection
    let mut viewport = Viewport::new(ViewportOptions::default());
    viewport.set_size(1920.0, 1080.0, 1.0); // 1920x1080 screen pixels
    viewport.set_total_bars(store.count());
    viewport.set_visible_range(449_700.0, 450_000.0); // View last 300 bars

    // 4. O(1) Block-Accelerated Min/Max queries
    let (y_min, y_max) = store.get_visible_min_max(
        viewport.from_index,
        viewport.from_index + viewport.bars_visible,
    );
    viewport.update_auto_y(y_min, y_max, true);

    // 5. Screen <-> World coordinate transforms
    let screen_x = viewport.bar_index_to_screen_px(449_950.0);
    let screen_y = viewport.price_to_screen_px(y_min + (y_max - y_min) * 0.5);
    println!("Bar 449,950 screen location: ({:.1}px, {:.1}px)", screen_x, screen_y);

    // 6. Simulate real-time streaming market ticks (sub-millisecond update)
    let last_idx = store.count() - 1;
    let updated_bar = Bar {
        time: store.time[last_idx],
        open: store.get_open(last_idx),
        high: store.get_high(last_idx).max(105.5),
        low: store.get_low(last_idx).min(103.2),
        close: 105.2,
        volume: store.volume[last_idx] + 15,
        flags: None,
    };
    store.update_last(updated_bar);
}
```

---

### Example 2: Offscreen Headless GPU Rendering (`chartmelogne-render`)

Render an image directly to a GPU texture or capture a screenshot:

```rust
use chartmelogne_core::{
    candle_store::CandleStore,
    synthetic::{generate_synthetic_candles, SyntheticDataOptions},
};
use chartmelogne_render::{ChartRenderer, ViewParams};
use wgpu::util::DeviceExt;

fn main() {
    // 1. Setup wgpu device and queue
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    })).expect("GPU Adapter");

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None))
        .expect("Device");

    // 2. Load dataset
    let data = generate_synthetic_candles(SyntheticDataOptions {
        count: 10_000,
        ..Default::default()
    });
    let mut store = CandleStore::new(data.time.len(), 330);
    store.set_data(&data, true).unwrap();

    // 3. Initialize renderer with RGBA8Unorm target format
    let mut renderer = ChartRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.resize(1000, 600);
    renderer.upload_all(&store.ohlc, store.count());
    renderer.upload_volume(&store.volume_instances, store.count());

    // 4. Configure frame parameters
    let view = ViewParams {
        from_index: 9_700.0,
        px_per_bar: 1000.0 / 300.0,
        y_min: 95.0,
        y_max: 110.0,
        base_price: store.base_price() as f32,
        count: store.count(),
        dpr: 1.0,
        clear_color: Some(wgpu::Color { r: 0.074, g: 0.09, b: 0.133, a: 1.0 }), // Dark background
        grid_color: Some([0.14, 0.15, 0.19, 1.0]),
        ..Default::default()
    };

    // 5. Encode and submit render pass
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Export Encoder"),
    });
    renderer.render(&mut encoder, &view);
    queue.submit(Some(encoder.finish()));

    // The output is now in renderer.texture() and renderer.texture_view()
    println!("Frame rendered successfully to wgpu::Texture!");
}
```

---

### Example 3: Running the Full Slint Interactive Application (`chartmelogne-slint`)

Launch the prebuilt interactive desktop chart application with mouse panning, wheel zooming, dynamic Y-axis autoscaling, crosshair hovering, HUD metrics, and menu controls:

```rust
use chartmelogne_core::{
    synthetic::{generate_synthetic_candles, SyntheticDataOptions},
    types::ChartType,
};
use chartmelogne_slint::{run_chart_app, ChartAppConfig};

fn main() -> Result<(), slint::PlatformError> {
    // Generate or fetch your dataset
    let data = generate_synthetic_candles(SyntheticDataOptions {
        count: 450_000,
        start_price: 1000.0,
        seed: 1337,
        ..Default::default()
    });

    let config = ChartAppConfig {
        candle_data: data,
        utc_offset_minutes: 330, // UTC+05:30 (IST)
        allow_zoom: true,
        show_axes: true,
        chart_type: ChartType::Candlestick,
        show_volume: true,
        show_indicators: false,
    };

    run_chart_app(config)
}
```

---

### Example 4: Embedding `ChartWidget` in Your Custom Slint Window

You can embed the `ChartWidget` component into any `.slint` file and connect the entire interactive controller with the `attach_chart_controller!` macro:

```slint
// In your custom Slint file (e.g. main_window.slint):
import { ChartWidget } from "chartmelogne_slint/ui/chart_widget.slint";

export component MainWindow inherits Window {
    HorizontalLayout {
        // Your custom sidebar
        Rectangle { width: 200px; background: #161b22; }

        // Embedded chart widget
        chart := ChartWidget {
            show_axes: true;                // or false
            chart_type: "candlestick";      // "candlestick", "line", or "area"
            show_volume: true;              // optional volume histogram
            show_menu_button: false;        // hide built-in menu, use window buttons
        }
    }
}
```

```rust
// In your Rust main.rs:
use chartmelogne_slint::{attach_chart_controller, ChartAppConfig};

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let window = MainWindow::new()?;
    let config = ChartAppConfig::default();

    // 1-Line Controller Attachment:
    attach_chart_controller!(&window, &config);

    window.run()
}
```
