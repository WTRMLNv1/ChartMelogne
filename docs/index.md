# ChartMelogne Rust API Documentation

**ChartMelogne** is a high-performance, GPU-accelerated financial charting library written in Rust. It is engineered to visualize datasets with hundreds of thousands of candlestick bars (e.g., 450,000+ 1-minute bars representing years of trading data) with 60 FPS pan and zoom, sub-millisecond tick updates, and minimal memory usage (< 20 MB total footprint).

Originally designed as a native alternative to TradingView and WebGL charting solutions, ChartMelogne-rs cleanly separates data processing, GPU rendering via `wgpu 30`, and declarative UI overlays via `Slint`.

---

## Documentation Structure

| Guide | Description |
|---|---|
| [**Getting Started & Integration Guide**](getting-started.md) | How to add ChartMelogne to your Cargo project, step-by-step guides for standalone, custom wgpu, and Slint integrations. |
| [**Core API Reference (`chartmelogne-core`)**](core-api.md) | Columnar data store (`CandleStore`), 256-block min/max index, coordinate math (`Viewport`), price and time axes, synthetic data generation, and formatting. |
| [**Renderer API Reference (`chartmelogne-render`)**](render-api.md) | `wgpu 30` renderer, WGSL shaders, instanced candle drawing, volume histograms, area/line series, grid pass, and custom render loops. |
| [**Slint UI Integration Guide (`chartmelogne-slint`)**](slint-integration.md) | How to embed the chart in Slint desktop applications, zero-copy texture sharing, event routing, crosshair overlays, and HUD components. |

---

## Architectural Overview

The workspace is organized into modular crates with strict dependency boundaries:

```
                  ┌───────────────────────────────┐
                  │       chartmelogne-slint      │  <-- Slint GUI glue, crosshair,
                  │  (Slint 1.18 + wgpu texture)  │      HUD, axis badges, interaction
                  └───────────────┬───────────────┘
                                  │
                  ┌───────────────▼───────────────┐
                  │      chartmelogne-render      │  <-- wgpu 30 rendering pipelines,
                  │  (WGSL Shaders: candle, grid, │      instanced VBOs, target texture
                  │        volume, line)          │
                  └───────────────┬───────────────┘
                                  │
                  ┌───────────────▼───────────────┐
                  │       chartmelogne-core       │  <-- Pure Rust, zero GPU/UI types.
                  │  (CandleStore, Viewport,      │      SoA memory store, 256-block index,
                  │   PriceAxis, TimeAxis)        │      math & projection transforms
                  └───────────────────────────────┘
```

### Design Principles

1. **Separation of Concerns**: `chartmelogne-core` has zero GPU or UI dependencies. It can be used standalone in CLI tools, backtesting engines, and web services.
2. **Columnar SoA Storage**: Timestamps (`f64`), volumes (`u32`), and flags (`u8`) are stored in contiguous memory columns. Prices are stored in a packed interleaved `f32` buffer (`[open, high, low, close]` relative to `base_price`), serving as both the CPU store and the direct GPU vertex upload source.
3. **Zero Heap Allocation in Steady State**: Panning, zooming, and cursor hovering produce zero dynamic heap allocations. Precomputed string tables and reusable buffers ensure consistent 60 FPS frame times without garbage collection or memory allocator pauses.
4. **Hardware Acceleration**: Candle bodies and wicks are rendered in a single instanced draw call using custom WGSL shaders, skipping multi-pass mesh construction.
