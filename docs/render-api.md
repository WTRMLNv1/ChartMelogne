# chartmelogne-render API Reference

`chartmelogne-render` is the GPU rendering engine of ChartMelogne. It compiles custom WGSL shaders and executes instanced draw calls on `wgpu 30` to render hundreds of thousands of candlesticks, volume bars, area/line series, and grid lines directly into a GPU texture in sub-millisecond frame times.

---

## Architecture & Rendering Pipeline

`ChartRenderer` operates offscreen: it renders into a dedicated `wgpu::Texture` (`Rgba8Unorm` or surface swapchain format). The resulting texture can either be sampled as a Slint UI image, integrated into a custom GUI, or read back to host memory for image export.

```
       Host Data (SoA / Packed OHLC)
                   │
                   ▼
       ChartRenderer::upload_all()
                   │
      ┌────────────┴─────────────┐
      ▼                          ▼
Instance Buffer (OHLC)    Volume Buffer (Vol, Up)
  [f32x4 per candle]        [f32x2 per bar]
      │                          │
      └────────────┬─────────────┘
                   │
                   ▼
     Combined Render Pass (render())
                   │
    ┌──────────────┼──────────────┬──────────────┐
    ▼              ▼              ▼              ▼
1. Grid Pass  2. Volume Pass 3. Candle Pass 4. Line/Area Pass
 (grid.wgsl)   (volume.wgsl)  (candle.wgsl)   (line.wgsl)
    └──────────────┬──────────────┴──────────────┘
                   │
                   ▼
          Target wgpu::Texture
      (Sampled in Slint / Swapchain)
```

### Instanced Candle Architecture

Conventional charting engines generate separate vertex meshes for every candle wick and body quad. For 450,000 candles, that would require millions of dynamic vertices pushed to the GPU each frame.

`chartmelogne-render` solves this via **hardware instancing**:
- **Static Unit Vertex Buffer (`UNIT_VERTICES`)**: Uploaded once to GPU memory at startup. Contains 12 vertices (36 `f32`s):
  - 6 vertices (2 triangles) forming a unit quad for the **wick** (`part = 0.0`).
  - 6 vertices (2 triangles) forming a unit quad for the **body** (`part = 1.0`).
- **Dynamic Instance Buffer**: 16 bytes per candle storing `[open, high, low, close]` as `f32` offsets relative to `base_price`.
- **Vertex Shader (`candle.wgsl`)**: Evaluates candle coordinates, wick clamping, body width, and bullish/bearish color selection on the GPU for each instance in parallel.

---

## Public Types & Structs

### `ChartRenderer`

The main rendering controller holding pipeline state, uniform buffers, instance buffers, and the target render texture.

```rust
pub struct ChartRenderer {
    // Private fields: device, queue, pipelines, buffers, texture, etc.
}
```

#### Constructors & Lifecycle

##### `ChartRenderer::new`
```rust
pub fn new(
    device: &wgpu::Device, 
    queue: &wgpu::Queue, 
    format: wgpu::TextureFormat
) -> Self
```
Initializes all 4 WGSL shader pipelines (candle, grid, volume, line), uniform bind groups, static unit vertex buffers, and an initial 1x1 scratch texture.

- `device`: Borrowed `wgpu::Device`.
- `queue`: Borrowed `wgpu::Queue`.
- `format`: Texture format of the target render target (e.g., `wgpu::TextureFormat::Rgba8Unorm`).

##### `resize`
```rust
pub fn resize(&mut self, w_px: u32, h_px: u32)
```
Resizes the target rendering texture in physical pixels. Automatically clamps dimensions to at least `1x1` to prevent wgpu validation panics. Reallocates the internal `wgpu::Texture` and its `wgpu::TextureView` if dimensions changed.

#### Data Upload & Streaming Methods

##### `upload_all`
```rust
pub fn upload_all(&mut self, ohlc: &[f32], count: usize)
```
Uploads an entire packed OHLC slice to the GPU instance buffer.
- `ohlc`: Contiguous slice with 4 floats per bar: `[open, high, low, close]` relative to `base_price`.
- `count`: Number of candlestick bars.
- Automatically doubles instance buffer capacity if `count > instance_capacity`.

