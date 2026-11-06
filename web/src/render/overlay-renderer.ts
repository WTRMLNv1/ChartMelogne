import type { ChartLayout } from './static-axis-renderer.js';
import type { TimeAxis } from '../axis/time-axis.js';
import type { CandleStore } from '../data/candle-store.js';
import type { Theme } from '../types.js';

export interface CrosshairState {
  screenX: number; // in device pixels
  screenY: number; // in device pixels (top-down)
  active: boolean;
}

export interface OverlayRenderParams {
  layout: ChartLayout;
  crosshair: CrosshairState;
  store: CandleStore;
  timeAxis: TimeAxis;
  fromIndex: number;
  barsVisible: number;
  yMin: number;
  yMax: number;
  priceDecimals?: number;
  theme: Theme;
}

export class OverlayRenderer {
  render(ctx: CanvasRenderingContext2D, params: OverlayRenderParams): void {
    const {
      layout,
      crosshair,
      store,
      timeAxis,
      fromIndex,
      barsVisible,
      yMin,
      yMax,
      priceDecimals = 2,
      theme,
    } = params;

    const { widthPx, heightPx, plotWidthPx, plotHeightPx, priceAxisWidthPx, dpr } = layout;

    // Clear overlay
    ctx.clearRect(0, 0, widthPx, heightPx);

    if (store.count === 0 || plotWidthPx <= 0 || plotHeightPx <= 0) return;

    const pxPerBar = plotWidthPx / barsVisible;
    const ySpan = yMax - yMin;
    const yScale = ySpan > 0 ? plotHeightPx / ySpan : 1;

    const toScreenY = (price: number) => plotHeightPx - (price - yMin) * yScale;
    const toPrice = (screenY: number) => yMin + (plotHeightPx - screenY) / yScale;

    const lastIdx = store.count - 1;
    const lastClose = store.close[lastIdx];
    const lastOpen = store.open[lastIdx];
    const isLastUp = lastClose >= lastOpen;
    const lastColor = isLastUp ? (theme.upColorHex ?? '#089981') : (theme.downColorHex ?? '#f23645');

    // 1. Last Price Line and Badge
    const lastY = Math.floor(toScreenY(lastClose)) + 0.5;
    if (lastY >= 0 && lastY <= plotHeightPx) {
      // Dashed line across plot area
      ctx.save();
      ctx.setLineDash([4 * dpr, 4 * dpr]);
      ctx.strokeStyle = lastColor;
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(0, lastY);
      ctx.lineTo(plotWidthPx, lastY);
      ctx.stroke();
      ctx.restore();

      // Last price badge on price axis
      const badgeH = Math.round(18 * dpr);
      const badgeW = Math.round(56 * dpr);
      const badgeX = plotWidthPx + 2 * dpr;
      const badgeY = Math.round(lastY - badgeH * 0.5);

      ctx.fillStyle = lastColor;
      this.drawRoundedRect(ctx, badgeX, badgeY, badgeW, badgeH, 3 * dpr);
      ctx.fill();

      ctx.fillStyle = '#ffffff';
      ctx.font = `bold ${Math.round(10 * dpr)}px ${theme.fontFamily}`;
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText(lastClose.toFixed(priceDecimals), badgeX + badgeW * 0.5, lastY);
    }

    // 2. Crosshair and Hover Badges
    if (crosshair.active && crosshair.screenX >= 0 && crosshair.screenX <= plotWidthPx && crosshair.screenY >= 0 && crosshair.screenY <= plotHeightPx) {
      const cursorX = crosshair.screenX;
      const cursorY = crosshair.screenY;

      // Find hovered bar index
      const hoveredBarIndex = Math.min(
        store.count - 1,
        Math.max(0, Math.round(fromIndex + cursorX / pxPerBar - 0.5))
      );
      const snappedCenterX = Math.floor((hoveredBarIndex + 0.5 - fromIndex) * pxPerBar) + 0.5;

      // Crosshair lines
      ctx.save();
      ctx.setLineDash([3 * dpr, 3 * dpr]);
      ctx.strokeStyle = theme.crosshair || '#9598a1';
      ctx.lineWidth = 1;

      // Vertical line
      ctx.beginPath();
      ctx.moveTo(snappedCenterX, 0);
      ctx.lineTo(snappedCenterX, plotHeightPx);
      ctx.stroke();

      // Horizontal line
      ctx.beginPath();
      ctx.moveTo(0, cursorY);
      ctx.lineTo(plotWidthPx, cursorY);
      ctx.stroke();
      ctx.restore();

      // Price Badge on Price Axis
      const hoverPrice = toPrice(cursorY);
      const pBadgeH = Math.round(18 * dpr);
      const pBadgeW = Math.round(56 * dpr);
      const pBadgeX = plotWidthPx + 2 * dpr;
      const pBadgeY = Math.round(cursorY - pBadgeH * 0.5);

      ctx.fillStyle = '#363a45';
      this.drawRoundedRect(ctx, pBadgeX, pBadgeY, pBadgeW, pBadgeH, 3 * dpr);
      ctx.fill();

      ctx.fillStyle = '#ffffff';
      ctx.font = `${Math.round(10 * dpr)}px ${theme.fontFamily}`;
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText(hoverPrice.toFixed(priceDecimals), pBadgeX + pBadgeW * 0.5, cursorY);

      // Time Badge on Time Axis
      const hoverTime = store.time[hoveredBarIndex];
      const timeParts = timeAxis.getLocalParts(hoverTime);
      const timeLabel = `${timeParts.day} ${timeParts.hours.toString().padStart(2, '0')}:${timeParts.minutes.toString().padStart(2, '0')}`;

      const tBadgeH = Math.round(18 * dpr);
      const tBadgeW = Math.round(70 * dpr);
      const tBadgeX = Math.round(snappedCenterX - tBadgeW * 0.5);
      const tBadgeY = plotHeightPx + 2 * dpr;

      ctx.fillStyle = '#363a45';
      this.drawRoundedRect(ctx, tBadgeX, tBadgeY, tBadgeW, tBadgeH, 3 * dpr);
      ctx.fill();

      ctx.fillStyle = '#ffffff';
      ctx.font = `${Math.round(10 * dpr)}px ${theme.fontFamily}`;
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText(timeLabel, snappedCenterX, tBadgeY + tBadgeH * 0.5);

      // 3. Top-left Candle Info Bar (OHLCV)
      const o = store.open[hoveredBarIndex];
      const h = store.high[hoveredBarIndex];
      const l = store.low[hoveredBarIndex];
      const c = store.close[hoveredBarIndex];
      const v = store.volume[hoveredBarIndex];
      const isUp = c >= o;
      const ohlcColor = isUp ? (theme.upColorHex ?? '#089981') : (theme.downColorHex ?? '#f23645');

      ctx.textAlign = 'left';
      ctx.textBaseline = 'top';
      ctx.font = `${Math.round(11 * dpr)}px ${theme.fontFamily}`;

      let textX = 12 * dpr;
      const textY = 10 * dpr;

      const drawLabelVal = (lbl: string, val: string, color: string) => {
        ctx.fillStyle = theme.axisText;
        ctx.fillText(lbl, textX, textY);
        textX += ctx.measureText(lbl).width + 3 * dpr;

        ctx.fillStyle = color;
        ctx.fillText(val, textX, textY);
        textX += ctx.measureText(val).width + 10 * dpr;
      };

      drawLabelVal('O', o.toFixed(priceDecimals), ohlcColor);
      drawLabelVal('H', h.toFixed(priceDecimals), ohlcColor);
      drawLabelVal('L', l.toFixed(priceDecimals), ohlcColor);
      drawLabelVal('C', c.toFixed(priceDecimals), ohlcColor);
      drawLabelVal('Vol', v.toLocaleString(), theme.axisText);
    }
  }

  private drawRoundedRect(
    ctx: CanvasRenderingContext2D,
    x: number,
    y: number,
    w: number,
    h: number,
    r: number
  ): void {
    ctx.beginPath();
    ctx.moveTo(x + r, y);
    ctx.lineTo(x + w - r, y);
    ctx.quadraticCurveTo(x + w, y, x + w, y + r);
    ctx.lineTo(x + w, y + h - r);
    ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
    ctx.lineTo(x + r, y + h);
    ctx.quadraticCurveTo(x, y + h, x, y + h - r);
    ctx.lineTo(x, y + r);
    ctx.quadraticCurveTo(x, y, x + r, y);
    ctx.closePath();
  }
}
