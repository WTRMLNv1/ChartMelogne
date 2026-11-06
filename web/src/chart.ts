import { CandleSeries, type SeriesDataChangeType } from './series/candle-series.js';
import { CandleRenderer } from './render/candle-renderer.js';
import { StaticAxisRenderer, type ChartLayout } from './render/static-axis-renderer.js';
import { OverlayRenderer, type CrosshairState } from './render/overlay-renderer.js';
import { TimeAxis } from './axis/time-axis.js';
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

  // Stacked canvas layers (Section 3.3)
  // 1. Static Layer (Canvas2D) - background, grid, axis labels
  private staticCanvas: HTMLCanvasElement;
  private staticCtx: CanvasRenderingContext2D;
  private staticRenderer: StaticAxisRenderer;

  // 2. Series Layer (WebGL2) - candles
  private seriesCanvas: HTMLCanvasElement;
  private gl: WebGL2RenderingContext;
  private candleRenderer: CandleRenderer;

  // 3. Overlay Layer (Canvas2D) - crosshair, badges, last-price line
  private overlayCanvas: HTMLCanvasElement;
  private overlayCtx: CanvasRenderingContext2D;
  private overlayRenderer: OverlayRenderer;

  private timeAxis: TimeAxis;
  private viewport: Viewport;
  private interactionHandler: InteractionHandler;
  private resizeObserver: ResizeObserver;

  private layout: ChartLayout = {
    widthPx: 800,
    heightPx: 600,
    plotWidthPx: 735,
    plotHeightPx: 572,
    priceAxisWidthPx: 65,
    timeAxisHeightPx: 28,
    dpr: 1,
  };

  private crosshairState: CrosshairState = {
    screenX: -1,
    screenY: -1,
    active: false,
  };

  private candleSeriesList: CandleSeries[] = [];
  private renderScheduled = false;
  private overlayScheduled = false;
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
    this.staticRenderer = new StaticAxisRenderer();

    // 2. Series canvas (WebGL2) - positioned over plot area
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

    // 3. Overlay canvas (Canvas2D) - covers full chart
    this.overlayCanvas = this.createCanvas(3);
    const overlayCtx = this.overlayCanvas.getContext('2d');
    if (!overlayCtx) throw new Error('Failed to create 2D context for overlay layer');
    this.overlayCtx = overlayCtx;
    this.overlayRenderer = new OverlayRenderer();

    // Viewport, TimeAxis and Interaction
    this.viewport = new Viewport();
    this.timeAxis = new TimeAxis({
      utcOffsetMinutes: options.utcOffsetMinutes ?? 330,
    });
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
          // Note: screenY from InteractionHandler was bottom-up; convert to top-down for Canvas2D
          const topDownY = this.layout.heightPx - screenY;
          this.crosshairState.screenX = screenX;
          this.crosshairState.screenY = topDownY;
          this.crosshairState.active = true;

          const index = Math.round(this.viewport.screenPxToBarIndex(screenX));
          this.emit('crosshair', { index, screenX, screenY: topDownY });
          this.requestOverlayRender();
        },
        onPointerLeave: () => {
          this.crosshairState.active = false;
          this.requestOverlayRender();
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
    const widthPx = Math.round(cssW * this.dpr);
    const heightPx = Math.round(cssH * this.dpr);

    const priceAxisWidthPx = Math.round(65 * this.dpr);
    const timeAxisHeightPx = Math.round(28 * this.dpr);
    const plotWidthPx = Math.max(1, widthPx - priceAxisWidthPx);
    const plotHeightPx = Math.max(1, heightPx - timeAxisHeightPx);

    this.layout = {
      widthPx,
      heightPx,
      plotWidthPx,
      plotHeightPx,
      priceAxisWidthPx,
      timeAxisHeightPx,
      dpr: this.dpr,
    };

    // Static and Overlay canvases cover full container
    this.staticCanvas.width = widthPx;
    this.staticCanvas.height = heightPx;
    this.overlayCanvas.width = widthPx;
    this.overlayCanvas.height = heightPx;

    // Series canvas is styled to cover the plot area
    this.seriesCanvas.style.width = `${cssW - 65}px`;
    this.seriesCanvas.style.height = `${cssH - 28}px`;
    this.seriesCanvas.width = plotWidthPx;
    this.seriesCanvas.height = plotHeightPx;

    this.viewport.setSize(plotWidthPx, plotHeightPx, this.dpr);
    this.gl.viewport(0, 0, plotWidthPx, plotHeightPx);

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

  getLayout(): ChartLayout {
    return this.layout;
  }

  getTimeAxis(): TimeAxis {
    return this.timeAxis;
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
        this.renderAll();
      });
    }
  }

  requestOverlayRender(): void {
    if (!this.overlayScheduled && !this.renderScheduled) {
      this.overlayScheduled = true;
      requestAnimationFrame(() => {
        this.overlayScheduled = false;
        this.renderOverlay();
      });
    }
  }

  renderSync(waitForGpu: boolean = false): void {
    this.renderAll();
    if (waitForGpu) {
      this.candleRenderer.finish();
    }
  }

  private renderAll(): void {
    if (this.candleSeriesList.length === 0) return;
    const primarySeries = this.candleSeriesList[0];
    const store = primarySeries.getStore();
    const count = store.count;
    if (count === 0) return;

    const vp = this.viewport;

    // Auto-fit Y from visible range
    const { min, max } = store.getVisibleMinMax(vp.fromIndex, vp.fromIndex + vp.barsVisible);
    vp.updateAutoY(min, max);

    // 1. Static layer (Canvas2D) - background, grid, axes
    this.staticRenderer.render(this.staticCtx, {
      layout: this.layout,
      fromIndex: vp.fromIndex,
      barsVisible: vp.barsVisible,
      yMin: vp.yMin,
      yMax: vp.yMax,
      timeArray: store.time,
      barsCount: count,
      dayStarts: store.dayStarts,
      dayCount: store.dayCount,
      timeAxis: this.timeAxis,
      theme: this.theme,
      priceOptions: {
        priceDecimals: this.options.priceDecimals,
      },
    });

    // 2. Series layer (WebGL2) - candles
    const gl = this.gl;
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);

    this.candleRenderer.render({
      fromIndex: vp.fromIndex,
      barsVisible: vp.barsVisible,
      yMin: vp.yMin,
      yMax: vp.yMax,
      basePrice: store.basePrice,
      totalBars: count,
      viewportWidthPx: this.layout.plotWidthPx,
      viewportHeightPx: this.layout.plotHeightPx,
      dpr: this.dpr,
      theme: this.theme,
    });

    // 3. Overlay layer (Canvas2D) - crosshair, badges, last-price line
    this.renderOverlay();
  }

  private renderOverlay(): void {
    if (this.candleSeriesList.length === 0) return;
    const store = this.candleSeriesList[0].getStore();
    const vp = this.viewport;

    this.overlayRenderer.render(this.overlayCtx, {
      layout: this.layout,
      crosshair: this.crosshairState,
      store,
      timeAxis: this.timeAxis,
      fromIndex: vp.fromIndex,
      barsVisible: vp.barsVisible,
      yMin: vp.yMin,
      yMax: vp.yMax,
      priceDecimals: this.options.priceDecimals ?? 2,
      theme: this.theme,
    });
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

    const totalEstimatedBytes = storeColumnsBytes + cpuInstanceBufferBytes + gpuVboBytes;

    return {
      storeColumnsBytes,
      cpuInstanceBufferBytes,
      gpuVboBytes,
      jsHeapBytes,
      totalEstimatedBytes,
    };
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
