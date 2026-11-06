import { generatePriceTicks, type PriceTick, type PriceTickOptions } from '../axis/price-axis.js';
import { TimeAxis, type TimeTick } from '../axis/time-axis.js';
import type { Theme } from '../types.js';

export interface ChartLayout {
  widthPx: number;
  heightPx: number;
  plotWidthPx: number;
  plotHeightPx: number;
  priceAxisWidthPx: number;
  timeAxisHeightPx: number;
  dpr: number;
}

export interface StaticRenderParams {
  layout: ChartLayout;
  fromIndex: number;
  barsVisible: number;
  yMin: number;
  yMax: number;
  timeArray: Float64Array;
  barsCount: number;
  dayStarts?: Uint32Array;
  dayCount?: number;
  timeAxis: TimeAxis;
  theme: Theme;
  priceOptions?: PriceTickOptions;
}

export class StaticAxisRenderer {
  render(ctx: CanvasRenderingContext2D, params: StaticRenderParams): void {
    const {
      layout,
      fromIndex,
      barsVisible,
      yMin,
      yMax,
      timeArray,
      barsCount,
      dayStarts,
      dayCount,
      timeAxis,
      theme,
      priceOptions,
    } = params;

    const { widthPx, heightPx, plotWidthPx, plotHeightPx, priceAxisWidthPx, dpr } = layout;

    // 1. Clear & Background
    ctx.fillStyle = theme.background;
    ctx.fillRect(0, 0, widthPx, heightPx);

    if (plotWidthPx <= 0 || plotHeightPx <= 0) return;

    // Font setup
    const fontSize = Math.round((theme.fontSize || 11) * dpr);
    ctx.font = `${fontSize}px ${theme.fontFamily}`;
    ctx.textBaseline = 'middle';

    const gridColor = theme.grid;
    const textColor = theme.axisText;
    const borderColor = theme.grid;

    // 2. Horizontal Price Grid & Price Axis Labels
    const priceTicks = generatePriceTicks(yMin, yMax, plotHeightPx, priceOptions);
    const ySpan = yMax - yMin;
    const yScale = ySpan > 0 ? plotHeightPx / ySpan : 1;

    ctx.lineWidth = 1;
    ctx.strokeStyle = gridColor;

    for (const tick of priceTicks) {
      // In Canvas2D top-down coordinates:
      const y = Math.floor(plotHeightPx - (tick.price - yMin) * yScale) + 0.5;
      if (y < 0 || y > plotHeightPx) continue;

      // Grid line
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(plotWidthPx, y);
      ctx.stroke();

      // Label on price axis
      ctx.fillStyle = textColor;
      ctx.textAlign = 'left';
      ctx.fillText(tick.label, plotWidthPx + 6 * dpr, y);
    }

    // 3. Vertical Time Grid & Time Axis Labels
    if (barsCount > 0 && barsVisible > 0) {
      const pxPerBar = plotWidthPx / barsVisible;
      const timeTicks = timeAxis.generateTicks(timeArray, barsCount, fromIndex, barsVisible, plotWidthPx, dayStarts, dayCount);

      for (const tick of timeTicks) {
        const x = Math.floor((tick.barIndex + 0.5 - fromIndex) * pxPerBar) + 0.5;
        if (x < 0 || x > plotWidthPx) continue;

        // Grid line
        ctx.strokeStyle = tick.isMajor ? gridColor : gridColor;
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, plotHeightPx);
        ctx.stroke();

        // Label on time axis
        ctx.fillStyle = textColor;
        ctx.textAlign = 'center';
        ctx.fillText(tick.label, x, plotHeightPx + 14 * dpr);
      }
    }

    // 4. Axis Borders
    ctx.strokeStyle = borderColor;
    ctx.beginPath();
    // Vertical axis separator
    ctx.moveTo(plotWidthPx + 0.5, 0);
    ctx.lineTo(plotWidthPx + 0.5, heightPx);
    // Horizontal axis separator
    ctx.moveTo(0, plotHeightPx + 0.5);
    ctx.lineTo(widthPx, plotHeightPx + 0.5);
    ctx.stroke();
  }
}
