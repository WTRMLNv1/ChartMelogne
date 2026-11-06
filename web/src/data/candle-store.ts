import type { Bar, CandleData } from '../types.js';

export interface ValidationError {
  index: number;
  reason: string;
}

export function validateCandleData(data: CandleData, count: number): ValidationError | null {
  const { time, open, high, low, close } = data;
  for (let i = 0; i < count; i++) {
    const t = time[i];
    const o = open[i];
    const h = high[i];
    const l = low[i];
    const c = close[i];

    if (Number.isNaN(o) || Number.isNaN(h) || Number.isNaN(l) || Number.isNaN(c)) {
      return { index: i, reason: `NaN value in OHLC at index ${i}` };
    }
    if (i > 0 && t <= time[i - 1]) {
      return { index: i, reason: `Timestamp not strictly increasing at index ${i}: ${t} <= ${time[i - 1]}` };
    }
    const maxOC = o > c ? o : c;
    const minOC = o < c ? o : c;
    if (h < maxOC) {
      return { index: i, reason: `High (${h}) < max(Open, Close) (${maxOC}) at index ${i}` };
    }
    if (l > minOC) {
      return { index: i, reason: `Low (${l}) > min(Open, Close) (${minOC}) at index ${i}` };
    }
  }
  return null;
}

export class CandleStore {
  private _capacity: number;
  private _count: number = 0;
  private _basePrice: number = 0;

  utcOffsetMinutes: number;
  dayStarts: Uint32Array;
  dayCount: number = 0;
  private lastLocalDay: number = -Infinity;

  // Block summaries for O(pixels) visible min/max queries (block size 256)
  readonly blockSize: number = 256;
  blockMin: Float64Array;
  blockMax: Float64Array;
  blockCount: number = 0;
  private _minMaxResult = { min: 0, max: 1 };

  time: Float64Array;
  open: Float64Array;
  high: Float64Array;
  low: Float64Array;
  close: Float64Array;
  volume: Float64Array;
  flags: Uint8Array;

  // Packed f32 buffer for WebGL instances: [x, o, h, l, c] relative to basePrice
  // 5 floats (20 bytes) per bar
  gpuBuffer: Float32Array;

  constructor(initialCapacity: number = 1024, utcOffsetMinutes: number = 330) {
    this._capacity = Math.max(initialCapacity, 64);
    this.utcOffsetMinutes = utcOffsetMinutes;
    this.time = new Float64Array(this._capacity);
    this.open = new Float64Array(this._capacity);
    this.high = new Float64Array(this._capacity);
    this.low = new Float64Array(this._capacity);
    this.close = new Float64Array(this._capacity);
    this.volume = new Float64Array(this._capacity);
    this.flags = new Uint8Array(this._capacity);
    this.gpuBuffer = new Float32Array(this._capacity * 5);
    this.dayStarts = new Uint32Array(Math.max(64, Math.ceil(this._capacity / 100)));
    this.dayCount = 0;

    const blockCap = Math.max(16, Math.ceil(this._capacity / 256));
    this.blockMin = new Float64Array(blockCap);
    this.blockMax = new Float64Array(blockCap);
    this.blockCount = 0;
  }

  get count(): number {
    return this._count;
  }

  get capacity(): number {
    return this._capacity;
  }

  get basePrice(): number {
    return this._basePrice;
  }

  getMemoryUsage(): { storeBytes: number; cpuInstanceBufferBytes: number; totalBytes: number } {
    // 6 columns of Float64 (8 bytes each) + 1 column of Uint8 flags (1 byte)
    const storeBytes = this._capacity * (6 * 8 + 1);
    // Packed f32 GPU buffer (5 floats * 4 bytes = 20 bytes per slot)
    const cpuInstanceBufferBytes = this._capacity * 5 * 4;
    return {
      storeBytes,
      cpuInstanceBufferBytes,
      totalBytes: storeBytes + cpuInstanceBufferBytes,
    };
  }

