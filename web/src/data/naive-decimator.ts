import type { CandleData } from '../types.js';

export interface DecimatedCandle {
  rawIndex: number;
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
  count: number;
}

/**
 * Naive reference decimator (Section 5.6).
 * Iterates sequentially over raw bars, bucketing by bucketSize = 2^k.
 * Serves as the ground truth reference to verify min/max pyramid implementations.
 */
export function naiveDecimate(data: CandleData, count: number, k: number): DecimatedCandle[] {
  const bucketSize = 1 << k;
  if (count <= 0) return [];

  const numBuckets = Math.ceil(count / bucketSize);
  const result: DecimatedCandle[] = new Array(numBuckets);

  for (let b = 0; b < numBuckets; b++) {
    const startIdx = b * bucketSize;
    const endIdx = Math.min(count, startIdx + bucketSize);

    let o = data.open[startIdx];
    let h = -Infinity;
    let l = Infinity;
    let c = data.close[endIdx - 1];
    let vol = 0;
    let validCount = 0;

    for (let i = startIdx; i < endIdx; i++) {
      // Check gap flag (bit 0)
      if (data.flags && (data.flags[i] & 1) !== 0) {
        continue;
      }
      validCount++;
      if (data.high[i] > h) h = data.high[i];
      if (data.low[i] < l) l = data.low[i];
      vol += data.volume[i];
    }

    if (validCount === 0) {
      // Gap bucket
      h = o;
      l = o;
      c = o;
    }

    result[b] = {
      rawIndex: startIdx,
      open: o,
      high: h,
      low: l,
      close: c,
      volume: vol,
      count: validCount,
    };
  }

  return result;
}