##### `append`
```rust
pub fn append(&mut self, bar: &[f32; 4], index: usize)
```
Appends a new bar at `index`.
- If `index >= instance_capacity`, doubles capacity and copies existing data on the GPU using `encoder.copy_buffer_to_buffer` before writing the new bar.
- Writes 16 bytes directly via `queue.write_buffer`.

##### `update_last`
```rust
pub fn update_last(&mut self, bar: &[f32; 4], index: usize)
```
Updates the candle at `index` in-place. Performs a single 16-byte upload to the GPU buffer at byte offset `index * 16`. Designed for high-frequency tick updates without re-uploading historical bars.

##### `upload_volume`
```rust
pub fn upload_volume(&mut self, volume_data: &[f32], count: usize)
```
Uploads volume histogram data.
- `volume_data`: Slice with 2 floats per bar: `[volume, is_up]` where `is_up` is `1.0` (green) or `0.0` (red).
- `count`: Total bar count.

##### `append_volume`
```rust
pub fn append_volume(&mut self, bar: &[f32; 2], index: usize)
```
Appends a single volume bar at `index`. Expands buffer with GPU-to-GPU copy if needed.

##### `update_last_volume`
```rust
pub fn update_last_volume(&mut self, bar: &[f32; 2], index: usize)
```
In-place 8-byte write for streaming the active bar's volume.

##### `upload_indicator`
```rust
pub fn upload_indicator(
    &mut self,
    id: &str,
    color: [f32; 4],
    line_width: f32,
    values_relative: &[f32],
    count: usize,
)
```
Uploads or updates an indicator series directly on the GPU.
- `values_relative`: `f32` offsets relative to `base_price` (use `f32::NAN` for missing/warmup bars).
- `indicator.wgsl` automatically detects `NaN` and clips degenerate segments without vertex artifacts.

##### `remove_indicator` & `clear_indicators`
```rust
pub fn remove_indicator(&mut self, id: &str) -> bool
pub fn clear_indicators(&mut self)
```
Removes a specific indicator or all indicators from GPU memory.

##### `upload_markers` & `clear_markers`
```rust
pub fn upload_markers(&mut self, markers: &[MarkerGpuInstance])
pub fn clear_markers(&mut self)
pub fn marker_count(&self) -> usize
```
Uploads marker instances (up and down triangles) directly to GPU instance buffer. Render pass 6 draws all markers in a single instanced triangle draw call (`draw(0..3, 0..marker_count)`).

#### Frame Execution

##### `render`
```rust
pub fn render(&mut self, encoder: &mut wgpu::CommandEncoder, view: &ViewParams)
```
Renders all enabled elements (grid, volume, candles, line/area) into the internal texture using a single command encoder and single render pass.

- Computes visible index range `[first, last]` from `view.from_index`, `view.px_per_bar`, and viewport width with a 2-bar padding margin.
- Off-screen bars are completely discarded by instancing range `first..(last + 1)`, requiring zero GPU vertex work for culled bars.
- Reuses persistent grid vertex buffer without allocations.

#### Texture & State Accessors

```rust
pub fn texture(&self) -> &wgpu::Texture
pub fn texture_view(&self) -> &wgpu::TextureView
pub fn device(&self) -> &wgpu::Device
pub fn queue(&self) -> &wgpu::Queue
pub fn instance_capacity(&self) -> usize
pub fn instance_count(&self) -> usize
pub fn instance_buffer(&self) -> &wgpu::Buffer
pub fn plot_width(&self) -> u32
pub fn plot_height(&self) -> u32
```

---

## Uniform Structures

All uniform structs use `#[repr(C)]` and implement `bytemuck::Pod` and `bytemuck::Zeroable` to match WGSL std140 layout requirements:

### `CandleUniforms` (80 Bytes)

```rust
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CandleUniforms {
    pub viewport_px: [f32; 2], // Plot width & height in physical pixels (offset 0)
    pub px_per_bar: f32,       // Pixel width per bar (offset 8)
    pub from_index: f32,       // Fractional left-most bar index (offset 12)
    pub y_scale: f32,          // Vertical pixel scale: height / (y_max - y_min) (offset 16)
    pub y_offset: f32,         // y_min - base_price (offset 20)
    pub bars_per_node: f32,    // Always 1.0 (offset 24)
    pub dpr: f32,              // Device Pixel Ratio (offset 28)
    pub up_color: [f32; 4],    // RGBA for bullish candle (offset 32)
    pub down_color: [f32; 4],  // RGBA for bearish candle (offset 48)
    pub body_width: f32,       // Precalculated body width in pixels (offset 64)
    pub wick_width: f32,       // Precalculated wick width in pixels (offset 68)
    pub _pad: [f32; 2],        // std140 16-byte alignment padding (offset 72)
}
```

