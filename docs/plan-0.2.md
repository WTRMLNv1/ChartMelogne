# ChartMelogne: High-Performance Chart Rendering Library (Spec v0.2)

Name: ChartMelogne (published to npm; full source lives in the ChartMelogne repository). Status: draft. First consumer: AlgoMLN. Designed to be host-independent.

## 1. Purpose and scope

A standalone chart rendering library for dense financial time series (OHLCV candles, lines, histograms) that stays smooth at hundreds of thousands of bars on weak hardware. It renders; it does not fetch data, compute indicators, or place orders.

### Goals

- Smooth pan and zoom over 450k+ bars (5 years of 1m NSE data is about 360 x 250 x 5 = 450k) on an 11th gen i5 with integrated graphics.
- Cost per frame proportional to screen pixels, not to number of bars.
- Live-tick updates that touch O(log n) data, not O(n).
- Zero host dependencies. Works in a plain web page, Tauri, Electron, or anything with a canvas.
- Reproducible benchmarks that live in the repo.

### Non-goals (v1)

- Drawing tools (trendlines, fibs), indicator computation, order entry, chart-type transforms (Heikin-Ashi, Renko; the host transforms data before passing it in).
- Kinetic scrolling, touch gestures beyond basic pinch and drag.
- Server rendering or image export (maybe later).

## 2. Targets

Measured on the reference machine (11th gen i5 laptop, integrated GPU, 1080p, DPR 1 and DPR 1.25), dataset: 450,000 synthetic 1m bars from a fixed seed.

| Metric | Target |
| --- | --- |
| Pan/zoom frame time p95 | < 16.6 ms |
| Pan/zoom frame time p99 | < 25 ms |
| `setData` of 450k bars to first paint | < 150 ms (Phase 1), < 100 ms (after pyramid) |
| Live tick (`updateLast`) to painted | < 4 ms |
| Append bar | < 1 ms CPU |
| Memory, 450k bars, one candle series | < 60 MB total (JS + WASM + GPU buffers) |
| Idle CPU (no input, no data) | \~0% (no continuous rAF loop) |

These are gates, not hopes: each milestone below states which gate it must pass.

## 3. Architecture

### 3.1 Phased delivery

The design has a Rust core and a TS/WebGL2 frontend, but it is built in phases so that something runs at every step.

- Phase 1: pure TS + WebGL2, no decimation. Establishes the baseline and finds the real bottleneck.
- Phase 2: Rust core (pyramid) compiled to WASM, swapped in behind the same TS interface.
- Phase 3: axes, session calendar, crosshair and overlay.
- Phase 4: more series types, multi-pane, live-update path, themes.
- Phase 5 (optional): wgpu backend (native window and/or WebGPU).

### 3.2 Package layout (end state)

```
chartmln/
  core/    Rust crate, no host deps: store, pyramid, scales, ticks, hit-test
  wasm/    wasm-bindgen wrapper over core
  web/     TS package: WebGL2 backend, input, overlay, public API
  bench/   demo app + benchmark harness + fixtures
  docs/
```

In Phase 1 only `web/` and `bench/` exist. Do not create `core/` or a `Backend` interface until Phase 2, and only extract a backend interface when a second backend is actually started.

### 3.3 Layers (rendering)

Three stacked canvases in one container:

1. Static layer (Canvas2D): background, grid, axis labels. Redrawn only when the viewport range, size, theme or tick set changes.
2. Series layer (WebGL2): candles, lines, histograms. Redrawn on viewport or data change.
3. Overlay layer (Canvas2D): crosshair, tooltip, last-price line, markers. Redrawn on pointer move, cheap.

A pointer move repaints only the overlay. A pan repaints static (labels) and series. Rendering is on demand: input or data marks the chart dirty, and one `requestAnimationFrame` is scheduled; there is no permanent loop.

## 4. Data model

### 4.1 Columnar storage (SoA)

Each candle series holds parallel arrays, never an array of objects:

- `time`: Float64Array (epoch milliseconds, UTC; library never assumes a timezone)
- `open`, `high`, `low`, `close`: Float64Array
- `volume`: Float64Array

F64 in the store keeps full precision for the host; only the GPU upload is f32 (see section 7.2). Capacity is fixed at creation and grows by doubling.

Constraints (validated on `setData` in debug builds, documented in release): `time` strictly increasing; `high >= max(open, close)`; `low <= min(open, close)`; no NaN in OHLC (use a separate gap flag, see below). Violations are reported with the first bad index.

