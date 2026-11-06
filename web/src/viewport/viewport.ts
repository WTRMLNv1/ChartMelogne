import type { ViewportState } from '../types.js';

export interface ViewportOptions {
  minBarsVisible?: number;
  rightPaddingRatio?: number; // up to 0.5 (50% of viewport width)
  yPaddingRatio?: number;     // 0.05 (5%)
}

export class Viewport {
  fromIndex: number = 0;
  barsVisible: number = 100;
  yMin: number = 0;
  yMax: number = 1;
  yMode: 'auto' | 'manual' = 'auto';

  // Target values for smooth y auto-fit easing
  targetYMin: number = 0;
  targetYMax: number = 1;

  widthPx: number = 800;
  heightPx: number = 600;
  dpr: number = 1;
  totalBars: number = 0;

  readonly minBarsVisible: number;
  readonly rightPaddingRatio: number;
  readonly yPaddingRatio: number;

  constructor(options: ViewportOptions = {}) {
    this.minBarsVisible = options.minBarsVisible ?? 10;
    this.rightPaddingRatio = options.rightPaddingRatio ?? 0.5;
    this.yPaddingRatio = options.yPaddingRatio ?? 0.05;
  }

  get pxPerBar(): number {
    return this.barsVisible > 0 ? this.widthPx / this.barsVisible : 1;
  }

  get yScale(): number {
    const span = this.yMax - this.yMin;
    return span > 0 ? this.heightPx / span : 1;
  }

  getState(): ViewportState {
    return {
      fromIndex: this.fromIndex,
      barsVisible: this.barsVisible,
      yMin: this.yMin,
      yMax: this.yMax,
      yMode: this.yMode,
    };
  }

  setSize(widthPx: number, heightPx: number, dpr: number): void {
    this.widthPx = Math.max(1, widthPx);
    this.heightPx = Math.max(1, heightPx);
    this.dpr = dpr;
    this.clamp();
  }

  setTotalBars(total: number): void {
    this.totalBars = Math.max(0, total);
    this.clamp();
  }

  fitContent(): void {
    if (this.totalBars <= 0) {
      this.fromIndex = 0;
      this.barsVisible = 100;
      return;
    }
    // Show all bars with some right padding
    const paddingBars = Math.min(this.totalBars * 0.1, 50);
    this.barsVisible = Math.max(this.minBarsVisible, this.totalBars + paddingBars);
    this.fromIndex = 0;
    this.clamp();
  }

  panByPixels(deltaScreenPx: number): void {
    const deltaBars = -deltaScreenPx / this.pxPerBar;
    this.fromIndex += deltaBars;
    this.clamp();
  }

  zoomAt(cursorScreenPx: number, zoomFactor: number): void {
    const anchorBar = this.screenPxToBarIndex(cursorScreenPx);
    const newBarsVisible = this.barsVisible * zoomFactor;

    this.barsVisible = this.clampBarsVisible(newBarsVisible);
    const newPxPerBar = this.widthPx / this.barsVisible;
    this.fromIndex = anchorBar - cursorScreenPx / newPxPerBar;
    this.clamp();
  }

  setVisibleRange(fromIndex: number, toIndex: number): void {
    const span = Math.max(this.minBarsVisible, toIndex - fromIndex);
    this.barsVisible = span;
    this.fromIndex = fromIndex;
    this.clamp();
  }

  updateAutoY(rawMin: number, rawMax: number, immediate: boolean = false): void {
    if (this.yMode !== 'auto') return;

    let span = rawMax - rawMin;
    if (span <= 0) span = Math.max(1, Math.abs(rawMin) * 0.1);

    const pad = span * this.yPaddingRatio;
    this.targetYMin = rawMin - pad;
    this.targetYMax = rawMax + pad;

    if (immediate || this.yMin === 0 && this.yMax === 1) {
      this.yMin = this.targetYMin;
      this.yMax = this.targetYMax;
    } else {
      // Exponential smoothing step
      const alpha = 0.25;
      this.yMin += (this.targetYMin - this.yMin) * alpha;
      this.yMax += (this.targetYMax - this.yMax) * alpha;
    }
  }

  screenPxToBarIndex(screenPx: number): number {
    return this.fromIndex + screenPx / this.pxPerBar;
  }

  barIndexToScreenPx(barIndex: number): number {
    return (barIndex - this.fromIndex) * this.pxPerBar;
  }

  priceToScreenPx(price: number): number {
    return (price - this.yMin) * this.yScale;
  }

  screenPxToPrice(screenPx: number): number {
    return this.yMin + screenPx / this.yScale;
  }

  isLastBarVisible(): boolean {
    if (this.totalBars <= 0) return true;
    const lastBar = this.totalBars - 1;
    return lastBar >= this.fromIndex && lastBar <= this.fromIndex + this.barsVisible;
  }

  private clampBarsVisible(bars: number): number {
    const maxBars = Math.max(this.minBarsVisible, this.totalBars > 0 ? this.totalBars * 2 : 10000);
    return Math.min(maxBars, Math.max(this.minBarsVisible, bars));
  }

  private clamp(): void {
    this.barsVisible = this.clampBarsVisible(this.barsVisible);

    if (this.totalBars <= 0) {
      return;
    }

    const paddingBars = this.barsVisible * this.rightPaddingRatio;
    const minFromIndex = -this.barsVisible * 0.5;
    const maxFromIndex = this.totalBars - this.barsVisible + paddingBars;

    if (this.fromIndex < minFromIndex) {
      this.fromIndex = minFromIndex;
    } else if (this.fromIndex > maxFromIndex) {
      this.fromIndex = maxFromIndex;
    }
  }
}