### `VolumeUniforms` (64 Bytes)

```rust
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct VolumeUniforms {
    pub viewport_px: [f32; 2],
    pub px_per_bar: f32,
    pub from_index: f32,
    pub max_volume: f32,
    pub height_ratio: f32,
    pub bar_width: f32,
    pub max_h: f32,
    pub up_color: [f32; 4],
    pub down_color: [f32; 4],
}
```

### `GridUniforms` (32 Bytes)

```rust
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GridUniforms {
    pub viewport_px: [f32; 2],
    pub _padding: [f32; 2],
    pub color: [f32; 4],
}
```

### `LineUniforms` (80 Bytes)

```rust
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LineUniforms {
    pub viewport_px: [f32; 2],
    pub px_per_bar: f32,
    pub from_index: f32,
    pub y_scale: f32,
    pub y_offset: f32,
    pub dpr: f32,
    pub line_width: f32,
    pub is_area: f32, // 1.0 for area gradient, 0.0 for solid line
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
    pub color: [f32; 4],
    pub bottom_color: [f32; 4],
}
```

### `IndicatorUniforms` (48 Bytes)

```rust
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct IndicatorUniforms {
    pub viewport_px: [f32; 2],
    pub px_per_bar: f32,
    pub from_index: f32,
    pub y_scale: f32,
    pub y_offset: f32,
    pub dpr: f32,
    pub line_width: f32,
    pub color: [f32; 4],
}
```

### `MarkerUniforms` (32 Bytes)

```rust
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MarkerUniforms {
    pub viewport_px: [f32; 2],
    pub px_per_bar: f32,
    pub from_index: f32,
    pub y_scale: f32,
    pub y_offset: f32,
    pub dpr: f32,
    pub _pad: f32,
}
```

### `MarkerGpuInstance` (48 Bytes per Marker)

```rust
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MarkerGpuInstance {
    pub x: f32,           // Candle index (float)
    pub y_rel: f32,       // Candle low (for Up) or candle high (for Down) relative to base_price
    pub dir: f32,         // 0.0 for Up (pointing up / green), 1.0 for Down (pointing down / red)
    pub size: f32,        // Marker visual size in logical pixels (e.g. 10.0)
    pub stack_index: f32, // Stacking level (0.0, 1.0, 2.0...) on this candle
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
    pub color: [f32; 4],  // RGBA color
}
```

---

## `ViewParams` Struct

Passed to `ChartRenderer::render` to configure each frame:

```rust
pub struct ViewParams {
    pub from_index: f32,
    pub px_per_bar: f32,
    pub y_min: f32,
    pub y_max: f32,
    pub base_price: f32,
    pub count: usize,
    pub dpr: f32,
    pub up_color: [f32; 4],
    pub down_color: [f32; 4],
    pub clear_color: Option<wgpu::Color>,
    pub grid_color: Option<[f32; 4]>,
    pub horizontal_grid_y: Vec<f32>,
    pub vertical_grid_x: Vec<f32>,
    pub show_volume: bool,
    pub volume_max: f32,
    pub volume_height_ratio: f32,
    pub volume_up_color: [f32; 4],
    pub volume_down_color: [f32; 4],
    pub allow_zoom: bool,
    pub show_axes: bool,
    pub chart_type: chartmelogne_core::types::ChartType,
    pub show_indicators: bool,
}
```

### Default Parameters
```rust
let view = ViewParams::default();
// up_color: [0.03, 0.65, 0.55, 1.0] (#089981)
// down_color: [0.95, 0.25, 0.33, 1.0] (#f23645)
// volume_height_ratio: 0.20 (volume occupies bottom 20% of viewport)
// chart_type: ChartType::Candlestick
```

---

## WGSL Shaders Overview

All shader code is embedded as strings via `include_str!`:

| Constant | File | Description |
|---|---|---|
| `CANDLE_SHADER_SRC` | `candle.wgsl` | Instanced candle drawing (wick quad + body quad in a single pass). |
| `GRID_SHADER_SRC` | `grid.wgsl` | Screen-space horizontal/vertical grid line quads. |
| `VOLUME_SHADER_SRC` | `volume.wgsl` | Instanced volume histogram bars anchored at the bottom edge. |
| `LINE_SHADER_SRC` | `line.wgsl` | Instanced line segment extrusion and vertical area gradient polygon fill. |