### 4.2 The x-axis is the bar index

The chart plots by bar index, not by timestamp. Markets have overnight and weekend gaps; a time axis would leave empty space. Labels come from the session calendar (section 8) which maps index to time. This means the x-coordinate of bar `i` is simply `i`, which keeps shader math trivial and exact.

### 4.3 Gaps

Missing bars inside a session (a zero-volume flat bar, or a real hole) are the host's concern. The store accepts a per-bar flag byte (`Uint8Array`): bit 0 = gap/no-data (not drawn). Bit 1 reserved for markers. Default flags are zero.

## 5. The min/max pyramid (Phase 2)

### 5.1 Structure

Level 0 is the raw bars. Level `k` has one node per `2^k` consecutive raw bars, where node `j` covers raw bars `[j * 2^k, (j+1) * 2^k)`.

A node stores: `open` (first child's open), `close` (last child's close), `high` (max), `low` (min), `volume` (sum), and the raw index of its first bar. Levels are stored as separate SoA arrays. Total pyramid memory is about 1x the raw data (geometric series 1/2 + 1/4 + ...), so the full store is about 2x.

A partial trailing bucket is a valid node (it aggregates the bars that exist so far).

### 5.2 Build

Built in one pass bottom-up on `setData`: O(n) total. Stop at the level where one node covers the entire series.

### 5.3 Update

`append` and `updateLast` mutate leaf `i`, then for each level `k` recompute node `i >> k` by merging its two children (`2j`, `2j+1`, second may be absent). Recomputing from children, rather than patching with max/min, stays correct even if a bar's high is later corrected downward. Cost: O(log n), about 19 merges for 450k bars.

### 5.4 Viewport query

Given visible raw range `[lo, hi)` and plot width `W` in device pixels:

1. `barsPerPixel = (hi - lo) / W`
2. `k = max(0, floor(log2(barsPerPixel)))` (aim for at least one pixel per node, so nodes are never narrower than a pixel).
3. Node range: `[lo >> k, ceil(hi / 2^k))`.
4. Return that slice of level `k` as the draw set.

Result: at most about `2W` nodes are ever drawn (when zoomed out), and exactly the raw bars when `barsPerPixel < 2`. A node draws like a candle: body from its open to close, wick from low to high.

### 5.5 Level selection hysteresis

To avoid flicker at a level boundary during slow zoom, hold the current `k` until `barsPerPixel` leaves `[2^k * 0.7, 2^(k+1) * 1.4]`.

### 5.6 Correctness reference

A naive reference decimator (loop over raw bars, bucket by `floor(i / 2^k)`) lives in the test suite. Property tests (fixed seeds plus randomized with proptest in Rust) assert pyramid output equals naive output for random data, random appends and random `updateLast` sequences.

## 6. Viewport and interaction

### 6.1 State

`viewport = { fromIndex: f64, barsVisible: f64, yMode, yMin, yMax }`, with `fromIndex` fractional (sub-bar panning is smooth).

### 6.2 Transform

The whole pan/zoom is a uniform transform: `screenX = (barIndex - fromIndex) * pxPerBar`. Pan and zoom change uniforms only; no instance buffer is rebuilt and nothing is re-uploaded.

### 6.3 Behaviour

- Drag pans horizontally; wheel zooms anchored at the cursor; Shift+wheel or horizontal wheel pans.
- Zoom clamps: minimum 10 bars visible, maximum all bars plus right padding.
- Right padding: the user may pan past the last bar by up to 50% of the width.
- Follow-live mode: if the last bar is visible when a bar is appended, the view scrolls to keep it at the same screen position; otherwise the view is untouched.
- Y axis: auto-fit to the visible bars' min/max (from the pyramid, so O(pixels)) with 5% padding, with an option to lock the range after the user drags the y axis.

### 6.4 Auto-fit cost

Visible min/max is read from the same decimated node set used for drawing, not by scanning raw bars. Smooth y-scale animation is a short exponential ease on `yMin`/`yMax` only.

## 7. Rendering (WebGL2)

### 7.1 Candle draw: one call per series

A unit geometry of 12 vertices (two quads: wick, then body) is drawn instanced, once per candle:

```
gl.drawArraysInstanced(gl.TRIANGLES, 0, 12, instanceCount)
```

Per-instance attributes (divisor 1), tightly packed f32:

