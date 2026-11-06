import { test, describe } from 'node:test';
import assert from 'node:assert';
import { naiveDecimate } from './naive-decimator.js';
import { generateSyntheticCandles } from './synthetic.js';

describe('Naive Reference Decimator', () => {
  test('decimates at k=0 (identity)', () => {
    const data = generateSyntheticCandles({ count: 16, seed: 99 });
    const decimated = naiveDecimate(data, 16, 0);

    assert.strictEqual(decimated.length, 16);
    for (let i = 0; i < 16; i++) {
      assert.strictEqual(decimated[i].open, data.open[i]);
      assert.strictEqual(decimated[i].high, data.high[i]);
      assert.strictEqual(decimated[i].low, data.low[i]);
      assert.strictEqual(decimated[i].close, data.close[i]);
      assert.strictEqual(decimated[i].volume, data.volume[i]);
    }
  });

  test('decimates at k=2 (bucketSize 4)', () => {
    const data = generateSyntheticCandles({ count: 16, seed: 99 });
    const decimated = naiveDecimate(data, 16, 2);

    assert.strictEqual(decimated.length, 4);

    // Bucket 0 covers bars 0..3
    assert.strictEqual(decimated[0].open, data.open[0]);
    assert.strictEqual(decimated[0].close, data.close[3]);

    const expectedHigh = Math.max(data.high[0], data.high[1], data.high[2], data.high[3]);
    const expectedLow = Math.min(data.low[0], data.low[1], data.low[2], data.low[3]);
    const expectedVol = data.volume[0] + data.volume[1] + data.volume[2] + data.volume[3];

    assert.strictEqual(decimated[0].high, expectedHigh);
    assert.strictEqual(decimated[0].low, expectedLow);
    assert.strictEqual(decimated[0].volume, expectedVol);
  });

  test('handles partial trailing buckets correctly', () => {
    const data = generateSyntheticCandles({ count: 7, seed: 101 });
    const decimated = naiveDecimate(data, 7, 2); // bucketSize 4: bucket 0 has 4, bucket 1 has 3

    assert.strictEqual(decimated.length, 2);
    assert.strictEqual(decimated[1].rawIndex, 4);
    assert.strictEqual(decimated[1].open, data.open[4]);
    assert.strictEqual(decimated[1].close, data.close[6]);
    assert.strictEqual(decimated[1].count, 3);
  });
});