  setData(data: CandleData, debug: boolean = true): void {
    const n = data.time.length;
    if (
      data.open.length !== n ||
      data.high.length !== n ||
      data.low.length !== n ||
      data.close.length !== n ||
      data.volume.length !== n
    ) {
      throw new Error(`Mismatched array lengths in setData: time has ${n}`);
    }

    if (debug) {
      const err = validateCandleData(data, n);
      if (err) {
        throw new Error(`Validation failed at index ${err.index}: ${err.reason}`);
      }
    }

    if (n > this._capacity) {
      let newCap = this._capacity;
      while (newCap < n) {
        newCap *= 2;
      }
      this.reallocate(newCap);
    }

    this._count = n;
    this.time.set(data.time.subarray(0, n));
    this.open.set(data.open.subarray(0, n));
    this.high.set(data.high.subarray(0, n));
    this.low.set(data.low.subarray(0, n));
    this.close.set(data.close.subarray(0, n));
    this.volume.set(data.volume.subarray(0, n));

    if (data.flags) {
      this.flags.set(data.flags.subarray(0, n));
    } else {
      this.flags.fill(0, 0, n);
    }

    // Set basePrice from first bar's close, or 0 if empty
    this._basePrice = n > 0 ? this.close[0] : 0;

    // Pack into gpuBuffer
    this.rebuildGpuBuffer(0, n);

    // Build dayStarts (bar index of the first bar of each unique local day) in O(n) integer math
    const utcOffsetMs = this.utcOffsetMinutes * 60_000;
    let dayCap = Math.max(128, Math.ceil(n / 100));
    if (this.dayStarts.length < dayCap) {
      this.dayStarts = new Uint32Array(dayCap);
    }
    let days = 0;
    let prevDay = -Infinity;
    for (let i = 0; i < n; i++) {
      const localDay = Math.floor((this.time[i] + utcOffsetMs) / 86_400_000);
      if (localDay !== prevDay) {
        if (days >= this.dayStarts.length) {
          const next = new Uint32Array(this.dayStarts.length * 2);
          next.set(this.dayStarts.subarray(0, days));
          this.dayStarts = next;
        }
        this.dayStarts[days++] = i;
        prevDay = localDay;
      }
    }
    this.dayCount = days;
    this.lastLocalDay = prevDay;

    // Build block min/max summaries (block size 256)
    const numBlocks = Math.ceil(n / 256);
    if (this.blockMin.length < numBlocks) {
      this.blockMin = new Float64Array(numBlocks);
      this.blockMax = new Float64Array(numBlocks);
    }
    this.blockCount = numBlocks;

    for (let b = 0; b < numBlocks; b++) {
      const bStart = b << 8;
      const bEnd = Math.min(n, bStart + 256);
      let bMin = Infinity;
      let bMax = -Infinity;
      for (let i = bStart; i < bEnd; i++) {
        if ((this.flags[i] & 1) !== 0) continue;
        const l = this.low[i];
        const h = this.high[i];
        if (l < bMin) bMin = l;
        if (h > bMax) bMax = h;
      }
      this.blockMin[b] = bMin === Infinity ? (bStart < n ? this.low[bStart] : 0) : bMin;
      this.blockMax[b] = bMax === -Infinity ? (bStart < n ? this.high[bStart] : 1) : bMax;
    }
  }

  append(bar: Bar): number {
    const idx = this._count;
    if (idx >= this._capacity) {
      this.reallocate(this._capacity * 2);
    }

    if (idx === 0) {
      this._basePrice = bar.close;
    }

    this.time[idx] = bar.time;
    this.open[idx] = bar.open;
    this.high[idx] = bar.high;
    this.low[idx] = bar.low;
    this.close[idx] = bar.close;
    this.volume[idx] = bar.volume;
    this.flags[idx] = bar.flags ?? 0;

    this.writeGpuInstance(idx);
    this._count++;

    // Update dayStarts
    const utcOffsetMs = this.utcOffsetMinutes * 60_000;
    const localDay = Math.floor((bar.time + utcOffsetMs) / 86_400_000);
    if (this.dayCount === 0 || localDay !== this.lastLocalDay) {
      if (this.dayCount >= this.dayStarts.length) {
        const next = new Uint32Array(Math.max(128, this.dayStarts.length * 2));
        next.set(this.dayStarts.subarray(0, this.dayCount));
        this.dayStarts = next;
      }
      this.dayStarts[this.dayCount++] = idx;
      this.lastLocalDay = localDay;
    }

    // Update block min/max summaries
    const b = idx >> 8;
    if (b >= this.blockCount) {
      if (b >= this.blockMin.length) {
        const nextCap = Math.max(16, this.blockMin.length * 2);
        const nextMin = new Float64Array(nextCap);
        const nextMax = new Float64Array(nextCap);
        nextMin.set(this.blockMin.subarray(0, this.blockCount));
        nextMax.set(this.blockMax.subarray(0, this.blockCount));
        this.blockMin = nextMin;
        this.blockMax = nextMax;
      }
      this.blockMin[b] = bar.low;
      this.blockMax[b] = bar.high;
      this.blockCount = b + 1;
    } else {
      if (bar.low < this.blockMin[b]) this.blockMin[b] = bar.low;
      if (bar.high > this.blockMax[b]) this.blockMax[b] = bar.high;
    }

    return idx;
  }

  updateLast(bar: Bar): number {
    if (this._count === 0) {
      return this.append(bar);
    }
    const idx = this._count - 1;
    this.time[idx] = bar.time;
    this.open[idx] = bar.open;
    this.high[idx] = bar.high;
    this.low[idx] = bar.low;
    this.close[idx] = bar.close;
    this.volume[idx] = bar.volume;
    if (bar.flags !== undefined) {
      this.flags[idx] = bar.flags;
    }

    this.writeGpuInstance(idx);

    // Keep dayStarts consistent
    const utcOffsetMs = this.utcOffsetMinutes * 60_000;
    const localDay = Math.floor((bar.time + utcOffsetMs) / 86_400_000);
    if (this.dayCount > 0 && this.dayStarts[this.dayCount - 1] === idx) {
      this.lastLocalDay = localDay;
    }

    // Update block min/max summaries for the affected block
    const b = idx >> 8;
    const bStart = b << 8;
    const bEnd = idx + 1;
    let bMin = Infinity;
    let bMax = -Infinity;
    for (let i = bStart; i < bEnd; i++) {
      if ((this.flags[i] & 1) !== 0) continue;
      const l = this.low[i];
      const h = this.high[i];
      if (l < bMin) bMin = l;
      if (h > bMax) bMax = h;
    }
    this.blockMin[b] = bMin === Infinity ? this.low[idx] : bMin;
    this.blockMax[b] = bMax === -Infinity ? this.high[idx] : bMax;

    return idx;
  }

