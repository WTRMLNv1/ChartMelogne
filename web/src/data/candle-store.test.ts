import { test, describe } from 'node:test';
import assert from 'node:assert';
import { CandleStore, validateCandleData } from './candle-store.js';
import { generateSyntheticCandles } from './synthetic.js';

describe('CandleStore & Validation', () => {
  test('validates synthetic data successfully', () => {
    const data = generateSyntheticCandles({ count: 10_000, seed: 123 });
    const err = validateCandleData(data, data.time.length);
    assert.strictEqual(err, null, 'Generated synthetic candles should be valid');
  });

  test('detects NaN values in OHLC', () => {
    const data = generateSyntheticCandles({ count: 10 });
    data.high[3] = NaN;
    const err = validateCandleData(data, 10);
    assert.notStrictEqual(err, null);
    assert.strictEqual(err?.index, 3);
  });

  test('detects invalid high < max(open, close)', () => {
    const data = generateSyntheticCandles({ count: 10 });
    data.high[2] = Math.min(data.open[2], data.close[2]) - 1;
    const err = validateCandleData(data, 10);
    assert.notStrictEqual(err, null);
    assert.strictEqual(err?.index, 2);
  });

  test('detects non-monotonic timestamps', () => {
    const data = generateSyntheticCandles({ count: 10 });
    data.time[4] = data.time[3];
    const err = validateCandleData(data, 10);
    assert.notStrictEqual(err, null);
    assert.strictEqual(err?.index, 4);
  });

  test('setData correctly populates store and packed ohlc buffer', () => {
    const store = new CandleStore(100);
    const data = generateSyntheticCandles({ count: 500, startPrice: 100 });
    store.setData(data);

    assert.strictEqual(store.count, 500);
    assert.ok(store.capacity >= 500);
    assert.strictEqual(store.basePrice, data.close[0]);

    // Check packed 16-byte OHLC buffer (4 floats per bar: open, high, low, close)
    const base = store.basePrice;
    assert.strictEqual(Math.fround(store.ohlc[0]), Math.fround(data.open[0] - base));
    assert.strictEqual(Math.fround(store.ohlc[1]), Math.fround(data.high[0] - base));
    assert.strictEqual(Math.fround(store.ohlc[2]), Math.fround(data.low[0] - base));
    assert.strictEqual(Math.fround(store.ohlc[3]), Math.fround(data.close[0] - base));

    // Check last element packed
    const lastIdx = 499;
    const offset = lastIdx * 4;
    assert.strictEqual(Math.fround(store.ohlc[offset]), Math.fround(data.open[lastIdx] - base));
    assert.strictEqual(Math.fround(store.ohlc[offset + 1]), Math.fround(data.high[lastIdx] - base));
    assert.strictEqual(Math.fround(store.ohlc[offset + 2]), Math.fround(data.low[lastIdx] - base));
    assert.strictEqual(Math.fround(store.ohlc[offset + 3]), Math.fround(data.close[lastIdx] - base));
  });

  test('append and updateLast mutate ohlc and scratchInstance in O(1)', () => {
    const store = new CandleStore(2);
    store.append({
      time: 1000,
      open: 100,
      high: 105,
      low: 95,
      close: 102,
      volume: 10,
    });
    assert.strictEqual(store.count, 1);
    assert.strictEqual(store.basePrice, 102);
    assert.strictEqual(Math.fround(store.getOpen(0)), 100);
    assert.strictEqual(Math.fround(store.getHigh(0)), 105);

    store.updateLast({
      time: 1000,
      open: 100,
      high: 108,
      low: 95,
      close: 107,
      volume: 15,
    });
    assert.strictEqual(store.count, 1);
    assert.strictEqual(Math.fround(store.getHigh(0)), 108);
    assert.strictEqual(Math.fround(store.getClose(0)), 107);
    assert.strictEqual(Math.fround(store.scratchInstance[1]), Math.fround(108 - 102));

    store.append({
      time: 2000,
      open: 107,
      high: 110,
      low: 106,
      close: 109,
      volume: 20,
    });
    assert.strictEqual(store.count, 2);
    assert.strictEqual(Math.fround(store.getClose(1)), 109);
    assert.strictEqual(Math.fround(store.scratchInstance[3]), Math.fround(109 - 102));
  });

  test('getVisibleMinMax returns bounds for visible range', () => {
    const store = new CandleStore(10);
    const data = generateSyntheticCandles({ count: 50 });
    store.setData(data);

    const { min, max } = store.getVisibleMinMax(10, 20);
    let expectedMin = Infinity;
    let expectedMax = -Infinity;
    for (let i = 10; i <= 20; i++) {
      const l = store.getLow(i);
      const h = store.getHigh(i);
      if (l < expectedMin) expectedMin = l;
      if (h > expectedMax) expectedMax = h;
    }
    assert.strictEqual(min, expectedMin);
    assert.strictEqual(max, expectedMax);
  });

  test('crosshair and tooltip OHLC values match original input within f32 tolerance across high-price instruments (Rs 1,30,000)', () => {
    const basePrices = [100.0, 2500.0, 25000.0, 130000.0];

    for (const basePrice of basePrices) {
      const COUNT = 1000;
      const data = generateSyntheticCandles({
        count: COUNT,
        startPrice: basePrice,
        volatility: 0.002,
        seed: 42,
      });

      const store = new CandleStore(COUNT);
      store.setData(data);

      // Verify each bar reconstructed from store.getBar / getOpen / getClose matches input within f32 relative tolerance
      for (let i = 0; i < COUNT; i++) {
        const o = store.getOpen(i);
        const h = store.getHigh(i);
        const l = store.getLow(i);
        const c = store.getClose(i);

        // Sub-paisa resolution: max allowed error < 0.05
        assert.ok(
          Math.abs(o - data.open[i]) < 0.05,
          `Open price error exceeds tolerance at basePrice=${basePrice}, idx=${i}: reconstructed=${o}, original=${data.open[i]}`
        );
        assert.ok(
          Math.abs(h - data.high[i]) < 0.05,
          `High price error exceeds tolerance at basePrice=${basePrice}, idx=${i}: reconstructed=${h}, original=${data.high[i]}`
        );
        assert.ok(
          Math.abs(l - data.low[i]) < 0.05,
          `Low price error exceeds tolerance at basePrice=${basePrice}, idx=${i}: reconstructed=${l}, original=${data.low[i]}`
        );
        assert.ok(
          Math.abs(c - data.close[i]) < 0.05,
          `Close price error exceeds tolerance at basePrice=${basePrice}, idx=${i}: reconstructed=${c}, original=${data.close[i]}`
        );

        const bar = store.getBar(i);
        assert.strictEqual(bar.time, data.time[i]);
        assert.strictEqual(bar.volume, data.volume[i]);
      }
    }
  });

  test('block-accelerated getVisibleMinMax matches naive scan on seeded data across all range scales', () => {
    // Naive reference scan
    function naiveScan(store: CandleStore, fromIndex: number, toIndex: number): { min: number; max: number } {
      const start = Math.max(0, Math.floor(fromIndex));
      const end = Math.min(store.count, Math.ceil(toIndex));
      if (start >= end) {
        const idx = Math.min(Math.max(0, start), store.count - 1);
        return { min: store.getLow(idx), max: store.getHigh(idx) };
      }
      let min = Infinity;
      let max = -Infinity;
      for (let i = start; i < end; i++) {
        if ((store.flags[i] & 1) !== 0) continue;
        const low = store.getLow(i);
        const high = store.getHigh(i);
        if (low < min) min = low;
        if (high > max) max = high;
      }
      return { min, max };
    }

    const COUNT = 10_000;
    const store = new CandleStore(COUNT);
    const data = generateSyntheticCandles({ count: COUNT, seed: 777 });
    store.setData(data);

    // Test cases:
    // 1. Sub-block range (< 256 bars)
    // 2. Multi-block range spanning full blocks
    // 3. Range spanning the entire dataset
    // 4. Random ranges with various alignments
    const testRanges = [
      { from: 0, to: COUNT, desc: 'Full dataset range' },
      { from: 5, to: 45, desc: 'Tiny sub-block range inside block 0' },
      { from: 250, to: 260, desc: 'Range crossing block boundary (255/256)' },
      { from: 100, to: 2000, desc: 'Multi-block range with head and tail partials' },
      { from: 512, to: 1024, desc: 'Exact multiple of block size (aligned)' },
      { from: 513, to: 1023, desc: 'Unaligned multi-block range' },
    ];

    // Add 50 pseudo-random ranges
    let seed = 42;
    const rng = () => {
      seed = (seed * 16807) % 2147483647;
      return (seed - 1) / 2147483646;
    };
    for (let r = 0; r < 50; r++) {
      const idxA = Math.floor(rng() * COUNT);
      const idxB = Math.floor(rng() * COUNT);
      const from = Math.min(idxA, idxB);
      const to = Math.max(idxA, idxB) + 1;
      testRanges.push({ from, to, desc: `Random range [${from}, ${to})` });
    }

    for (const r of testRanges) {
      const naive = naiveScan(store, r.from, r.to);
      const fast = store.getVisibleMinMax(r.from, r.to);

      assert.strictEqual(
        fast.min,
        naive.min,
        `Min mismatch for ${r.desc}: fast=${fast.min}, naive=${naive.min}`
      );
      assert.strictEqual(
        fast.max,
        naive.max,
        `Max mismatch for ${r.desc}: fast=${fast.max}, naive=${naive.max}`
      );
    }

    // Verify after append
    store.append({
      time: store.time[COUNT - 1] + 60_000,
      open: 100,
      high: 99999, // new all-time high
      low: 0.01,  // new all-time low
      close: 100,
      volume: 100,
    });
    const afterAppend = store.getVisibleMinMax(0, store.count);
    assert.ok(Math.abs(afterAppend.max - 99999) < 0.05);
    assert.ok(Math.abs(afterAppend.min - 0.01) < 0.05);

    // Verify after updateLast
    store.updateLast({
      time: store.time[store.count - 1],
      open: 100,
      high: 88888, // updated downward
      low: 0.5,
      close: 100,
      volume: 100,
    });
    const afterUpdate = store.getVisibleMinMax(0, store.count);
    assert.ok(Math.abs(afterUpdate.max - 88888) < 0.05);
    assert.ok(Math.abs(afterUpdate.min - 0.5) < 0.05);
  });
});
