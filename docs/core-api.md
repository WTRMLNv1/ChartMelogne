# `chartmelogne-core` API Reference

The `chartmelogne-core` crate contains the data model, memory-optimized storage engine, 256-block hierarchical indexing, coordinate projection math, and axis tick generators. It has zero GPU, windowing, or UI dependencies.

---

## 1. `CandleStore`

Defined in `chartmelogne_core::candle_store`.

`CandleStore` is a high-performance columnar Structure-of-Arrays (SoA) container holding historical and live market data.

### Memory Layout
For 450,000 bars, total CPU memory is approximately **12.49 MB**:
- `time: Vec<f64>` (8 bytes per bar, epoch milliseconds UTC)
- `volume: Vec<u32>` (4 bytes per bar)
- `flags: Vec<u8>` (1 byte per bar: bit 0 = gap/invalid, bit 1 = reserved for markers)
- `ohlc: Vec<f32>` (16 bytes per bar, packed as interleaved `[open - base, high - base, low - base, close - base]`)
- `day_starts: Vec<u32>` (indices of the first bar of each local civil calendar day)
- `block_min / block_max / block_max_vol: Vec<f64>` (summary metrics for contiguous 256-bar blocks)

### Constructor

```rust
pub fn new(initial_capacity: usize, utc_offset_minutes: i32) -> Self
```
- `initial_capacity`: Pre-allocates internal vectors to prevent reallocation during streaming ingestion.
- `utc_offset_minutes`: Timezone offset (e.g., `330` for UTC+05:30 IST, `-300` for UTC-05:00 EST). Used for Howard Hinnant civil day calculations.

### Ingestion & Mutation Methods

#### `set_data`
```rust
pub fn set_data(&mut self, data: &CandleData, debug: bool) -> Result<(), ValidationError>
```
Populates the store with historical data in a single fused, cache-resident pass over 256-bar blocks:
- Validates data if `debug = true` (checks for NaN, strictly increasing timestamps, and `high >= max(open, close)`).
- Normalizes prices relative to `base_price = data.close[0]` into packed `f32` instances.
- Computes `day_starts` civil calendar boundaries.
- Generates 256-bar block min/max/volume summary metrics.

#### `append`
```rust
pub fn append(&mut self, bar: Bar) -> usize
```
Appends a completed candle in $O(1)$ amortized time. Updates `day_starts` and block summaries. Returns the index of the newly added candle.

#### `update_last`
```rust
pub fn update_last(&mut self, bar: Bar) -> usize
```
Updates the active candle in real time (the live tick hot path):
- Updates `ohlc` instance and volume.
- **Fast-path $O(1)$ check**: If the tick does not retract the block's existing high, low, or volume, it executes in $O(1)$ without rescanning the active block.
- Returns the index of the updated candle (`count - 1`).

### Query Methods

#### `get_visible_min_max`
```rust
pub fn get_visible_min_max(&self, from_index: f64, to_index: f64) -> (f64, f64)
```
Returns `(min_price, max_price)` across the visible bar range `[from_index, to_index]`.
- Uses hierarchical 256-bar block summaries:
  1. Head partial block: direct raw scan of remaining bars in the block.
  2. Aligned middle blocks: instant $O(1)$ lookup per block from `block_min` and `block_max`.
  3. Tail partial block: direct raw scan of initial bars in the block.

#### `get_visible_max_volume`
```rust
pub fn get_visible_max_volume(&self, from_index: f64, to_index: f64) -> u32
```
Returns the maximum volume within `[from_index, to_index]` using the 256-bar block volume index.

#### Buffer Byte Slices (Zero-Copy GPU Upload)
```rust
pub fn ohlc_bytes(&self) -> &[u8]
pub fn volume_bytes(&self) -> &[u8]
```
Returns direct raw byte slices of the active packed OHLC instances (`16 * count` bytes) and volume instances (`8 * count` bytes) for zero-copy upload into `wgpu::Buffer`.

#### Individual Bar Accessors
```rust
pub fn count(&self) -> usize
pub fn base_price(&self) -> f64
pub fn get_open(&self, i: usize) -> f64
pub fn get_high(&self, i: usize) -> f64
pub fn get_low(&self, i: usize) -> f64
pub fn get_close(&self, i: usize) -> f64
pub fn get_bar(&self, i: usize) -> Bar
pub fn get_memory_usage(&self) -> MemoryUsage
```

---

## 2. `Viewport`

Defined in `chartmelogne_core::viewport`.

`Viewport` manages camera pan, zoom, auto-scaling, and bi-directional coordinate projections between screen pixels and data coordinates.

### Constructor & Configuration

```rust
pub struct ViewportOptions {
    pub min_bars_visible: Option<f64>,      // Default: 10.0
    pub right_padding_ratio: Option<f64>,  // Default: 0.5 (up to 50% blank margin on right)
    pub y_padding_ratio: Option<f64>,      // Default: 0.05 (5% top/bottom margin)
}

pub fn new(options: ViewportOptions) -> Self
```

### Navigation & Scaling