  getVisibleMinMax(fromIndex: number, toIndex: number): { min: number; max: number } {
    if (this._count === 0) {
      this._minMaxResult.min = 0;
      this._minMaxResult.max = 1;
      return this._minMaxResult;
    }
    const start = Math.max(0, Math.floor(fromIndex));
    const end = Math.min(this._count, Math.ceil(toIndex));

    if (start >= end) {
      const idx = Math.min(Math.max(0, start), this._count - 1);
      this._minMaxResult.min = this.low[idx];
      this._minMaxResult.max = this.high[idx];
      return this._minMaxResult;
    }

    let min = Infinity;
    let max = -Infinity;

    const startBlock = Math.ceil(start / 256);
    const endBlock = Math.floor(end / 256);

    if (startBlock >= endBlock) {
      // Small range or within 1-2 blocks: direct scan
      for (let i = start; i < end; i++) {
        if ((this.flags[i] & 1) !== 0) continue;
        const l = this.low[i];
        const h = this.high[i];
        if (l < min) min = l;
        if (h > max) max = h;
      }
    } else {
      // 1. Partial head block at the left edge
      const headEnd = startBlock << 8;
      for (let i = start; i < headEnd; i++) {
        if ((this.flags[i] & 1) !== 0) continue;
        const l = this.low[i];
        const h = this.high[i];
        if (l < min) min = l;
        if (h > max) max = h;
      }

      // 2. Full blocks in O(1) per block
      const bMin = this.blockMin;
      const bMax = this.blockMax;
      for (let b = startBlock; b < endBlock; b++) {
        const bl = bMin[b];
        const bh = bMax[b];
        if (bl < min) min = bl;
        if (bh > max) max = bh;
      }

      // 3. Partial tail block at the right edge
      const tailStart = endBlock << 8;
      for (let i = tailStart; i < end; i++) {
        if ((this.flags[i] & 1) !== 0) continue;
        const l = this.low[i];
        const h = this.high[i];
        if (l < min) min = l;
        if (h > max) max = h;
      }
    }

    if (min === Infinity || max === -Infinity) {
      this._minMaxResult.min = 0;
      this._minMaxResult.max = 1;
    } else {
      this._minMaxResult.min = min;
      this._minMaxResult.max = max;
    }
    return this._minMaxResult;
  }

  private writeGpuInstance(i: number): void {
    const offset = i * 5;
    const base = this._basePrice;
    this.gpuBuffer[offset] = i;
    this.gpuBuffer[offset + 1] = this.open[i] - base;
    this.gpuBuffer[offset + 2] = this.high[i] - base;
    this.gpuBuffer[offset + 3] = this.low[i] - base;
    this.gpuBuffer[offset + 4] = this.close[i] - base;
  }

  private rebuildGpuBuffer(start: number, end: number): void {
    const base = this._basePrice;
    let offset = start * 5;
    for (let i = start; i < end; i++) {
      this.gpuBuffer[offset++] = i;
      this.gpuBuffer[offset++] = this.open[i] - base;
      this.gpuBuffer[offset++] = this.high[i] - base;
      this.gpuBuffer[offset++] = this.low[i] - base;
      this.gpuBuffer[offset++] = this.close[i] - base;
    }
  }

  private reallocate(newCap: number): void {
    const oldCap = this._capacity;
    this._capacity = newCap;

    const copyF64 = (oldArr: Float64Array) => {
      const next = new Float64Array(newCap);
      next.set(oldArr.subarray(0, this._count));
      return next;
    };

    this.time = copyF64(this.time);
    this.open = copyF64(this.open);
    this.high = copyF64(this.high);
    this.low = copyF64(this.low);
    this.close = copyF64(this.close);
    this.volume = copyF64(this.volume);

    const nextFlags = new Uint8Array(newCap);
    nextFlags.set(this.flags.subarray(0, this._count));
    this.flags = nextFlags;

    const nextGpu = new Float32Array(newCap * 5);
    nextGpu.set(this.gpuBuffer.subarray(0, this._count * 5));
    this.gpuBuffer = nextGpu;

    const blockCap = Math.max(16, Math.ceil(newCap / 256));
    if (this.blockMin.length < blockCap) {
      const nextBlockMin = new Float64Array(blockCap);
      const nextBlockMax = new Float64Array(blockCap);
      nextBlockMin.set(this.blockMin.subarray(0, this.blockCount));
      nextBlockMax.set(this.blockMax.subarray(0, this.blockCount));
      this.blockMin = nextBlockMin;
      this.blockMax = nextBlockMax;
    }
  }
}
