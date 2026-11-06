/**
 * Core type definitions for ChartMelogne.
 */

export interface CandleData {
  time: Float64Array;
  open: Float64Array;
  high: Float64Array;
  low: Float64Array;
  close: Float64Array;
  volume: Float64Array;
  flags?: Uint8Array;
}

export interface Bar {
  time: number;
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
  flags?: number;
}

export interface ViewportState {
  fromIndex: number;
  barsVisible: number;
  yMin: number;
  yMax: number;
  yMode?: 'auto' | 'manual';
}

export interface Theme {
  background: string;
  grid: string;
  axisText: string;
  crosshair: string;
  upColor: [number, number, number, number];    // RGBA in 0..1
  downColor: [number, number, number, number];  // RGBA in 0..1
  upColorHex?: string;
  downColorHex?: string;
  fontFamily: string;
  fontSize: number;
}

export interface ChartOptions {
  theme?: Theme;
  utcOffsetMinutes?: number;
  priceDecimals?: number;
  dpr?: number;
}

export interface CandleSeriesOptions {
  capacity?: number;
  pane?: number;
}