| Attribute | Type | Meaning |
| --- | --- | --- |
| `x` | f32 | bar index (or node start index, in pyramid mode) |
| `o,h,l,c` | f32 x4 | prices relative to `basePrice` |

20 bytes per candle; 450k candles = 9 MB. Direction (up/down) and colour are derived in the vertex shader from `c >= o`, so there is no per-instance colour data. Node width in pyramid mode is a uniform (`barsPerNode`).

WebGL2 has no `baseInstance`. To draw a sub-range, bind attribute pointers with a byte offset to the first visible instance instead. In Phase 1 the full series is uploaded once and the visible slice is selected this way.

### 7.2 Precision

GPU prices are f32 relative to a per-series `basePrice` (for example the first close). F32 has about 7 digits, so even a stock at Rs 1,30,000 keeps sub-paisa resolution once the large base value is removed. The x value is a bar index, exactly representable in f32 up to 16.7M.

### 7.3 Pixel snapping

The vertex shader snaps candle x-centres and wick widths to whole device pixels. Wick width is `max(1 device px, 0.1 * bodyWidth)`. Body height has a minimum of 1 device pixel so dojis stay visible. Body width is `clamp(pxPerBar * 0.7, 1, 30)` device pixels; below 3 px per bar, bodies are dropped and only 1 px wicks are drawn.

### 7.4 Uniforms

`pxPerBar`, `fromIndex`, `yScale`, `yOffset`, `viewportPx`, `basePrice`, `barsPerNode`, up colour, down colour, wick width, min body height. All colours come from the active theme.

### 7.5 Other series types

- Line: vertices as a triangle strip of per-segment quads (instanced), with width in pixels and miter-free joins; decimated min/max per pixel column so a line over 450k points is about 2W segments.
- Histogram: instanced bars from a baseline; volume uses candle direction for colour.
- Area: line plus a fill instance pass (later).

### 7.6 Resize and DPR

`ResizeObserver` on the container; canvas backing size = CSS size x DPR rounded; re-snap on DPR change. A resize invalidates all layers and re-fits axes but never re-uploads series data.

### 7.7 Markers and trade overlays

Markers are required for backtest review and live trading (signals, entries, exits), so they are built early, at M3.5, right after the overlay layer. They are columnar like everything else, sorted by bar index:

| Column | Type | Meaning |
| --- | --- | --- |
| `index` | Uint32Array | bar index the marker belongs to |
| `kind` | Uint8Array | arrow up, arrow down, circle, square, diamond, flag |
| `anchor` | Uint8Array | above bar, below bar, or at a given price |
| `price` | Float32Array (optional) | used only when anchor is "at price" |
| `color` | Uint8Array | theme token id (buy, sell, neutral, profit, loss) |
| `size` | Uint8Array | small, medium, large |
| `id` | Uint32Array | host id returned in events (for example a trade id) |
| `text` | string table (optional) | short label, 12 characters at most |

**Rendering.** One instanced draw call per marker set. Each instance is 16 bytes: x, y (price relative to `basePrice`), and a packed word holding kind, size, colour and flags. Shapes are signed-distance-field sprites in the fragment shader, so edges stay sharp at any DPR. Marker size is fixed in pixels and does not scale with zoom. For above-bar and below-bar anchors the library reads the candle's high or low from the series store when the markers are uploaded, then offsets by 6 px, so the host never supplies a y for those. Only the visible index range is drawn, found by binary search.

**Labels.** Marker text is drawn on the overlay layer, and only when `pxPerBar` is at least 12. Zoomed-out views show shapes only.

**Density.** When `barsPerPixel` is 1 or more, several markers can land in one pixel column. Markers with the same kind and colour in the same column merge into one drawn marker with a count badge. This is a bounded per-frame pass over the visible markers only.

**Interaction.** `markerHover` and `markerClick` events return the host `id`. Hit-testing is a binary search on `index`, then a pixel-distance check. Tooltip content comes from a host callback; the library does not format trade data.

**Trade overlays.** Two small primitives, each one instanced draw call:

- Levels: horizontal price lines (stop-loss, take-profit, entry) from `fromIndex` to `toIndex`, or open-ended to the right edge, with an optional label chip on the price axis.
- Segments: a line from (indexA, priceA) to (indexB, priceB) for entry-to-exit connectors, coloured by profit or loss, optionally dashed.

