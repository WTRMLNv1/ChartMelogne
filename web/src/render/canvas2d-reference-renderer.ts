import type { CandleData, Theme } from '../types.js';

export interface Canvas2dRenderParams {
  data: CandleData;
  count: number;
  fromIndex: number;
  barsVisible: number;
  yMin: number;
  yMax: number;
  viewportWidthPx: number;
  viewportHeightPx: number;
  dpr: number;
  theme: Theme;
  barsPerNode?: number;
}

/**
 * Canvas2D reference renderer for golden image checks and shader math verification (Section 12.3).
 */
export class Canvas2dReferenceRenderer {
  render(ctx: CanvasRenderingContext2D, params: Canvas2dRenderParams): void {
    const {
      data,
      count,
      fromIndex,
      barsVisible,
      yMin,
      yMax,
      viewportWidthPx,
      viewportHeightPx,
      dpr,
      theme,
      barsPerNode = 1.0,
    } = params;

    if (count <= 0 || viewportWidthPx <= 0 || viewportHeightPx <= 0) return;

    const pxPerBar = viewportWidthPx / barsVisible;
    const ySpan = yMax - yMin;
    const yScale = ySpan > 0 ? viewportHeightPx / ySpan : 1.0;

    const firstVisible = Math.max(0, Math.floor(fromIndex - 2));
    const lastVisible = Math.min(count - 1, Math.ceil(fromIndex + barsVisible + 2));

    const upColor = theme.upColorHex ?? '#089981';
    const downColor = theme.downColorHex ?? '#f23645';

    for (let i = firstVisible; i <= lastVisible; i++) {
      if (data.flags && (data.flags[i] & 1) !== 0) continue;

      const o = data.open[i];
      const h = data.high[i];
      const l = data.low[i];
      const c = data.close[i];

      const isUp = c >= o;
      ctx.fillStyle = isUp ? upColor : downColor;

      // Candle horizontal center snapped to pixel center
      let centerX = (i + 0.5 * barsPerNode - fromIndex) * pxPerBar;
      centerX = Math.floor(centerX) + 0.5;

      let bodyWidth = Math.min(30.0 * dpr, Math.max(1.0, pxPerBar * 0.7));
      let wickWidth = Math.max(1.0, Math.floor(Math.max(1.0, 0.1 * bodyWidth) + 0.5));

      if (pxPerBar < 3.0 * dpr) {
        bodyWidth = 0.0;
        wickWidth = 1.0;
      }

      // In Canvas2D, y=0 is top, y=height is bottom (inverted from WebGL bottom-up)
      // WebGL y: (price - yMin) * yScale
      // Canvas2D y: viewportHeightPx - (price - yMin) * yScale
      const toCanvasY = (price: number) => viewportHeightPx - (price - yMin) * yScale;

      const yHigh = toCanvasY(h);
      const yLow = toCanvasY(l);

      // Draw Wick
      ctx.fillRect(centerX - wickWidth * 0.5, yHigh, wickWidth, Math.max(1, yLow - yHigh));

      // Draw Body
      if (bodyWidth > 0) {
        const bodyTop = toCanvasY(Math.max(o, c));
        const bodyBottom = toCanvasY(Math.min(o, c));
        let bodyHeight = bodyBottom - bodyTop;
        if (bodyHeight < 1.0) {
          bodyHeight = 1.0;
        }
        ctx.fillRect(centerX - bodyWidth * 0.5, bodyTop, bodyWidth, bodyHeight);
      }
    }
  }
}
