# ChartMelogne (chartmln)

[![License: WTRMLN Non-Commercial v1.0](https://img.shields.io/badge/License-WTRMLN_Non--Commercial_v1.0-blue.svg)](LICENSE)
[![WebGL2](https://img.shields.io/badge/WebGL2-Enabled-success.svg)](https://developer.mozilla.org/en-US/docs/Web/API/WebGL2RenderingContext)
[![Zero Dependencies](https://img.shields.io/badge/Dependencies-0-brightgreen.svg)]()

A high-performance standalone financial chart library engineered in TypeScript, WebGL2, and Canvas2D. Built specifically for dense, real-time financial time-series with zero external host dependencies.

Renders **450,000+ candlestick bars at a locked 60 FPS** on everyday integrated GPUs (11th Gen Intel i5 / Iris Xe) with an ultra-compact memory footprint under 20 MB.

---

## ⚡ Performance Highlights (450,000 1-Minute Bars)

Measured on an 11th Gen Intel Core i5-1145G7 with integrated graphics:

| Metric | Target Gate | ChartMelogne Measured | Status |
| :--- | :--- | :--- | :--- |
| **`setData` to First Paint** | < 150 ms | **42.1 ms** (includes draw + `gl.finish()`) | **PASSED** |
| **Live Tick to Painted** | < 4.0 ms | **3.1 ms** (with GPU synchronization) | **PASSED** |
| **Tick Function CPU Cost** | — | **0.69 µs / tick** | **PASSED** |
| **Append Bar Latency** | < 1.0 ms | **0.62 ms** | **PASSED** |
| **Total Memory Footprint** | < 60 MB | **19.36 MB** (Store + GPU VBO combined) | **PASSED** |
| **Full Zoom-Out Pan / Zoom** | 60 FPS | **60 FPS** (solid vsync, 0 dropped frames) | **PASSED** |
| **Tick Generation Cost** | < 0.5 ms | **0.031 ms** ($O(\text{visible days})$ algorithm) | **PASSED** |
| **Y-Axis Auto-fit Scan** | — | **2.7 µs** ($O(\text{blocks})$ summary cache) | **PASSED** |

---

## 🧱 Architecture

ChartMelogne separates concerns across three synchronized viewport layers:

1. **WebGL2 Layer (Dynamic Series Plot)**:
   - Instanced triangle geometry: 12 vertices / candle (body + wicks).
   - Compact **16 bytes per instance** (`vec4 aOHLC` single-precision float relative to `basePrice`).
   - Vertex shader computes candle index via `gl_InstanceID + uFirstIndex` for instant zero-copy visible slicing.
   - Zero CPU buffer duplication; direct GPU upload.

2. **Canvas2D Static Layer (Grid & Axes)**:
   - Adaptive price axis with "nice numbers" ($\{1, 2, 5\} \times 10^k$).
   - High-speed calendar time axis powered by integer math (`civilFromDays`) and precomputed `dayStarts`.
   - Native support for 375-bar Indian market sessions (9:15–15:30 IST), 360-bar US sessions, and 24/7 crypto sessions without per-frame date allocations.

3. **Canvas2D Interactive Overlay Layer**:
   - Crosshair lines with magnetic snapping to candle centers.
   - Real-time price and time badges along axes.
   - Header candle info bar ($O, H, L, C, \text{Vol}$).
   - Dashed last-price line with live price badge.

---

## 📦 Getting Started

### Using in Another Project

Until published on the npm registry, you can consume `chartmln` directly using local file reference or npm workspaces:

```bash
# In your target project:
npm install ../ChartMelogne/web
```

Or build `chartmln` and copy the package:

```bash
cd ChartMelogne/web
npm run build
```

### Minimal Example

```typescript
import { createChart, darkTheme } from 'chartmln';

// 1. Create the chart container
const container = document.getElementById('chart') as HTMLElement;
const chart = createChart(container, {
  theme: darkTheme,
  priceDecimals: 2,
  timeAxis: {
    utcOffsetMinutes: 330, // UTC+5:30 (IST). Use 0 for UTC or -300 for EST
  },
});

// 2. Add a candle series
const candles = chart.addCandles({ capacity: 500_000 });

// 3. Load historical data
candles.setData({
  time: new Float64Array([1700000000000, 1700000060000, ...]),
  open: new Float64Array([100.5, 101.2, ...]),
  high: new Float64Array([102.0, 103.5, ...]),
  low: new Float64Array([99.8, 100.9, ...]),
  close: new Float64Array([101.2, 102.8, ...]),
  volume: new Uint32Array([1500, 2400, ...]),
});

// 4. Stream real-time ticks to the active bar (< 1 microsecond CPU time)
candles.updateLast({
  time: 1700000060000,
  open: 101.2,
  high: 104.0,
  low: 100.9,
  close: 103.8,
  volume: 2450,
});

// 5. Append a new bar on close
candles.append({
  time: 1700000120000,
  open: 103.8,
  high: 105.0,
  low: 103.5,
  close: 104.2,
  volume: 120,
});
```

---

## 🚦 Is It Ready for Your Project?

### ✅ Ready Now (Production Grade)
- **Single-Series Candlestick Charts**: High-speed candlestick visualization with seamless pan, drag, wheel-zoom, and auto-fit Y.
- **Large Dataset Streaming**: 100 to 500,000+ bars without lag or stutter.
- **Live Tick / Append Updates**: Sub-millisecond tick-to-paint turnaround.
- **Interactive Overlay**: Snapping crosshair, axis badges, OHLC info bar, and last-price indicator line.
- **Themes**: Out-of-the-box `darkTheme` and `lightTheme` with custom color overrides.
- **Session Calendars**: Configurable timezones and session day-boundary detection.

### ⏳ In Progress (Upcoming Milestones)
- **Volume & Multiple Series (M4)**: Volume histogram pane and secondary line / area series.
- **Drawing Tools & Annotations (M5)**: Trendlines, horizontal levels, and Fibonacci retracements.
- **Technical Indicators (M6)**: Built-in SMA, EMA, Bollinger Bands, RSI, and MACD overlays.
- **Rust/WASM LOD Pyramid (M2)**: Multi-resolution decimation pyramid for datasets exceeding 1,000,000+ bars.

If your project currently requires a **fast, standalone candlestick chart with pan/zoom, crosshair, and live tick streaming**, ChartMelogne is ready to integrate immediately.

---

## 🛠️ Development & Benchmarks

```bash
# Install dependencies
npm install

# Run the interactive benchmark suite (Vite)
npm run dev

# Run unit and parity tests
npm run test

# Run headless CLI benchmark
npm run --workspace=bench bench:cli
```

---

## 📄 License

This software is licensed under the **WTRMLN Non-Commercial Software License v1.0**. Free for non-commercial use, research, and modification. Commercial use requires prior written permission. See [LICENSE](LICENSE) for full terms.