```ts
const trades = chart.addMarkers({ pane: 0, capacity: 100_000 });
trades.setData({ index, kind, anchor, color, size, id });
trades.append({ index, kind, anchor, color, size, id }); // live signals
trades.clear();

chart.indexAtTime(ms);              // binary search on candle time
const levels = chart.addLevels();
levels.add({ price, fromIndex, toIndex, color, label });
chart.on('markerClick', (e) => { /* e.id */ });
```

### 7.8 Symbol compare (later version)

Step 1 (v1.x): a compare series on the same pane, normalised to percent change. All series in the pane switch to a percent axis, computed in the vertex shader as `(price / anchorPrice - 1) * 100`. The anchor is the first visible bar, so panning rebases by changing one uniform; nothing is re-uploaded.

Alignment: symbols have different bar sets (F&O stocks have 360 bars a day, others 375, and some bars are missing). The compare series is merged onto the primary symbol's bar index by timestamp in one O(n) pass. Bars the compare symbol lacks are marked as gaps and not drawn.

Step 2 (later): an optional second price axis on the left for the compare symbol, with its own y-scale, so it can be viewed in absolute price instead of percent. API shape: `chart.addLine({ axis: 'left' })`. This needs a per-series axis binding in the pane, which is why it follows the percent mode rather than replacing it.

The compare series gets its own pyramid, and the pane's auto-fit reads visible min/max from both.

## 8. Axes and session calendar

### 8.1 Price axis

"Nice number" ticks: step chosen from `{1, 2, 5} x 10^k` so there are about `height / 50 px` ticks. Tick formatting uses the host-provided `priceDecimals` (default derived from tick size) and optional `tickSize` for last-price rounding.

### 8.2 Time axis

Because x is a bar index, labels are generated from the timestamps of the visible bars. The library takes `utcOffsetMinutes` (for IST, 330) and finds day boundaries by comparing local calendar dates of adjacent bars. Label granularity adapts to zoom:

- Under 1 day visible: `HH:mm`, with the date shown at day starts.
- Days to weeks: day starts labelled `dd MMM`.
- Months or more: month and year at month starts.

No session lengths are baked in: 360-bar and 375-bar days both fall out of the timestamps. A host may pass an optional `SessionCalendar` hook (`isDayStart(i)`, `label(i, level)`) to override defaults, for example to mark holidays.

### 8.3 Text rendering

Canvas2D only, on the static layer. Measure results are cached by string, and fonts are loaded before the first measure. Text is not drawn in WebGL.

## 9. Live updates

- `append(bar)`: write to the next slot, `bufferSubData` 20 bytes, update pyramid (section 5.3), mark dirty.
- `updateLast(bar)`: same, on the last slot. Ticks within one bar call this repeatedly. Multiple updates within one frame are coalesced into one upload.
- Capacity growth: allocate 2x, copy once, rebind. This is rare and amortised.
- Follow-live behaviour per section 6.3.
- The host may batch (`appendMany`) for reconnect backfills.

## 10. Public API (TS)

```ts
const chart = createChart(container, {
  theme: darkTheme,
  utcOffsetMinutes: 330,
  priceDecimals: 2,
});

const candles = chart.addCandles({ capacity: 500_000 });
candles.setData({ time, open, high, low, close, volume }); // typed arrays, ownership transferred
candles.append(bar);
candles.updateLast(bar);

const ema = chart.addLine({ pane: 0, color: '#...' });
ema.setData(time, values);

const vol = chart.addPane({ heightRatio: 0.2 }).addHistogram();
vol.setData(time, volume);

chart.setVisibleRange(fromIndex, toIndex);
chart.fitContent();
chart.on('crosshair', (e) => { /* index, time, ohlcv, price */ });
chart.on('visibleRangeChange', (r) => { /* ... */ });
chart.destroy();
```

Rules: `setData` takes typed arrays (not arrays of objects) and may take ownership to avoid a copy. The library never mutates arrays it has not been given ownership of. The API is synchronous from the host's view; WASM init is a single awaited `await init()` at startup (Phase 2 onward).

## 11. Theming

A theme is a plain object of tokens: background, grid, axis text, crosshair, up colour, down colour, wick colour mode, font family and size. The library ships `darkTheme` and `lightTheme`. Up/down colours are semantic and separate per theme, matching AlgoMLN's rule of monochrome chrome with colour used only where it carries meaning.

## 12. Testing

### 12.1 Unit and property tests (Rust core, Phase 2)