```rust
// Update screen dimension & DPR
pub fn set_size(&mut self, width_px: f64, height_px: f64, dpr: f64)

// Set total candle count
pub fn set_total_bars(&mut self, total: usize)

// Pan horizontally by device screen pixels
pub fn pan_by_pixels(&mut self, delta_screen_px: f64)

// Pan vertically by screen pixels (when in manual Y mode)
pub fn pan_y_by_pixels(&mut self, delta_screen_py: f64)

// Zoom horizontally anchored around a screen pixel coordinate
pub fn zoom_at(&mut self, cursor_screen_px: f64, zoom_factor: f64)

// Zoom vertically anchored around a price level
pub fn zoom_y_at(&mut self, cursor_screen_py: f64, zoom_factor: f64)

// Set explicit visible bar index range
pub fn set_visible_range(&mut self, from_index: f64, to_index: f64)

// Reset zoom to view the entire dataset
pub fn fit_content(&mut self)

// Smooth or immediate Auto-Y tracking of price extremes
pub fn update_auto_y(&mut self, raw_min: f64, raw_max: f64, immediate: bool)
```

### Coordinate Transforms

```rust
// Screen pixel X -> Bar index
pub fn screen_px_to_bar_index(&self, screen_px: f64) -> f64

// Bar index -> Screen pixel X
pub fn bar_index_to_screen_px(&self, bar_index: f64) -> f64

// World Price -> Screen pixel Y
pub fn price_to_screen_px(&self, price: f64) -> f64

// Screen pixel Y -> World Price
pub fn screen_px_to_price(&self, screen_px: f64) -> f64
```

---

## 3. `PriceAxis`

Defined in `chartmelogne_core::price_axis`.

Generates monotonic, evenly-spaced "nice number" price ticks in `{1, 2, 5, 10} × 10^k`.

```rust
pub struct PriceTickOptions {
    pub price_decimals: Option<usize>,
    pub tick_size: Option<f64>,
    pub target_spacing_px: Option<f64>, // Default: 50.0 px
}

// Compute nice number step closest to raw_step
pub fn get_nice_price_step(raw_step: f64) -> f64

// Compute decimal places needed without heap allocation
pub fn get_precision(step: f64) -> usize

// Populate reusable vector (zero-allocation per frame)
pub fn generate_price_ticks_into(
    y_min: f64,
    y_max: f64,
    height_px: f64,
    options: &PriceTickOptions,
    ticks: &mut Vec<PriceTick>,
)

// Generate price ticks (allocates new vector)
pub fn generate_price_ticks(
    y_min: f64,
    y_max: f64,
    height_px: f64,
    options: &PriceTickOptions,
) -> Vec<PriceTick>
```

---

## 4. `TimeAxis`

Defined in `chartmelogne_core::time_axis`.

Generates date and time labels based on timestamp spans and zoom levels:
- **Intraday Zoom** (`span < 24 hours`): Labels formatted as `"HH:mm"`, with day boundaries formatted as `"d MMM"`.
- **Daily Zoom** (`1 day <= span < 60 days`): Labels formatted as `"d MMM"`, with month boundaries formatted as `"MMM 'yy"`.
- **Monthly Zoom** (`span >= 60 days`): Labels formatted as `"MMM yyyy"`.

### Constructor

```rust
pub struct TimeAxisOptions {
    pub utc_offset_minutes: Option<i32>,  // Default: 330
    pub min_tick_spacing_px: Option<f64>, // Default: 80.0 px
}

pub fn new(options: TimeAxisOptions) -> Self
```

### Methods

```rust
// Generate ticks using internal tick pool (zero allocations in steady state)
pub fn generate_ticks(
    &mut self,
    time_array: &[f64],
    count: usize,
    from_index: f64,
    bars_visible: f64,
    viewport_width_px: f64,
    day_starts: Option<&[u32]>,
    day_count: Option<usize>,
) -> &[TimeTick]

// Convert epoch time in ms to local civil date components
pub fn get_local_parts(&self, time_ms: f64) -> LocalTimeParts

// Howard Hinnant algorithm: unix day number -> civil year, month (0..11), day (1..31)
pub fn civil_from_days(z: i64) -> CivilDate
```

---

## 5. `Synthetic` Data Generator

Defined in `chartmelogne_core::synthetic`.

Generates deterministic, reproducible synthetic market data for unit testing and benchmarking:

```rust
pub struct SyntheticDataOptions {
    pub count: usize,          // e.g. 450_000
    pub start_price: f64,      // e.g. 1000.0
    pub start_time_ms: f64,    // Epoch ms
    pub interval_ms: f64,      // Bar duration (e.g. 60_000.0 for 1-minute bars)
    pub volatility: f64,       // e.g. 0.002
    pub seed: u32,             // Seed for Mulberry32 PRNG
}

pub fn generate_synthetic_candles(options: SyntheticDataOptions) -> CandleData
```

---

## 6. Common Types

