export interface TimeTick {
  barIndex: number;
  timeMs: number;
  label: string;
  isMajor: boolean; // e.g. day or month start
}

export interface SessionCalendarHook {
  isDayStart?: (index: number) => boolean;
  label?: (index: number, level: TimeZoomLevel) => string;
}

export type TimeZoomLevel = 'intraday' | 'daily' | 'monthly';

export interface TimeAxisOptions {
  utcOffsetMinutes?: number; // e.g. 330 for IST
  minTickSpacingPx?: number; // e.g. 80px
  calendar?: SessionCalendarHook;
}

const MONTH_NAMES = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

// Precompute all 1440 "HH:mm" strings for zero-allocation intraday lookups
const MINUTE_STRINGS: string[] = new Array(1440);
for (let h = 0; h < 24; h++) {
  const hh = h.toString().padStart(2, '0');
  for (let m = 0; m < 60; m++) {
    const mm = m.toString().padStart(2, '0');
    MINUTE_STRINGS[h * 60 + m] = `${hh}:${mm}`;
  }
}

// Precompute all 31 * 12 "dd MMM" strings
const DAY_MONTH_STRINGS: string[][] = [];
for (let d = 0; d <= 31; d++) {
  DAY_MONTH_STRINGS[d] = [];
  for (let m = 0; m < 12; m++) {
    DAY_MONTH_STRINGS[d][m] = `${d} ${MONTH_NAMES[m]}`;
  }
}

// Cache for "MMM yyyy" and "MMM 'yy" strings
const MONTH_YEAR_CACHE = new Map<number, string>();
const MONTH_SHORT_YEAR_CACHE = new Map<number, string>();

function getMonthYearString(year: number, month: number): string {
  const key = year * 100 + month;
  let str = MONTH_YEAR_CACHE.get(key);
  if (!str) {
    str = `${MONTH_NAMES[month]} ${year}`;
    MONTH_YEAR_CACHE.set(key, str);
  }
  return str;
}

function getDailyMonthStartString(year: number, month: number): string {
  const key = year * 100 + month;
  let str = MONTH_SHORT_YEAR_CACHE.get(key);
  if (!str) {
    const yy = year.toString().slice(-2);
    str = `${MONTH_NAMES[month]} '${yy}`;
    MONTH_SHORT_YEAR_CACHE.set(key, str);
  }
  return str;
}

/**
 * Converts a Unix day number (days since 1970-01-01) to civil year, month (0..11), day (1..31).
 * Algorithm by Howard Hinnant (public domain). Pure integer math with zero allocations or Date objects.
 */
