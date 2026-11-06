import { CandleSeries, type SeriesDataChangeType } from './series/candle-series.js';
import { CandleRenderer } from './render/candle-renderer.js';
import { Viewport } from './viewport/viewport.js';
import { InteractionHandler } from './input/interaction-handler.js';
import { darkTheme } from './themes.js';
import type { ChartOptions, CandleSeriesOptions, Theme, ViewportState } from './types.js';

export interface ChartEventMap {
  visibleRangeChange: (state: ViewportState) => void;
  crosshair: (info: { index: number; screenX: number; screenY: number }) => void;
}

export class Chart {
  private container: HTMLElement;
  private options: ChartOptions;
  private theme: Theme;

  // Stacked canvas layers
  private staticCanvas: HTMLCanvasElement;
  private staticCtx: CanvasRenderingContext2D;

  private seriesCanvas: HTMLCanvasElement;
  private gl: WebGL2RenderingContext;
  private candleRenderer: CandleRenderer;

  private overlayCanvas: HTMLCanvasElement;
  private overlayCtx: CanvasRenderingContext2D;

  private viewport: Viewport;
  private interactionHandler: InteractionHandler;
  private resizeObserver: ResizeObserver;

  private candleSeriesList: CandleSeries[] = [];
  private renderScheduled = false;
  private eventListeners: { [K in keyof ChartEventMap]?: Array<ChartEventMap[K]> } = {};

  private dpr: number = 1;

  constructor(container: HTMLElement, options: ChartOptions = {}) {
    this.container = container;
    this.options = options;
    this.theme = options.theme ?? darkTheme;

    // Apply container styling
    this.container.style.position = 'relative';
    this.container.style.overflow = 'hidden';
    this.container.style.userSelect = 'none';

    // 1. Static canvas (Canvas2D)
    this.staticCanvas = this.createCanvas(1);
    const staticCtx = this.staticCanvas.getContext('2d');
    if (!staticCtx) throw new Error('Failed to create 2D context for static layer');
    this.staticCtx = staticCtx;

    // 2. Series canvas (WebGL2)
    this.seriesCanvas = this.createCanvas(2);
    const gl = this.seriesCanvas.getContext('webgl2', {
      alpha: true,
      antialias: false,
      powerPreference: 'high-performance',
      preserveDrawingBuffer: false,
    });
    if (!gl) throw new Error('WebGL2 is not supported on this device/browser');
    this.gl = gl;
    this.candleRenderer = new CandleRenderer(this.gl);

    // 3. Overlay canvas (Canvas2D)
    this.overlayCanvas = this.createCanvas(3);
    const overlayCtx = this.overlayCanvas.getContext('2d');
    if (!overlayCtx) throw new Error('Failed to create 2D context for overlay layer');
    this.overlayCtx = overlayCtx;

    // Viewport and Interaction
    this.viewport = new Viewport();
    this.updateDpr();

    this.interactionHandler = new InteractionHandler(
      this.overlayCanvas,
      {
        onPan: (deltaScreenPx) => {
          this.viewport.panByPixels(deltaScreenPx);
          this.emit('visibleRangeChange', this.viewport.getState());
          this.requestRender();
        },
        onZoom: (cursorScreenPx, zoomFactor) => {
          this.viewport.zoomAt(cursorScreenPx, zoomFactor);
          this.emit('visibleRangeChange', this.viewport.getState());
          this.requestRender();
        },
        onPointerMove: (screenX, screenY) => {
          const index = Math.round(this.viewport.screenPxToBarIndex(screenX));
          this.emit('crosshair', { index, screenX, screenY });
        },
      },
      () => this.dpr
    );

    // ResizeObserver
    this.resizeObserver = new ResizeObserver(() => this.resize());
    this.resizeObserver.observe(this.container);

    this.resize();
  }

  private createCanvas(zIndex: number): HTMLCanvasElement {
    const canvas = document.createElement('canvas');
    canvas.style.position = 'absolute';
    canvas.style.top = '0';
    canvas.style.left = '0';
    canvas.style.width = '100%';
    canvas.style.height = '100%';
    canvas.style.zIndex = zIndex.toString();
    this.container.appendChild(canvas);
    return canvas;
  }

  private updateDpr(): void {
    this.dpr = this.options.dpr ?? window.devicePixelRatio ?? 1;
  }

  private resize(): void {
    const rect = this.container.getBoundingClientRect();
    const cssW = Math.max(1, Math.floor(rect.width));
    const cssH = Math.max(1, Math.floor(rect.height));

    this.updateDpr();
    const pixelW = Math.round(cssW * this.dpr);
    const pixelH = Math.round(cssH * this.dpr);

    this.staticCanvas.width = pixelW;
    this.staticCanvas.height = pixelH;
    this.seriesCanvas.width = pixelW;
    this.seriesCanvas.height = pixelH;
    this.overlayCanvas.width = pixelW;
    this.overlayCanvas.height = pixelH;

    this.viewport.setSize(pixelW, pixelH, this.dpr);
    this.gl.viewport(0, 0, pixelW, pixelH);

    this.renderStatic();
    this.requestRender();
  }