- Pyramid equals the naive reference for random data, appends and `updateLast` sequences.
- Level selection and hysteresis (no flicker across a sweep of zoom values).
- Tick generation: monotonic, evenly spaced, count within bounds, for ranges from 1e-4 to 1e7.
- Calendar: day-boundary detection for 360-bar, 375-bar and partial days.

### 12.2 Visual regression

Golden screenshots at fixed viewports (a seeded dataset, three zoom levels, both themes) compared with a small pixel tolerance. Updated only deliberately.

### 12.3 Pixel-vs-reference check

For a small dataset, a Canvas2D reference renderer draws the same candles; the WebGL output must match within tolerance. This catches shader math mistakes.

## 13. Benchmark protocol

All numbers must be reproducible. The `bench/` app:

- Dataset: seeded random-walk 1m OHLCV, 450,000 bars, generated in-page (no network). Seeds and generator are committed.
- Scenarios, each 10 s: continuous drag pan; continuous wheel-zoom sweep (full range to 100 bars and back); live tick at 100 updates/s on the last bar; append one bar per second; resize drag.
- Metrics: frame time per frame (p50, p95, p99, max), dropped frames (over 20 ms), JS heap, number of `bufferData`/`bufferSubData` bytes per second, time from `setData` to first paint.
- Output: a JSON result file stamped with browser, OS, GPU string, DPR, window size, git commit. Results are committed to `bench/results/`.
- The same scenarios run unchanged for every phase, so regressions and wins are directly comparable.

## 14. Milestones

Each milestone has a done-when gate. Do not start the next until the gate passes.

### M0: Baseline (Phase 1)

Single HTML/TS demo. Synthetic data generator. WebGL2 instanced candles, the full series uploaded once, uniform-based pan and zoom, on-screen FPS/frame-time readout. No axes, no decimation.

Gate: 450k bars pan and zoom with real numbers recorded on the reference machine. Decision recorded: where the time actually goes (upload, vertex work, overdraw at zoomed-out, JS).

### M1: Reference and harness

Naive decimator, benchmark scenarios and JSON output, Canvas2D reference renderer.

Gate: scenarios run end to end; M0 results saved as the baseline.

### M2: Pyramid (Rust core + WASM)

`core` crate with store and pyramid, property tests, WASM wrapper, TS integration behind the same series interface.

Gate: pyramid tests green; frame-time p95 clearly better than M0 at full zoom-out, or an explicit written finding that the baseline was already enough.

### M3: Axes and calendar

Price ticks, time labels, grid, crosshair, overlay layer, last-price line.

Gate: no regression of more than 10% on M2 frame times; label tests green.

### M3.5: Markers

Instanced SDF marker sprites, anchor resolution from the candle store, density merging, hit-testing and events, levels and entry/exit segments (section 7.7). The benchmark app gains a scenario with 50,000 markers.

Gate: with 50,000 markers loaded, pan and zoom p95 frame time stays within 10% of the M3 numbers; marker hit-test under 1 ms; marker tests green (anchor resolution, merging, binary-search range).

### M4: Series and panes

Line, histogram, multi-pane, shared x-transform, live-update path, themes.

Gate: live-tick scenario under 4 ms to painted; memory under 60 MB at 450k bars.

### M5 (optional): wgpu backend

Only if a native no-webview consumer exists or WebGL2 benchmarks hit a real limit. Reuses the core's instance buffers; adds a `Backend` interface at that point, not before.

## 15. AlgoMLN integration notes

- Rust to UI: send columns as binary through Tauri's IPC (a `Channel` or a custom protocol response) and wrap them as typed arrays on arrival. Avoid JSON for bar data.
- Backtest results: the host sends the OHLCV columns plus indicator columns and trade markers as separate arrays.
- Indicators are computed in AlgoMLN's Rust engine, not in the chart; the chart renders the resulting columns.
- The adapter that bridges AlgoMLN data to the chart lives in the AlgoMLN repo, not in ChartMelogne.

## 16. Open questions

- Markers: decided, in scope for v1 and built early at M3.5 (section 7.7).
- Symbol compare: decided, percent-normalised in a later version, then an optional separate left price axis after that (section 7.8).
- Pyramid as a standalone crate: not in v1. It stays an internal module until a concrete non-chart use exists.
- Publish target: npm. The ChartMelogne repository holds the full source (core, wasm, web, bench). Licence still to decide.
