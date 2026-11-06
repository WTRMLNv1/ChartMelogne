import type { Chart, CandleSeries } from 'chartmln';
import { MetricsTracker, type BenchmarkScenarioResult } from './metrics.js';

export class BenchmarkRunner {
  private chart: Chart;
  private series: CandleSeries;
  private container: HTMLElement;
  private metrics: MetricsTracker;

  constructor(chart: Chart, series: CandleSeries, container: HTMLElement, metrics: MetricsTracker) {
    this.chart = chart;
    this.series = series;
    this.container = container;
    this.metrics = metrics;
  }

  async runScenario(
    name: string,
    durationMs: number,
    action: (elapsedMs: number, dt: number) => void
  ): Promise<BenchmarkScenarioResult> {
    return new Promise((resolve) => {
      this.metrics.start();
      const startTime = performance.now();
      let lastTime = startTime;

      const frame = (now: number) => {
        this.metrics.recordFrame(now);
        const elapsed = now - startTime;
        const dt = now - lastTime;
        lastTime = now;

        action(elapsed, dt);

        if (elapsed < durationMs) {
          requestAnimationFrame(frame);
        } else {
          const result = this.metrics.stop(name);
          resolve(result);
        }
      };

      requestAnimationFrame(frame);
    });
  }

  // 1. Continuous drag pan (10s)
  async runPanScenario(durationMs: number = 10_000): Promise<BenchmarkScenarioResult> {
    const vp = this.chart.getViewport();
    const initialFrom = vp.fromIndex;
    const speed = 15; // bars per second oscillation

    return this.runScenario('continuous_pan', durationMs, (elapsed) => {
      // Oscillate left and right sinusoidally
      const offset = Math.sin(elapsed * 0.003) * 500;
      vp.fromIndex = Math.max(0, initialFrom + offset);
      this.chart.requestRender();
    });
  }

  // 2. Continuous wheel-zoom sweep (full range to 100 bars and back) (10s)
  async runZoomSweepScenario(durationMs: number = 10_000): Promise<BenchmarkScenarioResult> {
    const vp = this.chart.getViewport();
    const total = this.series.count;
    const minBars = 100;
    const maxBars = Math.min(total, 450_000);

    return this.runScenario('zoom_sweep', durationMs, (elapsed) => {
      // Sine wave between 0 (zoomed in to 100 bars) and 1 (zoomed out to 450k bars)
      const t = (Math.sin(elapsed * 0.002) + 1) * 0.5;
      // Exponential interpolation for natural zoom sweep
      const logMin = Math.log(minBars);
      const logMax = Math.log(maxBars);
      const bars = Math.exp(logMin + t * (logMax - logMin));

      vp.barsVisible = bars;
      this.chart.requestRender();
    });
  }

  // 3. Live tick at 100 updates/s on the last bar (10s)
  async runLiveTickScenario(durationMs: number = 10_000): Promise<BenchmarkScenarioResult> {
    const store = this.series.getStore();
    const lastIdx = store.count - 1;
    let baseClose = store.close[lastIdx];
    let tickCount = 0;

    return this.runScenario('live_tick_100hz', durationMs, (elapsed) => {
      // Aim for 100 updates/sec (~1 update every 10ms)
      const expectedTicks = Math.floor(elapsed / 10);
      while (tickCount < expectedTicks) {
        tickCount++;
        const delta = (Math.random() - 0.49) * 0.5;
        baseClose += delta;
        const currentHigh = Math.max(store.high[lastIdx], baseClose);
        const currentLow = Math.min(store.low[lastIdx], baseClose);

        this.series.updateLast({
          time: store.time[lastIdx],
          open: store.open[lastIdx],
          high: currentHigh,
          low: currentLow,
          close: baseClose,
          volume: store.volume[lastIdx] + 1,
        });
      }
    });
  }

  // 4. Append one bar per second (10s)
  async runAppendScenario(durationMs: number = 10_000): Promise<BenchmarkScenarioResult> {
    const store = this.series.getStore();
    let lastTime = store.time[store.count - 1];
    let lastClose = store.close[store.count - 1];
    let appendsDone = 0;

    return this.runScenario('append_bar_1hz', durationMs, (elapsed) => {
      const expectedAppends = Math.floor(elapsed / 1000);
      while (appendsDone < expectedAppends) {
        appendsDone++;
        lastTime += 60_000;
        const o = lastClose;
        const delta = (Math.random() - 0.49) * 2;
        const c = o + delta;
        const h = Math.max(o, c) + Math.random();
        const l = Math.min(o, c) - Math.random();

        this.series.append({
          time: lastTime,
          open: o,
          high: h,
          low: l,
          close: c,
          volume: 1000,
        });
        lastClose = c;
      }
    });
  }

  // 5. Resize drag (10s)
  async runResizeScenario(durationMs: number = 10_000): Promise<BenchmarkScenarioResult> {
    const origWidth = this.container.clientWidth;

    return this.runScenario('resize_drag', durationMs, (elapsed) => {
      // Oscillate container width by +/- 150px
      const offset = Math.sin(elapsed * 0.003) * 150;
      this.container.style.width = `${Math.round(origWidth + offset)}px`;
    }).then((result) => {
      // Restore width
      this.container.style.width = '100%';
      return result;
    });
  }
}