Defined in `chartmelogne_core::types`.

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bar {
    pub time: f64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: u32,
    pub flags: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CandleData {
    pub time: Vec<f64>,
    pub open: Vec<f64>,
    pub high: Vec<f64>,
    pub low: Vec<f64>,
    pub close: Vec<f64>,
    pub volume: Vec<u32>,
    pub flags: Option<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChartType {
    Candlestick,
    Line,
    Area,
}

---

## 7. Host-Provided Indicators (`HostIndicator`)

ChartMelogne does not compute indicator formulas internally (allowing developers to use any quantitative library, such as `ta-lib`, `polars`, or custom loops). The host application computes indicator values and passes them as a `HostIndicator`.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostIndicator {
    /// Unique identifier for this indicator series (e.g. "ema_20")
    pub id: String,
    /// Human-readable display label for HUD and legends (e.g. "EMA 20")
    pub name: String,
    /// RGBA line color with components in 0.0..1.0
    pub color: [f32; 4],
    /// Stroke width in logical pixels (default: 1.5)
    pub line_width: f32,
    /// Pre-calculated values per bar index. None represents warmup/missing periods.
    pub values: Vec<Option<f64>>,
    /// Whether this indicator is currently visible
    pub visible: bool,
}
```

### Constructors & Helpers

```rust
// 1. Create with Vec<Option<f64>> (None for warmup):
let ind = HostIndicator::new("SMA 20", [0.941, 0.725, 0.043, 1.0], sma_values);

// 2. Create from &[f64] slice with NaNs:
let ind = HostIndicator::from_f64_slice("RSI 14", [0.612, 0.153, 0.690, 1.0], &raw_slice);

// 3. Builder methods:
let ind = ind.with_line_width(2.0).with_visibility(true);
```

### `CandleStore` Indicator Methods

```rust
// Register or replace indicator
store.add_indicator(ind);

// Remove by id
store.remove_indicator("SMA 20");

// Clear all
store.clear_indicators();

// Convert to f32 relative offsets for GPU streaming:
let relative_f32 = store.get_indicator_relative_f32(&ind);
```

### Demo Calculation Helpers (`synthetic`)

For unit testing and mock data generation, `chartmelogne_core::synthetic` provides:
- `compute_sma(values: &[f64], period: usize) -> Vec<Option<f64>>`
- `compute_ema(values: &[f64], period: usize) -> Vec<Option<f64>>`

---

## 8. Visual Signal & Trade Markers (`ChartMarker`)

`ChartMarker` allows the host application to draw upward-pointing (buy/green) and downward-pointing (sell/red) triangle markers at specific candle bars and price levels.

### Flexibility: Pre-Mapped Bar Index or Auto-Mapped Timestamp

Markers can be placed using:
1. **Direct Candle Index** (`candle_index: usize`): If the host has already mapped the signal to a bar, zero lookup overhead is incurred.
2. **Timestamp** (`time: f64`): If the host supplies a Unix timestamp, `CandleStore` automatically binary-searches the candle timestamps and maps the marker to the closest bar.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartMarker {
    /// Candle index where the marker is placed (if pre-mapped by host)
    pub candle_index: Option<usize>,
    /// Timestamp in epoch seconds/ms (if time-based; auto-mapped to closest candle by store)
    pub time: Option<f64>,
    /// Price level in currency units
    pub price: f64,
    /// Up (triangle pointing up, green) or Down (triangle pointing down, red)
    pub direction: MarkerDirection,
    /// Optional RGBA color override. If None, Up defaults to green and Down defaults to red.
    pub color: Option<[f32; 4]>,
    /// Marker visual size in logical pixels (default: 10.0)
    pub size: f32,
    /// Optional textual label/tag (e.g. "BUY", "SELL", order quantity)
    pub text: Option<String>,
}

pub enum MarkerDirection {
    Up,   // Points up, default color: [0.149, 0.651, 0.604, 1.0] (Emerald Green #26a69a)
    Down, // Points down, default color: [0.937, 0.325, 0.314, 1.0] (Coral Red #ef5350)
}
```

### Constructors & Helpers

```rust
// 1. Pre-mapped by candle index
let buy_marker = ChartMarker::up(50, 48200.0).with_text("BUY");
let sell_marker = ChartMarker::down(120, 52100.0).with_text("SELL");

// 2. Auto-mapped by timestamp
let ts_buy = ChartMarker::up_at_time(1700000000.0, 48200.0);
let ts_sell = ChartMarker::down_at_time(1700050000.0, 52100.0);

// 3. Custom color and size
let custom_marker = ChartMarker::up(80, 50000.0)
    .with_color([1.0, 0.84, 0.0, 1.0]) // Gold
    .with_size(14.0);
```

### `CandleStore` Marker Methods

```rust
// Add markers to store
store.add_marker(buy_marker);
store.set_markers(vec![buy_marker, sell_marker]);

// Binary search closest bar for any timestamp
let nearest_bar_idx = store.find_bar_index_by_time(1700000000.0);

// Resolve bar index for a marker (direct index or binary search from time)
let bar_idx = store.resolve_marker_bar_index(&marker);

// Convert to packed GPU instances (32 bytes per marker)
let gpu_instances: Vec<MarkerGpuInstance> = store.get_markers_gpu_instances();
```


#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewportMode {
    Auto,
    Manual,
}
```
