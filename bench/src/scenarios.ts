import type { Chart, CandleSeries } from 'chartmln';
import { MetricsTracker, computeHistogram, type BenchmarkScenarioResult, type LatencyHistogram } from './metrics.js';

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
    action: (elapsedMs: number, dt: number) => void,
    customMetrics?: Record<string, any>
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
          const result = this.metrics.stop(name, customMetrics);
          resolve(result);
        }
      };

      requestAnimationFrame(frame);
    });
  }

  // Measure exact tick-to-painted latency with gl.finish() over N samples
  measureLiveTickToPainted(samples: number = 500): LatencyHistogram {
    const store = this.series.getStore();
    const lastIdx = store.count - 1;
    let baseClose = store.getClose(lastIdx);
    const latencies: number[] = [];

    // Warm up
    for (let i = 0; i < 20; i++) {
      baseClose += 0.05;
      this.series.updateLast({
        time: store.time[lastIdx],
        open: store.getOpen(lastIdx),
        high: Math.max(store.getHigh(lastIdx), baseClose),
        low: Math.min(store.getLow(lastIdx), baseClose),
        close: baseClose,
        volume: store.volume[lastIdx] + 1,
      });
      this.chart.renderSync(true);
    }

    for (let i = 0; i < samples; i++) {
      const delta = (Math.random() - 0.49) * 0.5;
      baseClose += delta;
      const h = Math.max(store.getHigh(lastIdx), baseClose);
      const l = Math.min(store.getLow(lastIdx), baseClose);

      const t0 = performance.now();
      this.series.updateLast({
        time: store.time[lastIdx],
        open: store.getOpen(lastIdx),
        high: h,
        low: l,
        close: baseClose,
        volume: store.volume[lastIdx] + 1,
      });
      this.chart.renderSync(true); // forces GPU draw and gl.finish()
      const tDone = performance.now();
      latencies.push(tDone - t0);
    }

    return computeHistogram(latencies);
  }

  // Measure exact append-to-painted latency with gl.finish() over N samples
  measureAppendToPainted(samples: number = 200): LatencyHistogram {
    const store = this.series.getStore();
    let lastTime = store.time[store.count - 1];
    let lastClose = store.getClose(store.count - 1);
    const latencies: number[] = [];

    for (let i = 0; i < samples; i++) {
      lastTime += 60_000;
      const o = lastClose;
      const delta = (Math.random() - 0.49) * 2;
      const c = o + delta;
      const h = Math.max(o, c) + Math.random();
      const l = Math.min(o, c) - Math.random();

      const t0 = performance.now();
      this.series.append({
        time: lastTime,
        open: o,
        high: h,
        low: l,
        close: c,
        volume: 1000,
      });
      this.chart.renderSync(true); // forces GPU draw and gl.finish()
      const tDone = performance.now();
      latencies.push(tDone - t0);
      lastClose = c;
    }

    return computeHistogram(latencies);
  }

  // 1. Continuous drag pan (10s)
  async runPanScenario(durationMs: number = 10_000): Promise<BenchmarkScenarioResult> {
    const vp = this.chart.getViewport();
    const initialFrom = vp.fromIndex;

    return this.runScenario('continuous_pan', durationMs, (elapsed) => {
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
      const t = (Math.sin(elapsed * 0.002) + 1) * 0.5;
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
    let baseClose = store.getClose(lastIdx);
    let tickCount = 0;

    const latencyHist = this.measureLiveTickToPainted(200);

    return this.runScenario(
      'live_tick_100hz',
      durationMs,
      (elapsed) => {
        const expectedTicks = Math.floor(elapsed / 10);
        while (tickCount < expectedTicks) {
          tickCount++;
          const delta = (Math.random() - 0.49) * 0.5;
          baseClose += delta;
          const currentHigh = Math.max(store.getHigh(lastIdx), baseClose);
          const currentLow = Math.min(store.getLow(lastIdx), baseClose);

          this.series.updateLast({
            time: store.time[lastIdx],
            open: store.getOpen(lastIdx),
            high: currentHigh,
            low: currentLow,
            close: baseClose,
            volume: store.volume[lastIdx] + 1,
          });
        }
      },
      { tickToPaintedLatency: latencyHist }
    );
  }

  // 4. Append one bar per second (10s)
  async runAppendScenario(durationMs: number = 10_000): Promise<BenchmarkScenarioResult> {
    const store = this.series.getStore();
    let lastTime = store.time[store.count - 1];
    let lastClose = store.getClose(store.count - 1);
    let appendsDone = 0;

    const appendHist = this.measureAppendToPainted(100);

    return this.runScenario(
      'append_bar_1hz',
      durationMs,
      (elapsed) => {
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
      },
      { appendToPaintedLatency: appendHist }
    );
  }

  // 5. Resize drag (10s)
  async runResizeScenario(durationMs: number = 10_000): Promise<BenchmarkScenarioResult> {
    const origWidth = this.container.clientWidth;

    return this.runScenario('resize_drag', durationMs, (elapsed) => {
      const offset = Math.sin(elapsed * 0.003) * 150;
      this.container.style.width = `${Math.round(origWidth + offset)}px`;
    }).then((result) => {
      this.container.style.width = '100%';
      return result;
    });
  }
}
