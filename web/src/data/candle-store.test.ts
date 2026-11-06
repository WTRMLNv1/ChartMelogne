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

  test('setData correctly populates store and packed gpuBuffer', () => {
    const store = new CandleStore(100);
    const data = generateSyntheticCandles({ count: 500, startPrice: 100 });
    store.setData(data);

    assert.strictEqual(store.count, 500);
    assert.ok(store.capacity >= 500);
    assert.strictEqual(store.basePrice, data.close[0]);

    // Check packed GPU buffer
    const base = store.basePrice;
    assert.strictEqual(store.gpuBuffer[0], 0); // x
    assert.strictEqual(Math.fround(store.gpuBuffer[1]), Math.fround(data.open[0] - base));
    assert.strictEqual(Math.fround(store.gpuBuffer[2]), Math.fround(data.high[0] - base));
    assert.strictEqual(Math.fround(store.gpuBuffer[3]), Math.fround(data.low[0] - base));
    assert.strictEqual(Math.fround(store.gpuBuffer[4]), Math.fround(data.close[0] - base));

    // Check last element packed
    const lastIdx = 499;
    const offset = lastIdx * 5;
    assert.strictEqual(store.gpuBuffer[offset], lastIdx);
    assert.strictEqual(Math.fround(store.gpuBuffer[offset + 1]), Math.fround(data.open[lastIdx] - base));
  });

  test('append and updateLast mutate store and packed buffer in O(1)', () => {
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
    assert.strictEqual(store.gpuBuffer[0], 0);

    store.updateLast({
      time: 1000,
      open: 100,
      high: 108,
      low: 95,
      close: 107,
      volume: 15,
    });
    assert.strictEqual(store.count, 1);
    assert.strictEqual(store.high[0], 108);
    assert.strictEqual(store.close[0], 107);
    assert.strictEqual(Math.fround(store.gpuBuffer[2]), Math.fround(108 - 102));

    store.append({
      time: 2000,
      open: 107,
      high: 110,
      low: 106,
      close: 109,
      volume: 20,
    });
    assert.strictEqual(store.count, 2);
    assert.strictEqual(store.gpuBuffer[5], 1); // x = 1
  });

  test('getVisibleMinMax returns bounds for visible range', () => {
    const store = new CandleStore(10);
    const data = generateSyntheticCandles({ count: 50 });
    store.setData(data);

    const { min, max } = store.getVisibleMinMax(10, 20);
    let expectedMin = Infinity;
    let expectedMax = -Infinity;
    for (let i = 10; i <= 20; i++) {
      if (data.low[i] < expectedMin) expectedMin = data.low[i];
      if (data.high[i] > expectedMax) expectedMax = data.high[i];
    }
    assert.strictEqual(min, expectedMin);
    assert.strictEqual(max, expectedMax);
  });
});