### Instanced Candle Vertex Layout

The candle pipeline binds two vertex buffers:
1. **Buffer 0 (Step: `Vertex`, Stride: 12 bytes)**:
   - Location 0 (`Float32x3`): `[x_side (-0.5..0.5), y_anchor (0.0..1.0), part (0.0 = wick, 1.0 = body)]`.
2. **Buffer 1 (Step: `Instance`, Stride: 16 bytes)**:
   - Location 1 (`Float32x4`): `[open, high, low, close]` relative to `base_price`.

Inside `candle.wgsl`:
```wgsl
@vertex
fn vs_main(
    @location(0) unit: vec3<f32>,
    @location(1) ohlc: vec4<f32>,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    // 1. Calculate candle X center:
    let bar_center_x = (f32(instance_idx) + 0.5 - uniforms.from_index) * uniforms.px_per_bar;
    
    // 2. Determine wick vs body geometry:
    let is_body = unit.z > 0.5;
    let width = select(uniforms.wick_width, uniforms.body_width, is_body);
    let px_x = bar_center_x + unit.x * width;

    // 3. Project price Y coordinate:
    let open_y  = (ohlc.x - uniforms.y_offset) * uniforms.y_scale;
    let close_y = (ohlc.w - uniforms.y_offset) * uniforms.y_scale;
    let high_y  = (ohlc.y - uniforms.y_offset) * uniforms.y_scale;
    let low_y   = (ohlc.z - uniforms.y_offset) * uniforms.y_scale;

    // 4. Transform to NDC:
    let ndc_x = (px_x / uniforms.viewport_px.x) * 2.0 - 1.0;
    let ndc_y = (px_y / uniforms.viewport_px.y) * 2.0 - 1.0;
    // ...
}
```

---

## Standalone Usage Example

Below is a complete, working example using `wgpu` without any UI framework to render candles into an offscreen texture:

```rust
use chartmelogne_core::{
    candle_store::CandleStore,
    synthetic::{generate_synthetic_candles, SyntheticDataOptions},
    types::ChartType,
    viewport::Viewport,
};
use chartmelogne_render::{ChartRenderer, ViewParams};

async fn run_offscreen_render() {
    // 1. Initialize wgpu headless context
    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        })
        .await
        .expect("No suitable GPU adapter found");

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
        .expect("Failed to create wgpu device");

    // 2. Create renderer
    let mut renderer = ChartRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.resize(1920, 1080);

    // 3. Load data
    let candles = generate_synthetic_candles(SyntheticDataOptions {
        count: 50_000,
        start_price: 100.0,
        ..Default::default()
    });

    let mut store = CandleStore::new(candles.time.len(), 0);
    store.set_data(&candles, true).unwrap();

    renderer.upload_all(&store.ohlc, store.count());
    renderer.upload_volume(&store.volume_instances, store.count());

    // 4. Setup Viewport
    let mut viewport = Viewport::default();
    viewport.set_size(1920.0, 1080.0, 1.0);
    viewport.set_total_bars(store.count());
    viewport.set_visible_range(49_700.0, 50_000.0);

    let (y_min, y_max) = store.get_visible_min_max(viewport.from_index, viewport.from_index + viewport.bars_visible);
    viewport.update_auto_y(y_min, y_max, true);

    // 5. Build ViewParams
    let view_params = ViewParams {
        from_index: viewport.from_index as f32,
        px_per_bar: viewport.px_per_bar() as f32,
        y_min: viewport.y_min as f32,
        y_max: viewport.y_max as f32,
        base_price: store.base_price() as f32,
        count: store.count(),
        dpr: 1.0,
        chart_type: ChartType::Candlestick,
        show_volume: true,
        volume_max: store.get_visible_max_volume(viewport.from_index, viewport.from_index + viewport.bars_visible) as f32,
        ..Default::default()
    };

    // 6. Encode and submit render commands
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Offscreen Chart Render"),
    });
    renderer.render(&mut encoder, &view_params);
    queue.submit(Some(encoder.finish()));

    // The rendered pixels reside in renderer.texture()
}
```
