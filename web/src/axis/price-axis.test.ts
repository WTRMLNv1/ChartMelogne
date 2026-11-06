import { test, describe } from 'node:test';
import assert from 'node:assert';
import { generatePriceTicks, getNicePriceStep } from './price-axis.js';

describe('Price Tick Generation (Section 8.1 & 12.1)', () => {
  test('nice steps follow {1, 2, 5} x 10^k pattern', () => {
    assert.strictEqual(getNicePriceStep(0.0008), 0.001);
    assert.strictEqual(getNicePriceStep(0.0014), 0.001);
    assert.strictEqual(getNicePriceStep(0.0028), 0.002);
    assert.strictEqual(getNicePriceStep(0.006), 0.005);
    assert.strictEqual(getNicePriceStep(1.2), 1);
    assert.strictEqual(getNicePriceStep(3.1), 2);
    assert.strictEqual(getNicePriceStep(4.8), 5);
    assert.strictEqual(getNicePriceStep(8.9), 10);
    assert.strictEqual(getNicePriceStep(23), 20);
    assert.strictEqual(getNicePriceStep(78), 100);
  });

  test('generates monotonic, evenly-spaced ticks for ranges from 1e-4 to 1e7', () => {
    const testRanges = [
      { min: 0.0001, max: 0.0008, height: 400 },
      { min: 1.12, max: 1.19, height: 500 },
      { min: 450, max: 550, height: 600 },
      { min: 18500, max: 19200, height: 700 },
      { min: 1_000_000, max: 8_000_000, height: 800 },
    ];

    for (const { min, max, height } of testRanges) {
      const ticks = generatePriceTicks(min, max, height, { targetSpacingPx: 50 });
      assert.ok(ticks.length >= 2, `Expected at least 2 ticks for [${min}, ${max}], got ${ticks.length}`);

      // Count within reasonable bounds
      const expectedCount = Math.floor(height / 50);
      assert.ok(
        ticks.length <= expectedCount * 2 + 2,
        `Tick count ${ticks.length} out of bounds for height ${height}`
      );

      // Check strictly monotonic & evenly spaced
      let prev = ticks[0].price;
      const step = ticks.length > 1 ? ticks[1].price - ticks[0].price : 0;
      assert.ok(step > 0, 'Step must be positive');

      for (let i = 1; i < ticks.length; i++) {
        const cur = ticks[i].price;
        assert.ok(cur > prev, `Ticks must be strictly increasing: ${cur} > ${prev}`);
        const currentDiff = cur - prev;
        assert.ok(
          Math.abs(currentDiff - step) < step * 1e-4,
          `Ticks must be evenly spaced: diff=${currentDiff}, step=${step}`
        );
        prev = cur;
      }
    }
  });

  test('formats decimals cleanly based on step size', () => {
    const ticks = generatePriceTicks(10.1, 10.5, 400);
    for (const t of ticks) {
      assert.strictEqual(Number(t.label), t.price);
    }
  });
});
