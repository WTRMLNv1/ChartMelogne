import { test, describe } from 'node:test';
import assert from 'node:assert';
import { TimeAxis } from './time-axis.js';

describe('TimeAxis & Session Calendar (Section 8.2 & 12.1)', () => {
  test('detects day boundaries for 375-bar Indian market sessions (9:15 - 15:30 IST)', () => {
    const timeAxis = new TimeAxis({ utcOffsetMinutes: 330 }); // IST +5:30

    // Construct 2 consecutive trading days: Day 1 (375 bars) and Day 2 (375 bars)
    // Day 1: 2026-01-05 09:15 IST = 03:45 UTC
    const day1StartUtc = Date.UTC(2026, 0, 5, 3, 45, 0);
    // Day 2: 2026-01-06 09:15 IST = 03:45 UTC (19.75 hours gap)
    const day2StartUtc = Date.UTC(2026, 0, 6, 3, 45, 0);

    const times = new Float64Array(750);
    for (let i = 0; i < 375; i++) {
      times[i] = day1StartUtc + i * 60_000;
    }
    for (let i = 0; i < 375; i++) {
      times[375 + i] = day2StartUtc + i * 60_000;
    }

    assert.strictEqual(timeAxis.isDayBoundary(times, 0), true, 'Bar 0 is day start');
    assert.strictEqual(timeAxis.isDayBoundary(times, 1), false, 'Bar 1 is intraday');
    assert.strictEqual(timeAxis.isDayBoundary(times, 374), false, 'Bar 374 is end of day 1');
    assert.strictEqual(timeAxis.isDayBoundary(times, 375), true, 'Bar 375 is start of day 2');
    assert.strictEqual(timeAxis.isDayBoundary(times, 376), false, 'Bar 376 is intraday day 2');
  });

  test('detects day boundaries for 360-bar sessions and partial half-days', () => {
    const timeAxis = new TimeAxis({ utcOffsetMinutes: 330 });

    // Day 1 (360 bars)
    const day1Start = Date.UTC(2026, 2, 10, 3, 45, 0);
    // Day 2 (partial session: 60 bars)
    const day2Start = Date.UTC(2026, 2, 11, 3, 45, 0);

    const times = new Float64Array(420);
    for (let i = 0; i < 360; i++) times[i] = day1Start + i * 60_000;
    for (let i = 0; i < 60; i++) times[360 + i] = day2Start + i * 60_000;

    assert.strictEqual(timeAxis.isDayBoundary(times, 0), true);
    assert.strictEqual(timeAxis.isDayBoundary(times, 359), false);
    assert.strictEqual(timeAxis.isDayBoundary(times, 360), true, 'Partial day start detected');
    assert.strictEqual(timeAxis.isDayBoundary(times, 419), false);
  });

  test('adapts label format to zoom levels', () => {
    const timeAxis = new TimeAxis({ utcOffsetMinutes: 330 });
    const times = new Float64Array([
      Date.UTC(2026, 0, 5, 3, 45, 0), // 09:15 IST (Day start)
      Date.UTC(2026, 0, 5, 4, 30, 0), // 10:00 IST (Intraday)
    ]);

    // Intraday
    assert.strictEqual(timeAxis.formatLabel(times, 0, 'intraday'), '5 Jan');
    assert.strictEqual(timeAxis.formatLabel(times, 1, 'intraday'), '10:00');

    // Daily
    assert.strictEqual(timeAxis.formatLabel(times, 0, 'daily'), "Jan '26");

    // Monthly
    assert.strictEqual(timeAxis.formatLabel(times, 0, 'monthly'), 'Jan 2026');
  });

  test('generates valid ticks across visible range with minimum spacing', () => {
    const timeAxis = new TimeAxis({ utcOffsetMinutes: 330, minTickSpacingPx: 80 });
    const startMs = Date.UTC(2026, 0, 5, 3, 45, 0);
    const count = 375;
    const times = new Float64Array(count);
    for (let i = 0; i < count; i++) times[i] = startMs + i * 60_000;

    const ticks = timeAxis.generateTicks(times, count, 0, 100, 800);
    assert.ok(ticks.length >= 2, `Expected at least 2 ticks, got ${ticks.length}`);

    // Monotonic barIndex
    for (let i = 1; i < ticks.length; i++) {
      assert.ok(ticks[i].barIndex > ticks[i - 1].barIndex);
    }
  });
});