  addCandles(options: CandleSeriesOptions = {}): CandleSeries {
    const series = new CandleSeries(options, (type: SeriesDataChangeType, index: number) => {
      this.onSeriesDataChanged(series, type, index);
    });
    this.candleSeriesList.push(series);
    return series;
  }

  private onSeriesDataChanged(series: CandleSeries, type: SeriesDataChangeType, index: number): void {
    const store = series.getStore();
    this.viewport.setTotalBars(store.count);

    if (type === 'full') {
      this.candleRenderer.uploadData(store.gpuBuffer, store.count);
      this.fitContent();
    } else if (type === 'append') {
      const wasFollowing = this.viewport.isLastBarVisible();
      if (wasFollowing) {
        // Follow-live behavior (Section 6.3)
        this.viewport.fromIndex += 1;
      }
      this.candleRenderer.uploadData(store.gpuBuffer, store.count);
    } else if (type === 'updateLast') {
      this.candleRenderer.updateSubData(store.gpuBuffer, index, 1);
    }

    this.requestRender();
  }

  setVisibleRange(fromIndex: number, toIndex: number): void {
    this.viewport.setVisibleRange(fromIndex, toIndex);
    this.emit('visibleRangeChange', this.viewport.getState());
    this.requestRender();
  }

  fitContent(): void {
    this.viewport.fitContent();
    this.emit('visibleRangeChange', this.viewport.getState());
    this.requestRender();
  }

  getViewport(): Viewport {
    return this.viewport;
  }

  on<K extends keyof ChartEventMap>(event: K, handler: ChartEventMap[K]): void {
    if (!this.eventListeners[event]) {
      this.eventListeners[event] = [];
    }
    this.eventListeners[event]?.push(handler);
  }

  off<K extends keyof ChartEventMap>(event: K, handler: ChartEventMap[K]): void {
    const list = this.eventListeners[event];
    if (list) {
      this.eventListeners[event] = list.filter((h) => h !== handler) as any;
    }
  }

  private emit<K extends keyof ChartEventMap>(event: K, ...args: Parameters<ChartEventMap[K]>): void {
    const list = this.eventListeners[event];
    if (list) {
      for (const fn of list) {
        (fn as any)(...args);
      }
    }
  }

  requestRender(): void {
    if (!this.renderScheduled) {
      this.renderScheduled = true;
      requestAnimationFrame(() => {
        this.renderScheduled = false;
        this.render();
      });
    }
  }

  renderStatic(): void {
    const ctx = this.staticCtx;
    ctx.fillStyle = this.theme.background;
    ctx.fillRect(0, 0, this.staticCanvas.width, this.staticCanvas.height);
  }

  renderSync(waitForGpu: boolean = false): void {
    this.render();
    if (waitForGpu) {
      this.candleRenderer.finish();
    }
  }

  getMemoryBreakdown(): {
    storeColumnsBytes: number;
    cpuInstanceBufferBytes: number;
    gpuVboBytes: number;
    jsHeapBytes?: number;
    totalEstimatedBytes: number;
  } {
    let storeColumnsBytes = 0;
    let cpuInstanceBufferBytes = 0;
    for (const s of this.candleSeriesList) {
      const mem = s.getStore().getMemoryUsage();
      storeColumnsBytes += mem.storeBytes;
      cpuInstanceBufferBytes += mem.cpuInstanceBufferBytes;
    }
    const gpuMem = this.candleRenderer.getGpuMemoryUsage();
    const gpuVboBytes = gpuMem.totalGpuBytes;

    let jsHeapBytes: number | undefined;
    if (typeof performance !== 'undefined' && (performance as any).memory?.usedJSHeapSize) {
      jsHeapBytes = (performance as any).memory.usedJSHeapSize;
    }

    // Total = typed array stores + CPU instance buffer + GPU allocated VBO buffers
    const totalEstimatedBytes = storeColumnsBytes + cpuInstanceBufferBytes + gpuVboBytes;

    return {
      storeColumnsBytes,
      cpuInstanceBufferBytes,
      gpuVboBytes,
      jsHeapBytes,
      totalEstimatedBytes,
    };
  }

  private render(): void {
    const gl = this.gl;
    const vp = this.viewport;

    // Clear WebGL buffer
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);

    if (this.candleSeriesList.length === 0) return;

    // In Phase 1 we take the primary series
    const primarySeries = this.candleSeriesList[0];
    const store = primarySeries.getStore();
    const count = store.count;
    if (count === 0) return;

    // Auto-fit Y from visible range
    const { min, max } = store.getVisibleMinMax(vp.fromIndex, vp.fromIndex + vp.barsVisible);
    vp.updateAutoY(min, max);

    // Draw candles
    this.candleRenderer.render({
      fromIndex: vp.fromIndex,
      barsVisible: vp.barsVisible,
      yMin: vp.yMin,
      yMax: vp.yMax,
      basePrice: store.basePrice,
      totalBars: count,
      viewportWidthPx: vp.widthPx,
      viewportHeightPx: vp.heightPx,
      dpr: this.dpr,
      theme: this.theme,
    });
  }

  destroy(): void {
    this.resizeObserver.disconnect();
    this.interactionHandler.detach();
    this.candleRenderer.destroy();
    this.container.innerHTML = '';
  }
}

export function createChart(container: HTMLElement, options?: ChartOptions): Chart {
  return new Chart(container, options);
}
