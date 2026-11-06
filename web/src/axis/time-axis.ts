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

export class TimeAxis {
  readonly utcOffsetMinutes: number;
  readonly minTickSpacingPx: number;
  readonly calendar?: SessionCalendarHook;

  constructor(options: TimeAxisOptions = {}) {
    this.utcOffsetMinutes = options.utcOffsetMinutes ?? 330; // default IST (UTC+5:30)
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
    const localMs = timeMs + this.utcOffsetMinutes * 60_000;
    const d = new Date(localMs);
    return {
      year: d.getUTCFullYear(),
      month: d.getUTCMonth(),
      day: d.getUTCDate(),
      hours: d.getUTCHours(),
      minutes: d.getUTCMinutes(),
    };
  }

  isDayBoundary(timeArray: Float64Array, i: number): boolean {
    if (this.calendar?.isDayStart) {
      return this.calendar.isDayStart(i);
    }
    if (i <= 0) return true;
    const prev = this.getLocalParts(timeArray[i - 1]);
    const curr = this.getLocalParts(timeArray[i]);
    return prev.year !== curr.year || prev.month !== curr.month || prev.day !== curr.day;
  }

  isMonthBoundary(timeArray: Float64Array, i: number): boolean {
    if (i <= 0) return true;
    const prev = this.getLocalParts(timeArray[i - 1]);
    const curr = this.getLocalParts(timeArray[i]);
    return prev.year !== curr.year || prev.month !== curr.month;
  }

  formatLabel(timeArray: Float64Array, i: number, level: TimeZoomLevel): string {
    if (this.calendar?.label) {
      return this.calendar.label(i, level);
    }

    const { year, month, day, hours, minutes } = this.getLocalParts(timeArray[i]);
    const hh = hours.toString().padStart(2, '0');
    const mm = minutes.toString().padStart(2, '0');
    const mmm = MONTH_NAMES[month];

    const isDayStart = this.isDayBoundary(timeArray, i);
    const isMonthStart = this.isMonthBoundary(timeArray, i);

    switch (level) {
      case 'intraday':
        // If day start, show date; otherwise time HH:mm
        return isDayStart ? `${day} ${mmm}` : `${hh}:${mm}`;

      case 'daily':
        // Days to weeks: day starts labelled dd MMM, month starts with year
        return isMonthStart ? `${mmm} '${year.toString().slice(-2)}` : `${day} ${mmm}`;

      case 'monthly':
        // Months or more: month and year at month starts
        return `${mmm} ${year}`;
    }
  }

  generateTicks(
    timeArray: Float64Array,
    count: number,
    fromIndex: number,
    barsVisible: number,
    viewportWidthPx: number
  ): TimeTick[] {
    if (count === 0 || viewportWidthPx <= 0 || barsVisible <= 0) return [];

    const startIdx = Math.max(0, Math.floor(fromIndex));
    const endIdx = Math.min(count - 1, Math.ceil(fromIndex + barsVisible));
    if (startIdx >= endIdx) return [];

    const pxPerBar = viewportWidthPx / barsVisible;
    const visibleTimeSpanMs = timeArray[endIdx] - timeArray[startIdx];

    // Determine zoom level
    const ONE_DAY_MS = 24 * 3600_000;
    const SIXTY_DAYS_MS = 60 * ONE_DAY_MS;

    let level: TimeZoomLevel = 'intraday';
    if (visibleTimeSpanMs >= SIXTY_DAYS_MS) {
      level = 'monthly';
    } else if (visibleTimeSpanMs >= ONE_DAY_MS) {
      level = 'daily';
    }

    // Step calculation for minimum pixel distance
    const minBarsBetweenTicks = Math.max(1, Math.ceil(this.minTickSpacingPx / pxPerBar));

    const ticks: TimeTick[] = [];
    let lastTickBar = -Infinity;

    // Scan visible range and collect candidates
    for (let i = startIdx; i <= endIdx; i++) {
      const isDayStart = this.isDayBoundary(timeArray, i);
      const isMonthStart = this.isMonthBoundary(timeArray, i);

      // Prioritize boundaries
      const shouldConsider =
        level === 'monthly'
          ? isMonthStart
          : level === 'daily'
          ? isDayStart
          : isDayStart || i - lastTickBar >= minBarsBetweenTicks;

      if (shouldConsider && i - lastTickBar >= minBarsBetweenTicks * 0.7) {
        ticks.push({
          barIndex: i,
          timeMs: timeArray[i],
          label: this.formatLabel(timeArray, i, level),
          isMajor: isDayStart || isMonthStart,
        });
        lastTickBar = i;
      }
    }

    return ticks;
  }
}
