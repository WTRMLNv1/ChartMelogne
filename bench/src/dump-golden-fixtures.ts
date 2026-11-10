import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { fileURLToPath } from 'url';

import { createRng, generateSyntheticCandles } from '../../web/src/data/synthetic.ts';
import { CandleStore } from '../../web/src/data/candle-store.ts';
import { Viewport } from '../../web/src/viewport/viewport.ts';
import { generatePriceTicks } from '../../web/src/axis/price-axis.ts';
import { TimeAxis } from '../../web/src/axis/time-axis.ts';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const fixturesDir = path.resolve(__dirname, '../fixtures');

try {
  if (!fs.existsSync(fixturesDir)) {
    fs.mkdirSync(fixturesDir, { recursive: true });
  }
} catch (e) {
  // Directory might already exist or sandboxed
}

console.log('Generating golden fixtures...');

// 1. Generate synthetic dataset: 10,000 bars, seed 1337, startPrice 2500.0
const COUNT = 10_000;
const SEED = 1337;
const START_PRICE = 2500.0;
const UTC_OFFSET_MINUTES = 330;

const candles = generateSyntheticCandles({
  count: COUNT,
  seed: SEED,
  startPrice: START_PRICE,
});

function saveJson(filename: string, data: any) {
  const jsonStr = JSON.stringify(data, null, 2);
  const targetPath = path.join(fixturesDir, filename);
  try {
    fs.writeFileSync(targetPath, jsonStr, 'utf-8');
    console.log(`Saved ${filename} directly to ${targetPath}`);
  } catch (err) {
    const tmpPath = path.join(os.tmpdir(), filename);
    fs.writeFileSync(tmpPath, jsonStr, 'utf-8');
    console.log(`Fallback: saved ${filename} to ${tmpPath}`);
  }
}
const goldenCandles = {
  time: Array.from(candles.time),
  open: Array.from(candles.open),
  high: Array.from(candles.high),
  low: Array.from(candles.low),
  close: Array.from(candles.close),
  volume: Array.from(candles.volume),
};
saveJson('golden-candles.json', goldenCandles);

// 2. Load into CandleStore
const store = new CandleStore(COUNT, UTC_OFFSET_MINUTES);
store.setData(candles, true);

// 3. Viewport and Ticks for 4 zoom levels
// Viewport sized 1000 x 600 at dpr 1
// Layout plotWidth = 1000 - 65 = 935, plotHeight = 600 - 28 = 572
const PLOT_WIDTH = 935;
const PLOT_HEIGHT = 572;
const timeAxis = new TimeAxis({ utcOffsetMinutes: UTC_OFFSET_MINUTES });

const zoomLevels = [
  { name: 'zoomed_in_intraday', fromIndex: 500, barsVisible: 50 },
  { name: 'medium_daily', fromIndex: 1000, barsVisible: 400 },
  { name: 'wide_multiweek', fromIndex: 2000, barsVisible: 2500 },
  { name: 'full_zoom_out', fromIndex: 0, barsVisible: 10_000 },
];

const viewportTicksResult: Record<string, any> = {};

for (const level of zoomLevels) {
  const vp = new Viewport();
  vp.setSize(PLOT_WIDTH, PLOT_HEIGHT, 1);
  vp.setTotalBars(COUNT);
  vp.setVisibleRange(level.fromIndex, level.fromIndex + level.barsVisible);

  const { min, max } = store.getVisibleMinMax(vp.fromIndex, vp.fromIndex + vp.barsVisible);
  vp.updateAutoY(min, max, true);

  const firstVisible = Math.max(0, Math.floor(vp.fromIndex - 2));
  const lastVisible = Math.min(store.count - 1, Math.ceil(vp.fromIndex + vp.barsVisible + 2));

  const priceTicks = generatePriceTicks(vp.yMin, vp.yMax, PLOT_HEIGHT);
  const timeTicks = timeAxis.generateTicks(
    store.time,
    store.count,
    vp.fromIndex,
    vp.barsVisible,
    PLOT_WIDTH,
    store.dayStarts,
    store.dayCount
  );

  viewportTicksResult[level.name] = {
    fromIndex: vp.fromIndex,
    barsVisible: vp.barsVisible,
    viewportState: vp.getState(),
    visibleRange: {
      first: firstVisible,
      last: lastVisible,
    },
    minMax: { min, max },
    priceTicks: priceTicks.map((t) => ({ price: t.price, label: t.label })),
    timeTicks: timeTicks.map((t) => ({
      barIndex: t.barIndex,
      timeMs: t.timeMs,
      label: t.label,
      isMajor: t.isMajor,
    })),
  };
}

saveJson('golden-viewport-ticks.json', viewportTicksResult);

// 4. PRNG & Formatting fixtures
const rng = createRng(SEED);
const prngFirst64: number[] = [];
for (let i = 0; i < 64; i++) {
  prngFirst64.push(rng());
}

const first3Bars = [0, 1, 2].map((i) => store.getBar(i));
const last3Bars = [COUNT - 3, COUNT - 2, COUNT - 1].map((i) => store.getBar(i));

const testPrices = [
  0.125,
  2.5,
  1.005,
  0.5,
  1.5,
  2.675,
  0.0,
  1.0,
  1234.5678,
  0.0001,
  99.999,
  10.05,
  2.005,
  3.14159265,
  100000.5,
  0.055,
  1.25,
  0.375,
  0.625,
  0.875,
];

const toFixedFixtures = testPrices.map((val) => ({
  value: val,
  formatted: [0, 1, 2, 3, 4].map((d) => val.toFixed(d)),
}));

const badgeIndices = [0, 1, 50, 374, 375, 1000, 2500, 5000, 7500, 9999];
const crosshairTimeBadges = badgeIndices.map((idx) => {
  const timeMs = store.time[idx];
  const parts = timeAxis.getLocalParts(timeMs);
  const label = `${parts.day} ${parts.hours.toString().padStart(2, '0')}:${parts.minutes.toString().padStart(2, '0')}`;
  return { barIndex: idx, timeMs, label };
});

const hudIndices = [0, 100, 1000, 5000, 9999];
const hudOhlcv = hudIndices.map((idx) => {
  const o = store.getOpen(idx);
  const h = store.getHigh(idx);
  const l = store.getLow(idx);
  const c = store.getClose(idx);
  const v = store.volume[idx];
  return {
    barIndex: idx,
    open: o.toFixed(2),
    high: h.toFixed(2),
    low: l.toFixed(2),
    close: c.toFixed(2),
    volume: v.toLocaleString('en-US'),
  };
});

const goldenPrngFormatting = {
  prng_seed: SEED,
  prng_first_64: prngFirst64,
  first_3_bars: first3Bars,
  last_3_bars: last3Bars,
  to_fixed: toFixedFixtures,
  crosshair_time_badges: crosshairTimeBadges,
  hud_ohlcv: hudOhlcv,
};

saveJson('golden-prng-formatting.json', goldenPrngFormatting);
console.log('Fixture generation completed successfully!');