export function civilFromDays(z: number): { year: number; month: number; day: number } {
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

export function binarySearchDayStarts(dayStarts: Uint32Array, dayCount: number, targetBar: number): number {
  let low = 0;
  let high = dayCount - 1;
  let ans = dayCount;
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

export class TimeAxis {
  readonly utcOffsetMinutes: number;
  readonly utcOffsetMs: number;
  readonly minTickSpacingPx: number;
  readonly calendar?: SessionCalendarHook;

  // Reusable tick pool to eliminate per-frame allocations
  private tickPool: TimeTick[] = [];
  private activeTicks: TimeTick[] = [];

  constructor(options: TimeAxisOptions = {}) {
    this.utcOffsetMinutes = options.utcOffsetMinutes ?? 330; // default IST (UTC+5:30)
    this.utcOffsetMs = this.utcOffsetMinutes * 60_000;
    this.minTickSpacingPx = options.minTickSpacingPx ?? 80;
    this.calendar = options.calendar;
  }

  getLocalParts(timeMs: number): {
    year: number;
    month: number;
    day: number;
    hours: number;
    minutes: number;
  } {
    const localMs = timeMs + this.utcOffsetMs;
    const dayNumber = Math.floor(localMs / 86_400_000);
    const civil = civilFromDays(dayNumber);
    const msInDay = ((localMs % 86_400_000) + 86_400_000) % 86_400_000;
    const hours = Math.floor(msInDay / 3_600_000);
    const minutes = Math.floor((msInDay % 3_600_000) / 60_000);

    return {
      year: civil.year,
      month: civil.month,
      day: civil.day,
      hours,
      minutes,
    };
  }

  isDayBoundary(timeArray: Float64Array, i: number, dayStarts?: Uint32Array, dayCount?: number): boolean {
    if (this.calendar?.isDayStart) {
      return this.calendar.isDayStart(i);
    }
    if (i <= 0) return true;
    if (dayStarts && dayCount !== undefined) {
      const idx = binarySearchDayStarts(dayStarts, dayCount, i);
      return idx < dayCount && dayStarts[idx] === i;
    }
    const prevDay = Math.floor((timeArray[i - 1] + this.utcOffsetMs) / 86_400_000);
    const currDay = Math.floor((timeArray[i] + this.utcOffsetMs) / 86_400_000);
    return prevDay !== currDay;
  }

  isMonthBoundary(timeArray: Float64Array, i: number): boolean {
    if (i <= 0) return true;
    const prevDay = Math.floor((timeArray[i - 1] + this.utcOffsetMs) / 86_400_000);
    const currDay = Math.floor((timeArray[i] + this.utcOffsetMs) / 86_400_000);
    const prevCivil = civilFromDays(prevDay);
    const currCivil = civilFromDays(currDay);
    return prevCivil.year !== currCivil.year || prevCivil.month !== currCivil.month;
  }

  formatLabel(timeArray: Float64Array, i: number, level: TimeZoomLevel): string {
    if (this.calendar?.label) {
      return this.calendar.label(i, level);
    }

    const localMs = timeArray[i] + this.utcOffsetMs;
    const dayNumber = Math.floor(localMs / 86_400_000);
    const civil = civilFromDays(dayNumber);

    const isDayStart = this.isDayBoundary(timeArray, i);
    const isMonthStart = this.isMonthBoundary(timeArray, i);

    switch (level) {
      case 'intraday': {
        if (isDayStart) {
          return DAY_MONTH_STRINGS[civil.day]?.[civil.month] ?? `${civil.day} ${MONTH_NAMES[civil.month]}`;
        }
        const msInDay = ((localMs % 86_400_000) + 86_400_000) % 86_400_000;
        const minuteIndex = Math.floor(msInDay / 60_000);
        return MINUTE_STRINGS[minuteIndex] ?? '00:00';
      }

      case 'daily': {
        if (isMonthStart) {
          return getDailyMonthStartString(civil.year, civil.month);
        }
        return DAY_MONTH_STRINGS[civil.day]?.[civil.month] ?? `${civil.day} ${MONTH_NAMES[civil.month]}`;
      }

      case 'monthly':
        return getMonthYearString(civil.year, civil.month);
    }
  }

  generateTicks(
    timeArray: Float64Array,
    count: number,
    fromIndex: number,
    barsVisible: number,
    viewportWidthPx: number,
    dayStarts?: Uint32Array,
    dayCount?: number
  ): TimeTick[] {
    if (count === 0 || viewportWidthPx <= 0 || barsVisible <= 0) return [];

    const startIdx = Math.max(0, Math.floor(fromIndex));
    const endIdx = Math.min(count - 1, Math.ceil(fromIndex + barsVisible));
    if (startIdx >= endIdx) return [];

    const pxPerBar = viewportWidthPx / barsVisible;
    const visibleTimeSpanMs = timeArray[endIdx] - timeArray[startIdx];

    const ONE_DAY_MS = 24 * 3600_000;
    const SIXTY_DAYS_MS = 60 * ONE_DAY_MS;

    let level: TimeZoomLevel = 'intraday';
    if (visibleTimeSpanMs >= SIXTY_DAYS_MS) {
      level = 'monthly';
    } else if (visibleTimeSpanMs >= ONE_DAY_MS) {
      level = 'daily';
    }

    const minBarsBetweenTicks = Math.max(1, Math.ceil(this.minTickSpacingPx / pxPerBar));
    const minStepThreshold = minBarsBetweenTicks * 0.7;

    // Helper to add ticks to reusable pool without per-frame allocations
    let tickCount = 0;
    const pool = this.tickPool;
    const addTick = (barIndex: number, timeMs: number, label: string, isMajor: boolean) => {
      if (tickCount >= pool.length) {
        pool.push({ barIndex, timeMs, label, isMajor });
      } else {
        const t = pool[tickCount];
        t.barIndex = barIndex;
        t.timeMs = timeMs;
        t.label = label;
        t.isMajor = isMajor;
      }
      tickCount++;
    };

    const utcOffsetMs = this.utcOffsetMs;

    // Ensure we have dayStarts; if not provided by store, build locally
    let dStarts: Uint32Array;
    let dCount: number;

    if (dayStarts && dayCount !== undefined && dayCount > 0) {
      dStarts = dayStarts;
      dCount = dayCount;
    } else {
      // Fast fallback O(N) integer pass
      let cap = Math.max(32, Math.ceil(count / 100));
      dStarts = new Uint32Array(cap);
      let d = 0;
      let prevD = -Infinity;
      for (let i = 0; i < count; i++) {
        const localD = Math.floor((timeArray[i] + utcOffsetMs) / 86_400_000);
        if (localD !== prevD) {
          if (d >= dStarts.length) {
            const next = new Uint32Array(dStarts.length * 2);
            next.set(dStarts.subarray(0, d));
            dStarts = next;
          }
          dStarts[d++] = i;
          prevD = localD;
        }
      }
      dCount = d;
    }

    // Binary search dayStarts to locate the visible day index range in O(log days)
    const firstDayIdx = Math.max(0, binarySearchDayStarts(dStarts, dCount, startIdx));
    let lastDayIdx = binarySearchDayStarts(dStarts, dCount, endIdx);
    if (lastDayIdx < dCount && dStarts[lastDayIdx] <= endIdx) {
      // include
    } else {
      lastDayIdx = Math.min(dCount - 1, lastDayIdx);
    }

    if (level === 'monthly') {
      // O(visible days) iteration - at full zoom-out, only ~1200 iterations
      let lastTickBar = -Infinity;
      let prevMonth = -1;

      // Seed previous month before firstDayIdx if possible
      if (firstDayIdx > 0) {
        const prevBar = dStarts[firstDayIdx - 1];
        const prevDayNum = Math.floor((timeArray[prevBar] + utcOffsetMs) / 86_400_000);
        prevMonth = civilFromDays(prevDayNum).month;
      }

      for (let d = firstDayIdx; d <= lastDayIdx; d++) {
        const barIdx = dStarts[d];
        if (barIdx < startIdx || barIdx > endIdx) continue;

        const dayNum = Math.floor((timeArray[barIdx] + utcOffsetMs) / 86_400_000);
        const civil = civilFromDays(dayNum);

        const isMonthStart = civil.month !== prevMonth;
        prevMonth = civil.month;

        if (isMonthStart && barIdx - lastTickBar >= minStepThreshold) {
          const label = this.calendar?.label
            ? this.calendar.label(barIdx, 'monthly')
            : getMonthYearString(civil.year, civil.month);

          addTick(barIdx, timeArray[barIdx], label, true);
          lastTickBar = barIdx;
        }
      }
    } else if (level === 'daily') {
      // O(visible days) iteration
      let lastTickBar = -Infinity;
      let prevMonth = -1;

      if (firstDayIdx > 0) {
        const prevBar = dStarts[firstDayIdx - 1];
        const prevDayNum = Math.floor((timeArray[prevBar] + utcOffsetMs) / 86_400_000);
        prevMonth = civilFromDays(prevDayNum).month;
      }

      for (let d = firstDayIdx; d <= lastDayIdx; d++) {
        const barIdx = dStarts[d];
        if (barIdx < startIdx || barIdx > endIdx) continue;

        const dayNum = Math.floor((timeArray[barIdx] + utcOffsetMs) / 86_400_000);
        const civil = civilFromDays(dayNum);
        const isMonthStart = civil.month !== prevMonth;
        prevMonth = civil.month;

        if (barIdx - lastTickBar >= minStepThreshold) {
          let label: string;
          if (this.calendar?.label) {
            label = this.calendar.label(barIdx, 'daily');
          } else if (isMonthStart) {
            label = getDailyMonthStartString(civil.year, civil.month);
          } else {
            label = DAY_MONTH_STRINGS[civil.day]?.[civil.month] ?? `${civil.day} ${MONTH_NAMES[civil.month]}`;
          }

          addTick(barIdx, timeArray[barIdx], label, isMonthStart);
          lastTickBar = barIdx;
        }
      }
    } else {
      // Intraday: Sample only on pixel-spaced grid of bars (step = minBarsBetweenTicks)
      // O(pixels) iteration - at most 10-25 samples!
      let lastTickBar = -Infinity;

      // Find any day starts that fall within visible range
      let nextDayStartIdx = firstDayIdx;

      for (let i = startIdx; i <= endIdx; i += minBarsBetweenTicks) {
        // Check if there is an intervening day boundary
        let isDayStart = false;
        if (nextDayStartIdx < dCount && dStarts[nextDayStartIdx] <= i) {
          if (dStarts[nextDayStartIdx] >= startIdx) {
            isDayStart = true;
          }
          nextDayStartIdx++;
        }

        const localMs = timeArray[i] + utcOffsetMs;
        const dayNumber = Math.floor(localMs / 86_400_000);
        const civil = civilFromDays(dayNumber);

        let label: string;
        if (this.calendar?.label) {
          label = this.calendar.label(i, 'intraday');
        } else if (isDayStart || i === startIdx && firstDayIdx < dCount && dStarts[firstDayIdx] === i) {
          label = DAY_MONTH_STRINGS[civil.day]?.[civil.month] ?? `${civil.day} ${MONTH_NAMES[civil.month]}`;
        } else {
          const msInDay = ((localMs % 86_400_000) + 86_400_000) % 86_400_000;
          const minuteIndex = Math.floor(msInDay / 60_000);
          label = MINUTE_STRINGS[minuteIndex] ?? '00:00';
        }

        addTick(i, timeArray[i], label, isDayStart);
        lastTickBar = i;
      }
    }

    // Reuse activeTicks array
    this.activeTicks.length = tickCount;
    for (let i = 0; i < tickCount; i++) {
      this.activeTicks[i] = pool[i];
    }

    return this.activeTicks;
  }
}
