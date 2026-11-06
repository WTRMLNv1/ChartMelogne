import type { CandleData } from '../types.js';

/**
 * Fast 32-bit PRNG (Mulberry32) for reproducible synthetic benchmarks.
 */
export function createRng(seed: number = 42): () => number {
  let s = seed | 0;
  return function next(): number {
    s = (s + 0x6d2b79f5) | 0;
    let t = Math.imul(s ^ (s >>> 15), 1 | s);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export interface SyntheticDataOptions {
  count?: number;
  startPrice?: number;
  startTimeMs?: number;
  intervalMs?: number;
  volatility?: number;
  seed?: number;
}

/**
 * Generates reproducible synthetic OHLCV data using a seeded random walk.
 */
export function generateSyntheticCandles(options: SyntheticDataOptions = {}): CandleData {
  const {
    count = 450_000,
    startPrice = 1000.0,
    startTimeMs = Date.UTC(2020, 0, 1, 9, 15, 0),
    intervalMs = 60_000, // 1 minute
    volatility = 0.002,
    seed = 1337,
  } = options;

  const rng = createRng(seed);

  const time = new Float64Array(count);
  const open = new Float64Array(count);
  const high = new Float64Array(count);
  const low = new Float64Array(count);
  const close = new Float64Array(count);
  const volume = new Float64Array(count);
  const flags = new Uint8Array(count);

  let currentPrice = startPrice;
  let currentTime = startTimeMs;

  for (let i = 0; i < count; i++) {
    time[i] = currentTime;

    // Normal-ish distribution approximation using Irwin-Hall (sum of 4 uniform)
    const delta = (rng() + rng() + rng() + rng() - 2.0) * volatility * currentPrice;
    const o = currentPrice;
    let c = o + delta;
    if (c <= 1.0) c = 1.0; // avoid negative or zero price

    const maxOC = Math.max(o, c);
    const minOC = Math.min(o, c);

    // Wick expansions
    const upperWick = rng() * volatility * currentPrice * 1.5;
    const lowerWick = rng() * volatility * currentPrice * 1.5;

    const h = maxOC + upperWick;
    const l = Math.max(0.1, minOC - lowerWick);

    open[i] = o;
    high[i] = h;
    low[i] = l;
    close[i] = c;

    // Volume random positive
    volume[i] = Math.floor(100 + rng() * 5000);
    flags[i] = 0;

    currentPrice = c;
    currentTime += intervalMs;
  }

  return { time, open, high, low, close, volume, flags };
}
