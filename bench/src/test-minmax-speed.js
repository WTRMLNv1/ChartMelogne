// Benchmark: getVisibleMinMax BEFORE (linear O(N) scan) vs AFTER (O(blocks) accelerated) at 450k bars
import { performance } from 'perf_hooks';

// Seeded PRNG
function createRng(seed = 42) {
  let s = seed | 0;
  return function next() {
    s = (s + 0x6d2b79f5) | 0;
    let t = Math.imul(s ^ (s >>> 15), 1 | s);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const BARS = 450_000;
console.log(`Generating ${BARS.toLocaleString()} synthetic bars...`);
const rng = createRng(1337);

const time = new Float64Array(BARS);
const open = new Float64Array(BARS);
const high = new Float64Array(BARS);
const low = new Float64Array(BARS);
const close = new Float64Array(BARS);
const volume = new Float64Array(BARS);
const flags = new Uint8Array(BARS);

let curPrice = 1000;
let curTime = Date.UTC(2020, 0, 1, 3, 45, 0);

for (let i = 0; i < BARS; i++) {
  time[i] = curTime;
  const delta = (rng() + rng() - 1.0) * 2;
  const o = curPrice;
  const c = o + delta;
  const h = Math.max(o, c) + rng() * 3;
  const l = Math.min(o, c) - rng() * 3;
  open[i] = o;
  high[i] = h;
  low[i] = l;
  close[i] = c;
  volume[i] = 1000;
  flags[i] = 0;
  curPrice = c;
  curTime += 60_000;
}

// === 1. BEFORE: Naive Linear Scan over all 450,000 bars ===
function naiveGetVisibleMinMax(lowArr, highArr, flagsArr, count, fromIndex, toIndex) {
  const start = Math.max(0, Math.floor(fromIndex));
  const end = Math.min(count, Math.ceil(toIndex));
  let min = Infinity;
  let max = -Infinity;
  for (let i = start; i < end; i++) {
    if ((flagsArr[i] & 1) !== 0) continue;
    const l = lowArr[i];
    const h = highArr[i];
    if (l < min) min = l;
    if (h > max) max = h;
  }
  return { min, max };
}

// === 2. AFTER: Block Summaries (Block Size 256) ===
const BLOCK_SIZE = 256;
const numBlocks = Math.ceil(BARS / BLOCK_SIZE);
const blockMin = new Float64Array(numBlocks);
const blockMax = new Float64Array(numBlocks);

// Build block summaries
for (let b = 0; b < numBlocks; b++) {
  const bStart = b << 8;
  const bEnd = Math.min(BARS, bStart + BLOCK_SIZE);
  let bMin = Infinity;
  let bMax = -Infinity;
  for (let i = bStart; i < bEnd; i++) {
    if ((flags[i] & 1) !== 0) continue;
    if (low[i] < bMin) bMin = low[i];
    if (high[i] > bMax) bMax = high[i];
  }
  blockMin[b] = bMin;
  blockMax[b] = bMax;
}

// Reusable result container to avoid allocations
const result = { min: 0, max: 1 };

function blockGetVisibleMinMax(lowArr, highArr, flagsArr, bMinArr, bMaxArr, count, fromIndex, toIndex) {
  const start = Math.max(0, Math.floor(fromIndex));
  const end = Math.min(count, Math.ceil(toIndex));
  if (start >= end) {
    const idx = Math.min(Math.max(0, start), count - 1);
    result.min = lowArr[idx];
    result.max = highArr[idx];
    return result;
  }

  let min = Infinity;
  let max = -Infinity;

  const startBlock = Math.ceil(start / 256);
  const endBlock = Math.floor(end / 256);

  if (startBlock >= endBlock) {
    for (let i = start; i < end; i++) {
      if ((flagsArr[i] & 1) !== 0) continue;
      const l = lowArr[i];
      const h = highArr[i];
      if (l < min) min = l;
      if (h > max) max = h;
    }
  } else {
    // 1. Partial head block
    const headEnd = startBlock << 8;
    for (let i = start; i < headEnd; i++) {
      if ((flagsArr[i] & 1) !== 0) continue;
      const l = lowArr[i];
      const h = highArr[i];
      if (l < min) min = l;
      if (h > max) max = h;
    }

    // 2. Full blocks
    for (let b = startBlock; b < endBlock; b++) {
      const bl = bMinArr[b];
      const bh = bMaxArr[b];
      if (bl < min) min = bl;
      if (bh > max) max = bh;
    }

    // 3. Partial tail block
    const tailStart = endBlock << 8;
    for (let i = tailStart; i < end; i++) {
      if ((flagsArr[i] & 1) !== 0) continue;
      const l = lowArr[i];
      const h = highArr[i];
      if (l < min) min = l;
      if (h > max) max = h;
    }
  }

  result.min = min;
  result.max = max;
  return result;
}

// 1. Correctness check
console.log('\n--- Verifying Output Parity ---');
const naiveRes = naiveGetVisibleMinMax(low, high, flags, BARS, 0, BARS);
const blockRes = blockGetVisibleMinMax(low, high, flags, blockMin, blockMax, BARS, 0, BARS);

if (naiveRes.min !== blockRes.min || naiveRes.max !== blockRes.max) {
  console.error('Mismatch at full zoom-out:', naiveRes, blockRes);
  process.exit(1);
}

// Check partial unaligned ranges
for (let offset = 0; offset < 500; offset += 37) {
  const nR = naiveGetVisibleMinMax(low, high, flags, BARS, offset, BARS - offset);
  const bR = blockGetVisibleMinMax(low, high, flags, blockMin, blockMax, BARS, offset, BARS - offset);
  if (nR.min !== bR.min || nR.max !== bR.max) {
    console.error(`Mismatch at offset ${offset}:`, nR, bR);
    process.exit(1);
  }
}
console.log('PARITY VERIFIED: 100% match across full range and partial offset ranges!');

// 2. Performance Comparison at Full Zoom-Out (450,000 bars)
console.log('\n--- Performance Benchmark: Full Zoom-Out (450,000 bars) ---');

// Warmup
for (let i = 0; i < 20; i++) {
  blockGetVisibleMinMax(low, high, flags, blockMin, blockMax, BARS, 0, BARS);
}

// AFTER: 10,000 iterations
const ITERS_AFTER = 10_000;
const tAfter0 = performance.now();
for (let i = 0; i < ITERS_AFTER; i++) {
  blockGetVisibleMinMax(low, high, flags, blockMin, blockMax, BARS, 0, BARS);
}
const elapsedAfterMs = (performance.now() - tAfter0) / ITERS_AFTER;

// BEFORE: 200 iterations
const ITERS_BEFORE = 200;
const tBefore0 = performance.now();
for (let i = 0; i < ITERS_BEFORE; i++) {
  naiveGetVisibleMinMax(low, high, flags, BARS, 0, BARS);
}
const elapsedBeforeMs = (performance.now() - tBefore0) / ITERS_BEFORE;

console.log(`BEFORE (naive 450k bar scan):       ${elapsedBeforeMs.toFixed(3)} ms (${(elapsedBeforeMs * 1000).toFixed(1)} µs)`);
console.log(`AFTER (block-accelerated B=256):    ${elapsedAfterMs.toFixed(3)} ms (${(elapsedAfterMs * 1000).toFixed(1)} µs)`);
console.log(`SPEEDUP:                            ${(elapsedBeforeMs / elapsedAfterMs).toFixed(1)}x faster!`);
console.log(`TIME SAVED PER FRAME:               ${(elapsedBeforeMs - elapsedAfterMs).toFixed(3)} ms`);
