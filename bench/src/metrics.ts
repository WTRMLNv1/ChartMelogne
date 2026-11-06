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
  scenarios: Record<string, BenchmarkScenarioResult>;
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

  stop(scenarioName: string): BenchmarkScenarioResult {
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
    const fps = totalDuration > 0 ? (count / (totalDuration / 1000)) : 0;

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
    };
  }

  getCurrentFPS(): { fps: number; frameTime: number } {
    if (this.frameTimes.length === 0) return { fps: 0, frameTime: 0 };
    const recent = this.frameTimes.slice(-30);
    const sum = recent.reduce((a, b) => a + b, 0);
    const avg = sum / recent.length;
    return {
      fps: avg > 0 ? Math.round(1000 / avg) : 0,
      frameTime: Number(avg.toFixed(2)),
    };
  }
}

export function getWebGLInfo(): string {
  try {
    const canvas = document.createElement('canvas');
    const gl = canvas.getContext('webgl2') || canvas.getContext('webgl');
    if (!gl) return 'Unknown / WebGL not available';
    const debugInfo = gl.getExtension('WEBGL_debug_renderer_info');
    if (debugInfo) {
      const renderer = gl.getParameter(debugInfo.UNMASKED_RENDERER_WEBGL);
      const vendor = gl.getParameter(debugInfo.UNMASKED_VENDOR_WEBGL);
      return `${vendor} - ${renderer}`;
    }
    return gl.getParameter(gl.RENDERER) || 'WebGL generic';
  } catch {
    return 'Unknown';
  }
}
