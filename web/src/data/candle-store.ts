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

  constructor(initialCapacity: number = 1024) {
    this._capacity = Math.max(initialCapacity, 64);
    this.time = new Float64Array(this._capacity);
    this.open = new Float64Array(this._capacity);
    this.high = new Float64Array(this._capacity);
    this.low = new Float64Array(this._capacity);
    this.close = new Float64Array(this._capacity);
    this.volume = new Float64Array(this._capacity);
    this.flags = new Uint8Array(this._capacity);
    this.gpuBuffer = new Float32Array(this._capacity * 5);
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
    return idx;
  }

  getVisibleMinMax(fromIndex: number, toIndex: number): { min: number; max: number } {
    if (this._count === 0) {
      return { min: 0, max: 1 };
    }
    const start = Math.max(0, Math.floor(fromIndex));
    const end = Math.min(this._count, Math.ceil(toIndex));

    if (start >= end) {
      const idx = Math.min(Math.max(0, start), this._count - 1);
      return { min: this.low[idx], max: this.high[idx] };
    }

    let min = Infinity;
    let max = -Infinity;
    for (let i = start; i < end; i++) {
      // Ignore gap bars (bit 0 set)
      if ((this.flags[i] & 1) !== 0) continue;
      const l = this.low[i];
      const h = this.high[i];
      if (l < min) min = l;
      if (h > max) max = h;
    }

    if (min === Infinity || max === -Infinity) {
      return { min: 0, max: 1 };
    }
    return { min, max };
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
  }
}
