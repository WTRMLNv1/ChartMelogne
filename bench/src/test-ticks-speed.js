// Benchmark comparing generateTicks BEFORE (O(N) with Date) vs AFTER (O(days) with civilFromDays)
import { performance } from 'perf_hooks';

// Fast Mulberry32 PRNG
function createRng(seed = 42) {
  let s = seed | 0;
  return function next() {
    s = (s + 0x6d2b79f5) | 0;
    let t = Math.imul(s ^ (s >>> 15), 1 | s);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function generateSyntheticCandles(count = 450_000, seed = 1337) {
  const rng = createRng(seed);
  const time = new Float64Array(count);
  let currentPrice = 1000.0;
  let currentTime = Date.UTC(2020, 0, 1, 3, 45, 0); // 09:15 IST
  for (let i = 0; i < count; i++) {
    time[i] = currentTime;
    currentTime += 60_000;
  }
  return time;
}

const MONTH_NAMES = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

// === 1. BEFORE: Legacy generateTicks (O(N) scanning 450k bars, calling getLocalParts with new Date()) ===
function generateTicksBefore(timeArray, count, fromIndex, barsVisible, viewportWidthPx, utcOffsetMinutes = 330) {
  const getLocalParts = (t) => {
    const d = new Date(t + utcOffsetMinutes * 60_000);
    return {
      year: d.getUTCFullYear(),
      month: d.getUTCMonth(),
      day: d.getUTCDate(),
      hours: d.getUTCHours(),
      minutes: d.getUTCMinutes(),
    };
  };
  const isDayBoundary = (i) => {
    if (i <= 0) return true;
    const p = getLocalParts(timeArray[i - 1]);
    const c = getLocalParts(timeArray[i]);
    return p.year !== c.year || p.month !== c.month || p.day !== c.day;
  };
  const isMonthBoundary = (i) => {
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

// === 2. AFTER: Optimized generateTicks (O(days), civilFromDays, binary search, cached strings, zero allocation) ===
function civilFromDays(z) {
  z += 719468;
  const era = Math.floor((z >= 0 ? z : z - 146096) / 146097);
  const doe = z - era * 146097;
  const yoe = Math.floor((doe - Math.floor(doe / 1460) + Math.floor(doe / 36524) - Math.floor(doe / 146096)) / 365);
  const y = yoe + era * 400;
  const doy = doe - (365 * yoe + Math.floor(yoe / 4) - Math.floor(yoe / 100));
  const mp = Math.floor((5 * doy + 2) / 153);
  const d = doy - Math.floor((153 * mp + 2) / 5) + 1;
  const m = mp < 10 ? mp + 3 : mp - 9;
  return { year: y + (m <= 2 ? 1 : 0), month: m - 1, day: d };
}

function binarySearchDayStarts(dayStarts, dayCount, targetBar) {
  let low = 0, high = dayCount - 1, ans = dayCount;
  while (low <= high) {
    const mid = (low + high) >> 1;
    if (dayStarts[mid] >= targetBar) {
      ans = mid;
      high = mid - 1;
    } else {
      low = mid + 1;
    }
  }
  return ans;
}

// Cache structures
const MINUTE_STRINGS = new Array(1440);
for (let h = 0; h < 24; h++) {
  const hh = h.toString().padStart(2, '0');
  for (let m = 0; m < 60; m++) {
    MINUTE_STRINGS[h * 60 + m] = `${hh}:${m.toString().padStart(2, '0')}`;
  }
}

const DAY_MONTH_STRINGS = [];
for (let d = 0; d <= 31; d++) {
  DAY_MONTH_STRINGS[d] = [];
  for (let m = 0; m < 12; m++) {
    DAY_MONTH_STRINGS[d][m] = `${d} ${MONTH_NAMES[m]}`;
  }
}

const MONTH_YEAR_CACHE = new Map();
function getMonthYearString(year, month) {
  const key = year * 100 + month;
  let str = MONTH_YEAR_CACHE.get(key);
  if (!str) {
    str = `${MONTH_NAMES[month]} ${year}`;
    MONTH_YEAR_CACHE.set(key, str);
  }
  return str;
}

function getDailyMonthStartString(year, month) {
  const yy = year.toString().slice(-2);
  return `${MONTH_NAMES[month]} '${yy}`;
}

const tickPool = [];
const activeTicks = [];

function generateTicksAfter(timeArray, count, fromIndex, barsVisible, viewportWidthPx, dayStarts, dayCount, utcOffsetMinutes = 330) {
  const startIdx = Math.max(0, Math.floor(fromIndex));
  const endIdx = Math.min(count - 1, Math.ceil(fromIndex + barsVisible));
  if (startIdx >= endIdx) return [];

  const pxPerBar = viewportWidthPx / barsVisible;
  const utcOffsetMs = utcOffsetMinutes * 60_000;
  const visibleTimeSpanMs = timeArray[endIdx] - timeArray[startIdx];

  const ONE_DAY_MS = 24 * 3600_000;
  const SIXTY_DAYS_MS = 60 * ONE_DAY_MS;

  let level = 'intraday';
  if (visibleTimeSpanMs >= SIXTY_DAYS_MS) {
    level = 'monthly';
  } else if (visibleTimeSpanMs >= ONE_DAY_MS) {
    level = 'daily';
  }

  const minBarsBetweenTicks = Math.max(1, Math.ceil(80 / pxPerBar));
  const minStepThreshold = minBarsBetweenTicks * 0.7;

  let tickCount = 0;
  const addTick = (barIndex, label, isMajor) => {
    if (tickCount >= tickPool.length) {
      tickPool.push({ barIndex, label, isMajor });
    } else {
      const t = tickPool[tickCount];
      t.barIndex = barIndex;
      t.label = label;
      t.isMajor = isMajor;
    }
    tickCount++;
  };

  const firstDayIdx = Math.max(0, binarySearchDayStarts(dayStarts, dayCount, startIdx));
  let lastDayIdx = binarySearchDayStarts(dayStarts, dayCount, endIdx);
  if (lastDayIdx < dayCount && dayStarts[lastDayIdx] <= endIdx) {
  } else {
    lastDayIdx = Math.min(dayCount - 1, lastDayIdx);
  }

  if (level === 'monthly') {
    let lastTickBar = -Infinity;
    let prevMonth = -1;
    if (firstDayIdx > 0) {
      const prevBar = dayStarts[firstDayIdx - 1];
      const prevDayNum = Math.floor((timeArray[prevBar] + utcOffsetMs) / 86_400_000);
      prevMonth = civilFromDays(prevDayNum).month;
    }

    for (let d = firstDayIdx; d <= lastDayIdx; d++) {
      const barIdx = dayStarts[d];
      if (barIdx < startIdx || barIdx > endIdx) continue;

      const dayNum = Math.floor((timeArray[barIdx] + utcOffsetMs) / 86_400_000);
      const civil = civilFromDays(dayNum);
      const isMonthStart = civil.month !== prevMonth;
      prevMonth = civil.month;

      if (isMonthStart && barIdx - lastTickBar >= minStepThreshold) {
        addTick(barIdx, getMonthYearString(civil.year, civil.month), true);
        lastTickBar = barIdx;
      }
    }
  } else if (level === 'daily') {
    let lastTickBar = -Infinity;
    let prevMonth = -1;
    if (firstDayIdx > 0) {
      const prevBar = dayStarts[firstDayIdx - 1];
      const prevDayNum = Math.floor((timeArray[prevBar] + utcOffsetMs) / 86_400_000);
      prevMonth = civilFromDays(prevDayNum).month;
    }

    for (let d = firstDayIdx; d <= lastDayIdx; d++) {
      const barIdx = dayStarts[d];
      if (barIdx < startIdx || barIdx > endIdx) continue;

      const dayNum = Math.floor((timeArray[barIdx] + utcOffsetMs) / 86_400_000);
      const civil = civilFromDays(dayNum);
      const isMonthStart = civil.month !== prevMonth;
      prevMonth = civil.month;

      if (barIdx - lastTickBar >= minStepThreshold) {
        const label = isMonthStart
          ? getDailyMonthStartString(civil.year, civil.month)
          : (DAY_MONTH_STRINGS[civil.day]?.[civil.month] ?? `${civil.day} ${MONTH_NAMES[civil.month]}`);
        addTick(barIdx, label, isMonthStart);
        lastTickBar = barIdx;
      }
    }
  } else {
    for (let i = startIdx; i <= endIdx; i += minBarsBetweenTicks) {
      const localMs = timeArray[i] + utcOffsetMs;
      const msInDay = ((localMs % 86_400_000) + 86_400_000) % 86_400_000;
      const minuteIndex = Math.floor(msInDay / 60_000);
      addTick(i, MINUTE_STRINGS[minuteIndex] ?? '00:00', false);
    }
  }

  activeTicks.length = tickCount;
  for (let i = 0; i < tickCount; i++) {
    activeTicks[i] = tickPool[i];
  }
  return activeTicks;
}

// === RUN BENCHMARK ===
console.log('Generating 450,000 synthetic bars...');
const BARS = 450_000;
const times = generateSyntheticCandles(BARS, 1337);

// Build dayStarts
const utcOffsetMs = 330 * 60_000;
let dayCap = Math.max(128, Math.ceil(BARS / 100));
const dayStarts = new Uint32Array(dayCap);
let days = 0;
let prevDay = -Infinity;
for (let i = 0; i < BARS; i++) {
  const localDay = Math.floor((times[i] + utcOffsetMs) / 86_400_000);
  if (localDay !== prevDay) {
    dayStarts[days++] = i;
    prevDay = localDay;
  }
}

// 1. Correctness Verification
console.log('\n--- Verifying Correctness & Output Parity ---');
const ticksBefore = generateTicksBefore(times, BARS, 0, BARS, 1920, 330);
const ticksAfter = generateTicksAfter(times, BARS, 0, BARS, 1920, dayStarts, days, 330);

if (ticksBefore.length !== ticksAfter.length) {
  console.error(`Count mismatch: before=${ticksBefore.length}, after=${ticksAfter.length}`);
  process.exit(1);
}
for (let i = 0; i < ticksBefore.length; i++) {
  if (ticksBefore[i].barIndex !== ticksAfter[i].barIndex || ticksBefore[i].label !== ticksAfter[i].label) {
    console.error(`Mismatch at tick ${i}: before=`, ticksBefore[i], 'after=', ticksAfter[i]);
    process.exit(1);
  }
}
console.log(`PARITY VERIFIED: All ${ticksAfter.length} ticks have identical indices and labels!`);

// 2. Performance Comparison (Full zoom-out, 450k bars)
console.log('\n--- Performance Benchmark: Full Zoom-out (450,000 bars) ---');

// Warm up
for (let i = 0; i < 3; i++) {
  generateTicksAfter(times, BARS, 0, BARS, 1920, dayStarts, days, 330);
}

// Measure AFTER
const ITERS_AFTER = 1000;
const tAfter0 = performance.now();
for (let i = 0; i < ITERS_AFTER; i++) {
  generateTicksAfter(times, BARS, 0, BARS, 1920, dayStarts, days, 330);
}
const elapsedAfterMs = (performance.now() - tAfter0) / ITERS_AFTER;

// Measure BEFORE (fewer iters since it takes ~230ms per call)
const ITERS_BEFORE = 5;
const tBefore0 = performance.now();
for (let i = 0; i < ITERS_BEFORE; i++) {
  generateTicksBefore(times, BARS, 0, BARS, 1920, 330);
}
const elapsedBeforeMs = (performance.now() - tBefore0) / ITERS_BEFORE;

console.log(`BEFORE (O(N) with Date):        ${elapsedBeforeMs.toFixed(2)} ms / frame`);
console.log(`AFTER (O(days) civilFromDays):  ${(elapsedAfterMs * 1000).toFixed(1)} µs / frame (${elapsedAfterMs.toFixed(3)} ms)`);
console.log(`SPEEDUP:                         ${(elapsedBeforeMs / elapsedAfterMs).toFixed(0)}x faster!`);
console.log(`GATE (< 0.5 ms):                 ${elapsedAfterMs < 0.5 ? 'PASSED' : 'FAILED'}`);
