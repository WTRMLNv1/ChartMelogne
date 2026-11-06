export interface BenchmarkScenarioResult {
  scenarioName: string;
  durationMs: number;
  totalFrames: number;
  fps: number;
  p50Ms: number;
  p95Ms: number;
  p99Ms: number;
  maxMs: number;
  avgMs: number;
  droppedFrames: number; // frames taking > 20ms
  jsHeapMb?: number;
  customMetrics?: Record<string, any>;
}

export interface MemoryBreakdown {
  storeColumnsMb: number;
  cpuPackedBufferMb: number;
  gpuVboMb: number;
  totalMemoryMb: number;
  jsHeapMb?: number;
}

export interface LatencyHistogram {
  count: number;
  p50Ms: number;
  p95Ms: number;
  p99Ms: number;
  maxMs: number;
  avgMs: number;
}

export interface BenchmarkReport {
  timestamp: string;
  gitCommit?: string;
  userAgent: string;
  platform: string;
  dpr: number;
  windowSize: { width: number; height: number };
  gpuRenderer?: string;
  barsCount: number;
  setDataToPaintMs: number;
  setDataToFinishMs: number;
  liveTickToPainted: LatencyHistogram;
  appendLatency: LatencyHistogram;
  memory: MemoryBreakdown;
  gateVerification: Record<string, string>;
  scenarios: Record<string, BenchmarkScenarioResult>;
}

export function computeHistogram(samples: number[]): LatencyHistogram {
  if (samples.length === 0) {
    return { count: 0, p50Ms: 0, p95Ms: 0, p99Ms: 0, maxMs: 0, avgMs: 0 };
  }
  const sorted = [...samples].sort((a, b) => a - b);
  const count = sorted.length;
  const sum = sorted.reduce((a, b) => a + b, 0);

  const percentile = (p: number) => {
    const idx = Math.min(count - 1, Math.floor((p / 100) * count));
    return Number(sorted[idx].toFixed(3));
  };

  return {
    count,
    p50Ms: percentile(50),
    p95Ms: percentile(95),
    p99Ms: percentile(99),
    maxMs: Number(sorted[count - 1].toFixed(3)),
    avgMs: Number((sum / count).toFixed(3)),
  };
}

export class MetricsTracker {
  private frameTimes: number[] = [];
  private lastTimestamp = 0;
  private isTracking = false;

  start(): void {
    this.frameTimes = [];
    this.lastTimestamp = performance.now();
    this.isTracking = true;
  }

  recordFrame(now: number): void {
    if (!this.isTracking) return;
    if (this.lastTimestamp > 0) {
      const dt = now - this.lastTimestamp;
      this.frameTimes.push(dt);
    }
    this.lastTimestamp = now;
  }

  stop(scenarioName: string, customMetrics?: Record<string, any>): BenchmarkScenarioResult {
    this.isTracking = false;
    const sorted = [...this.frameTimes].sort((a, b) => a - b);
    const count = sorted.length;
    const totalDuration = this.frameTimes.reduce((acc, v) => acc + v, 0);

    const percentile = (p: number) => {
      if (count === 0) return 0;
      const idx = Math.min(count - 1, Math.floor((p / 100) * count));
      return Number(sorted[idx].toFixed(2));
    };

    const dropped = this.frameTimes.filter((t) => t > 20.0).length;
    const avg = count > 0 ? totalDuration / count : 0;
    const fps = totalDuration > 0 ? count / (totalDuration / 1000) : 0;

    let jsHeapMb: number | undefined;
    if ((performance as any).memory?.usedJSHeapSize) {
      jsHeapMb = Number(((performance as any).memory.usedJSHeapSize / (1024 * 1024)).toFixed(2));
    }

    return {
      scenarioName,
      durationMs: Number(totalDuration.toFixed(2)),
      totalFrames: count,
      fps: Number(fps.toFixed(1)),
      p50Ms: percentile(50),
      p95Ms: percentile(95),
      p99Ms: percentile(99),
      maxMs: count > 0 ? Number(sorted[count - 1].toFixed(2)) : 0,
      avgMs: Number(avg.toFixed(2)),
      droppedFrames: dropped,
      jsHeapMb,
      customMetrics,
    };
  }
}

export function getWebGLInfo(gl?: WebGL2RenderingContext | null): string {
  try {
    const context = gl || document.createElement('canvas').getContext('webgl2');
    if (!context) return 'Unknown / WebGL not available';
    const debugInfo = context.getExtension('WEBGL_debug_renderer_info');
    if (debugInfo) {
      const renderer = context.getParameter(debugInfo.UNMASKED_RENDERER_WEBGL);
      const vendor = context.getParameter(debugInfo.UNMASKED_VENDOR_WEBGL);
      return `${vendor} - ${renderer}`;
    }
    return context.getParameter(context.RENDERER) || 'WebGL generic';
  } catch {
    return 'Unknown';
  }
}
