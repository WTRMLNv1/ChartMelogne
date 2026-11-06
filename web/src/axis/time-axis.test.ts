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

  test('identical output between precomputed dayStarts and legacy scan across 360 and 375 bars/day datasets', () => {
    const MONTH_NAMES = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

    // Legacy O(N) tick generator using Date
    function generateTicksLegacy(
      timeArray: Float64Array,
      count: number,
      fromIndex: number,
      barsVisible: number,
      viewportWidthPx: number,
      utcOffsetMinutes: number = 330
    ) {
      const getLocalParts = (t: number) => {
        const d = new Date(t + utcOffsetMinutes * 60_000);
        return {
          year: d.getUTCFullYear(),
          month: d.getUTCMonth(),
          day: d.getUTCDate(),
          hours: d.getUTCHours(),
          minutes: d.getUTCMinutes(),
        };
      };
      const isDayBoundary = (i: number) => {
        if (i <= 0) return true;
        const p = getLocalParts(timeArray[i - 1]);
        const c = getLocalParts(timeArray[i]);
        return p.year !== c.year || p.month !== c.month || p.day !== c.day;
      };
      const isMonthBoundary = (i: number) => {
        if (i <= 0) return true;
        const p = getLocalParts(timeArray[i - 1]);
        const c = getLocalParts(timeArray[i]);
        return p.year !== c.year || p.month !== c.month;
      };

      const startIdx = Math.max(0, Math.floor(fromIndex));
      const endIdx = Math.min(count - 1, Math.ceil(fromIndex + barsVisible));
      const pxPerBar = viewportWidthPx / barsVisible;
      const visibleTimeSpanMs = timeArray[endIdx] - timeArray[startIdx];
      const level = visibleTimeSpanMs >= 60 * 24 * 3600_000 ? 'monthly' : visibleTimeSpanMs >= 24 * 3600_000 ? 'daily' : 'intraday';
      const minBars = Math.max(1, Math.ceil(80 / pxPerBar));

      const ticks = [];
      let lastTickBar = -Infinity;
      for (let i = startIdx; i <= endIdx; i++) {
        const dayStart = isDayBoundary(i);
        const monthStart = isMonthBoundary(i);
        const shouldConsider = level === 'monthly' ? monthStart : level === 'daily' ? dayStart : dayStart || (i - lastTickBar >= minBars);
        if (shouldConsider && i - lastTickBar >= minBars * 0.7) {
          const { year, month, day, hours, minutes } = getLocalParts(timeArray[i]);
          let label = '';
          if (level === 'intraday') {
            label = dayStart ? `${day} ${MONTH_NAMES[month]}` : `${hours.toString().padStart(2, '0')}:${minutes.toString().padStart(2, '0')}`;
          } else if (level === 'daily') {
            label = monthStart ? `${MONTH_NAMES[month]} '${year.toString().slice(-2)}` : `${day} ${MONTH_NAMES[month]}`;
          } else {
            label = `${MONTH_NAMES[month]} ${year}`;
          }
          ticks.push({ barIndex: i, label });
          lastTickBar = i;
        }
      }
      return ticks;
    }

    const testConfigs = [
      { barsPerDay: 375, days: 120 },
      { barsPerDay: 360, days: 120 },
    ];

    const timeAxis = new TimeAxis({ utcOffsetMinutes: 330 });

    for (const { barsPerDay, days } of testConfigs) {
      const totalBars = barsPerDay * days;
      const times = new Float64Array(totalBars);
      let t = Date.UTC(2025, 0, 1, 3, 45, 0); // 09:15 IST
      let idx = 0;
      for (let d = 0; d < days; d++) {
        for (let b = 0; b < barsPerDay; b++) {
          times[idx++] = t + b * 60_000;
        }
        t += 24 * 3600_000; // next day
      }

      // Test multiple zoom levels: monthly, daily, and intraday
      const zoomCases = [
        { from: 0, visible: totalBars, width: 1920, desc: 'Full zoom-out (monthly)' },
        { from: totalBars - barsPerDay * 15, visible: barsPerDay * 10, width: 1920, desc: '10 days (daily)' },
        { from: totalBars - barsPerDay * 3, visible: barsPerDay * 2, width: 1920, desc: '2 days (daily)' },
        { from: totalBars - 100, visible: 60, width: 1920, desc: '60 bars (intraday)' },
      ];

      for (const z of zoomCases) {
        const legacyTicks = generateTicksLegacy(times, totalBars, z.from, z.visible, z.width, 330);
        const newTicks = timeAxis.generateTicks(times, totalBars, z.from, z.visible, z.width);

        assert.strictEqual(
          newTicks.length,
          legacyTicks.length,
          `Tick count mismatch for ${z.desc} (${barsPerDay} b/d): new=${newTicks.length}, legacy=${legacyTicks.length}`
        );

        for (let k = 0; k < newTicks.length; k++) {
          assert.strictEqual(
            newTicks[k].barIndex,
            legacyTicks[k].barIndex,
            `Bar index mismatch at tick ${k} for ${z.desc}`
          );
          assert.strictEqual(
            newTicks[k].label,
            legacyTicks[k].label,
            `Label text mismatch at tick ${k} for ${z.desc}`
          );
        }
      }
    }
  });
});
