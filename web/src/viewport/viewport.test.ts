import { test, describe } from 'node:test';
import assert from 'node:assert';
import { Viewport } from './viewport.js';

describe('Viewport transforms and interaction logic', () => {
  test('coordinate transforms map forward and backward identically', () => {
    const vp = new Viewport();
    vp.setSize(1000, 500, 1);
    vp.setTotalBars(500);
    vp.fromIndex = 100;
    vp.barsVisible = 50;
    vp.yMin = 1000;
    vp.yMax = 2000;

    const testBar = 125.5;
    const screenX = vp.barIndexToScreenPx(testBar);
    assert.strictEqual(vp.screenPxToBarIndex(screenX), testBar);

    const testPrice = 1450.25;
    const screenY = vp.priceToScreenPx(testPrice);
    assert.strictEqual(Math.round(vp.screenPxToPrice(screenY) * 100) / 100, testPrice);
  });

  test('zoom anchors at the cursor position', () => {
    const vp = new Viewport();
    vp.setSize(1000, 500, 1);
    vp.setTotalBars(1000);
    vp.fromIndex = 100;
    vp.barsVisible = 100;

    const cursorPx = 400;
    const barAtCursorBefore = vp.screenPxToBarIndex(cursorPx);

    // Zoom in (fewer bars visible)
    vp.zoomAt(cursorPx, 0.8);

    const barAtCursorAfter = vp.screenPxToBarIndex(cursorPx);
    assert.ok(
      Math.abs(barAtCursorBefore - barAtCursorAfter) < 1e-6,
      `Bar at cursor should stay fixed during zoom: before=${barAtCursorBefore}, after=${barAtCursorAfter}`
    );
  });

  test('clamping enforces minBarsVisible and right-padding limits', () => {
    const vp = new Viewport({ minBarsVisible: 10, rightPaddingRatio: 0.5 });
    vp.setSize(1000, 500, 1);
    vp.setTotalBars(100);

    // Try zooming way in beyond minBarsVisible
    vp.zoomAt(500, 0.0001);
    assert.strictEqual(vp.barsVisible, 10);

    // Try panning past right padding
    vp.panByPixels(-100000);
    const maxAllowed = 100 - 10 + 10 * 0.5; // totalBars - barsVisible + padding
    assert.ok(vp.fromIndex <= maxAllowed + 1e-6);
  });

  test('updateAutoY applies padding ratio', () => {
    const vp = new Viewport({ yPaddingRatio: 0.05 });
    vp.updateAutoY(100, 200, true);

    const expectedSpan = 100;
    const expectedPad = 5;
    assert.strictEqual(vp.yMin, 100 - expectedPad);
    assert.strictEqual(vp.yMax, 200 + expectedPad);
  });
});
