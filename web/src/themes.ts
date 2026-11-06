import type { Theme } from './types.js';

export const darkTheme: Theme = {
  background: '#131722',
  grid: '#242732',
  axisText: '#787b86',
  crosshair: '#9598a1',
  upColor: [0.03, 0.65, 0.55, 1.0],     // #089981 in normalized RGBA
  downColor: [0.95, 0.25, 0.33, 1.0],   // #f23645 in normalized RGBA
  upColorHex: '#089981',
  downColorHex: '#f23645',
  fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Oxygen, Ubuntu, Cantarell, "Helvetica Neue", sans-serif',
  fontSize: 11,
};

export const lightTheme: Theme = {
  background: '#ffffff',
  grid: '#f0f3fa',
  axisText: '#787b86',
  crosshair: '#9598a1',
  upColor: [0.03, 0.65, 0.55, 1.0],
  downColor: [0.95, 0.25, 0.33, 1.0],
  upColorHex: '#089981',
  downColorHex: '#f23645',
  fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Oxygen, Ubuntu, Cantarell, "Helvetica Neue", sans-serif',
  fontSize: 11,
};
