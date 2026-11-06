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
  volume: Uint32Array;
  flags: Uint8Array;

  // Single CPU copy of OHLC: interleaved f32 relative to basePrice (16 bytes per candle)
  // Format per candle: [open - base, high - base, low - base, close - base]
  ohlc: Float32Array;

  // Small reused scratch array for subdata uploads on append/updateLast (zero allocations)
  scratchInstance = new Float32Array(4);

  constructor(initialCapacity: number = 1024, utcOffsetMinutes: number = 330) {
    this._capacity = Math.max(initialCapacity, 64);
    this.utcOffsetMinutes = utcOffsetMinutes;
    this.time = new Float64Array(this._capacity);
    this.volume = new Uint32Array(this._capacity);
    this.flags = new Uint8Array(this._capacity);
    this.ohlc = new Float32Array(this._capacity * 4);

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

  // Getters reconstructing prices with basePrice
  getOpen(i: number): number {
    return this._basePrice + this.ohlc[i * 4];
  }

  getHigh(i: number): number {
    return this._basePrice + this.ohlc[i * 4 + 1];
  }

  getLow(i: number): number {
    return this._basePrice + this.ohlc[i * 4 + 2];
  }

  getClose(i: number): number {
    return this._basePrice + this.ohlc[i * 4 + 3];
  }

  getBar(i: number): Bar {
    const off = i * 4;
    return {
      time: this.time[i],
      open: this._basePrice + this.ohlc[off],
      high: this._basePrice + this.ohlc[off + 1],
      low: this._basePrice + this.ohlc[off + 2],
      close: this._basePrice + this.ohlc[off + 3],
      volume: this.volume[i],
      flags: this.flags[i],
    };
  }

  getMemoryUsage(): {
    storeBytes: number;
    cpuInstanceBufferBytes: number;
    totalBytes: number;
    details: {
      timeBytes: number;
      volumeBytes: number;
      flagsBytes: number;
      ohlcBytes: number;
      dayStartsBytes: number;
      blockSummaryBytes: number;
    };
  } {
    const timeBytes = this._capacity * 8; // Float64
    const volumeBytes = this._capacity * 4; // Uint32
    const flagsBytes = this._capacity * 1; // Uint8
    const ohlcBytes = this._capacity * 16; // Float32 x 4
    const dayStartsBytes = this.dayStarts.byteLength;
    const blockSummaryBytes = this.blockMin.byteLength + this.blockMax.byteLength;

    const storeBytes = timeBytes + volumeBytes + flagsBytes + ohlcBytes + dayStartsBytes + blockSummaryBytes;
    return {
      storeBytes,
      cpuInstanceBufferBytes: 0, // ohlc IS the store and IS the buffer uploaded directly
      totalBytes: storeBytes,
      details: {
        timeBytes,
        volumeBytes,
        flagsBytes,
        ohlcBytes,
        dayStartsBytes,
        blockSummaryBytes,
      },
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
    this.volume.set(data.volume.subarray(0, n));

    if (data.flags) {
      this.flags.set(data.flags.subarray(0, n));
    } else {
      this.flags.fill(0, 0, n);
    }

    // Set basePrice from first bar's close, or 0 if empty
    const base = n > 0 ? data.close[0] : 0;
    this._basePrice = base;

    // Pack OHLC relative to basePrice directly into the single interleaved Float32Array
    let off = 0;
    for (let i = 0; i < n; i++) {
      this.ohlc[off] = data.open[i] - base;
      this.ohlc[off + 1] = data.high[i] - base;
      this.ohlc[off + 2] = data.low[i] - base;
      this.ohlc[off + 3] = data.close[i] - base;
      off += 4;
    }

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
        const iOff = i * 4;
        const l = base + this.ohlc[iOff + 2];
        const h = base + this.ohlc[iOff + 1];
        if (l < bMin) bMin = l;
        if (h > bMax) bMax = h;
      }
      this.blockMin[b] = bMin === Infinity ? (bStart < n ? base + this.ohlc[bStart * 4 + 2] : 0) : bMin;
      this.blockMax[b] = bMax === -Infinity ? (bStart < n ? base + this.ohlc[bStart * 4 + 1] : 1) : bMax;
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

    const base = this._basePrice;
    this.time[idx] = bar.time;
    this.volume[idx] = bar.volume;
    this.flags[idx] = bar.flags ?? 0;

    const off = idx * 4;
    const relO = bar.open - base;
    const relH = bar.high - base;
    const relL = bar.low - base;
    const relC = bar.close - base;

    this.ohlc[off] = relO;
    this.ohlc[off + 1] = relH;
    this.ohlc[off + 2] = relL;
    this.ohlc[off + 3] = relC;

    // Populate reused scratch array for subdata upload
    this.scratchInstance[0] = relO;
    this.scratchInstance[1] = relH;
    this.scratchInstance[2] = relL;
    this.scratchInstance[3] = relC;

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
    const base = this._basePrice;

    this.time[idx] = bar.time;
    this.volume[idx] = bar.volume;
    if (bar.flags !== undefined) {
      this.flags[idx] = bar.flags;
    }

    const off = idx * 4;
    const relO = bar.open - base;
    const relH = bar.high - base;
    const relL = bar.low - base;
    const relC = bar.close - base;

    this.ohlc[off] = relO;
    this.ohlc[off + 1] = relH;
    this.ohlc[off + 2] = relL;
    this.ohlc[off + 3] = relC;

    // Populate reused scratch array for subdata upload
    this.scratchInstance[0] = relO;
    this.scratchInstance[1] = relH;
    this.scratchInstance[2] = relL;
    this.scratchInstance[3] = relC;

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
      const iOff = i * 4;
      const l = base + this.ohlc[iOff + 2];
      const h = base + this.ohlc[iOff + 1];
      if (l < bMin) bMin = l;
      if (h > bMax) bMax = h;
    }
    this.blockMin[b] = bMin === Infinity ? bar.low : bMin;
    this.blockMax[b] = bMax === -Infinity ? bar.high : bMax;

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
      this._minMaxResult.min = this.getLow(idx);
      this._minMaxResult.max = this.getHigh(idx);
      return this._minMaxResult;
    }

    let min = Infinity;
    let max = -Infinity;
    const base = this._basePrice;
    const ohlc = this.ohlc;
    const flags = this.flags;

    const startBlock = Math.ceil(start / 256);
    const endBlock = Math.floor(end / 256);

    if (startBlock >= endBlock) {
      // Small range or within 1-2 blocks: direct scan
      for (let i = start; i < end; i++) {
        if ((flags[i] & 1) !== 0) continue;
        const off = i * 4;
        const l = base + ohlc[off + 2];
        const h = base + ohlc[off + 1];
        if (l < min) min = l;
        if (h > max) max = h;
      }
    } else {
      // 1. Partial head block at the left edge
      const headEnd = startBlock << 8;
      for (let i = start; i < headEnd; i++) {
        if ((flags[i] & 1) !== 0) continue;
        const off = i * 4;
        const l = base + ohlc[off + 2];
        const h = base + ohlc[off + 1];
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
        if ((flags[i] & 1) !== 0) continue;
        const off = i * 4;
        const l = base + ohlc[off + 2];
        const h = base + ohlc[off + 1];
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

  private reallocate(newCap: number): void {
    this._capacity = newCap;

    const nextTime = new Float64Array(newCap);
    nextTime.set(this.time.subarray(0, this._count));
    this.time = nextTime;

    const nextVol = new Uint32Array(newCap);
    nextVol.set(this.volume.subarray(0, this._count));
    this.volume = nextVol;

    const nextFlags = new Uint8Array(newCap);
    nextFlags.set(this.flags.subarray(0, this._count));
    this.flags = nextFlags;

    const nextOhlc = new Float32Array(newCap * 4);
    nextOhlc.set(this.ohlc.subarray(0, this._count * 4));
    this.ohlc = nextOhlc;

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
