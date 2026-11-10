use std::collections::HashMap;

use crate::types::{TimeTick, TimeZoomLevel};

pub const MONTH_NAMES: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

pub struct CivilDate {
    pub year: i32,
    pub month: usize, // 0..11
    pub day: usize,   // 1..31
}

/// Converts a Unix day number (days since 1970-01-01) to civil year, month (0..11), day (1..31).
/// Algorithm by Howard Hinnant (public domain). Pure integer math with zero allocations.
pub fn civil_from_days(mut z: i64) -> CivilDate {
    z += 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    CivilDate {
        year: (y + if m <= 2 { 1 } else { 0 }) as i32,
        month: (m - 1) as usize,
        day: d as usize,
    }
}

pub fn binary_search_day_starts(day_starts: &[u32], day_count: usize, target_bar: usize) -> usize {
    let mut low = 0;
    let mut high = day_count.saturating_sub(1);
    let mut ans = day_count;
    if day_count == 0 {
        return 0;
    }
    while low <= high {
        let mid = (low + high) >> 1;
        if (day_starts[mid] as usize) >= target_bar {
            ans = mid;
            if mid == 0 {
                break;
            }
            high = mid - 1;
        } else {
            low = mid + 1;
        }
    }
    ans
}

pub struct LocalTimeParts {
    pub year: i32,
    pub month: usize,
    pub day: usize,
    pub hours: usize,
    pub minutes: usize,
}

pub struct TimeAxisOptions {
    pub utc_offset_minutes: Option<i32>, // e.g. 330 for IST
    pub min_tick_spacing_px: Option<f64>, // e.g. 80px
}

impl Default for TimeAxisOptions {
    fn default() -> Self {
        Self {
            utc_offset_minutes: Some(330),
            min_tick_spacing_px: Some(80.0),
        }
    }
}

pub struct TimeAxis {
    pub utc_offset_minutes: i32,
    pub utc_offset_ms: f64,
    pub min_tick_spacing_px: f64,

    // Static / cached label tables
    minute_strings: Vec<String>,
    day_month_strings: Vec<Vec<String>>,
    month_year_cache: HashMap<i32, String>,
    month_short_year_cache: HashMap<i32, String>,

    // Reusable tick pool to eliminate per-frame allocations
    tick_pool: Vec<TimeTick>,
}

impl TimeAxis {
    pub fn new(options: TimeAxisOptions) -> Self {
        let utc_offset = options.utc_offset_minutes.unwrap_or(330);
        let utc_offset_ms = (utc_offset as f64) * 60_000.0;
        let min_spacing = options.min_tick_spacing_px.unwrap_or(80.0);

        // Precompute all 1440 "HH:mm" strings
        let mut minute_strings = Vec::with_capacity(1440);
        for h in 0..24 {
            for m in 0..60 {
                minute_strings.push(format!("{:02}:{:02}", h, m));
            }
        }

        // Precompute all 32 * 12 "d MMM" strings
        let mut day_month_strings = Vec::with_capacity(32);
        for d in 0..=31 {
            let mut row = Vec::with_capacity(12);
            for m in 0..12 {
                row.push(format!("{} {}", d, MONTH_NAMES[m]));
            }
            day_month_strings.push(row);
        }

        Self {
            utc_offset_minutes: utc_offset,
            utc_offset_ms,
            min_tick_spacing_px: min_spacing,

            minute_strings,
            day_month_strings,
            month_year_cache: HashMap::new(),
            month_short_year_cache: HashMap::new(),

            tick_pool: Vec::new(),
        }
    }

    pub fn get_local_parts(&self, time_ms: f64) -> LocalTimeParts {
        let local_ms = time_ms + self.utc_offset_ms;
        let day_number = (local_ms / 86_400_000.0).floor() as i64;
        let civil = civil_from_days(day_number);
        let ms_in_day = local_ms.rem_euclid(86_400_000.0);
        let hours = (ms_in_day / 3_600_000.0).floor() as usize;
        let minutes = ((ms_in_day % 3_600_000.0) / 60_000.0).floor() as usize;

        LocalTimeParts {
            year: civil.year,
            month: civil.month,
            day: civil.day,
            hours,
            minutes,
        }
    }

    pub fn is_day_boundary(&self, time_array: &[f64], i: usize, day_starts: Option<&[u32]>) -> bool {
        if i == 0 {
            return true;
        }
        if let Some(starts) = day_starts {
            let idx = binary_search_day_starts(starts, starts.len(), i);
            return idx < starts.len() && starts[idx] as usize == i;
        }
        let prev_day = ((time_array[i - 1] + self.utc_offset_ms) / 86_400_000.0).floor() as i64;
        let curr_day = ((time_array[i] + self.utc_offset_ms) / 86_400_000.0).floor() as i64;
        prev_day != curr_day
    }

    pub fn is_month_boundary(&self, time_array: &[f64], i: usize) -> bool {
        if i == 0 {
            return true;
        }
        let prev_day = ((time_array[i - 1] + self.utc_offset_ms) / 86_400_000.0).floor() as i64;
        let curr_day = ((time_array[i] + self.utc_offset_ms) / 86_400_000.0).floor() as i64;
        let prev_civil = civil_from_days(prev_day);
        let curr_civil = civil_from_days(curr_day);
        prev_civil.year != curr_civil.year || prev_civil.month != curr_civil.month
    }

    pub fn format_label(&mut self, time_array: &[f64], i: usize, level: TimeZoomLevel) -> String {
        let local_ms = time_array[i] + self.utc_offset_ms;
        let day_number = (local_ms / 86_400_000.0).floor() as i64;
        let civil = civil_from_days(day_number);

        let is_day_start = self.is_day_boundary(time_array, i, None);
        let is_month_start = self.is_month_boundary(time_array, i);

        match level {
            TimeZoomLevel::Intraday => {
                if is_day_start {
                    self.day_month_strings[civil.day][civil.month].clone()
                } else {
                    let ms_in_day = local_ms.rem_euclid(86_400_000.0);
                    let minute_index = ((ms_in_day / 60_000.0).floor() as usize).min(1439);
                    self.minute_strings[minute_index].clone()
                }
            }
            TimeZoomLevel::Daily => {
                if is_month_start {
                    self.get_daily_month_start_string(civil.year, civil.month)
                } else {
                    self.day_month_strings[civil.day][civil.month].clone()
                }
            }
            TimeZoomLevel::Monthly => {
                self.get_month_year_string(civil.year, civil.month)
            }
        }
    }

    pub fn get_month_year_string(&mut self, year: i32, month: usize) -> String {
        let key = year * 100 + (month as i32);
        if let Some(s) = self.month_year_cache.get(&key) {
            return s.clone();
        }
        let s = format!("{} {}", MONTH_NAMES[month], year);
        self.month_year_cache.insert(key, s.clone());
        s
    }

    pub fn get_daily_month_start_string(&mut self, year: i32, month: usize) -> String {
        let key = year * 100 + (month as i32);
        if let Some(s) = self.month_short_year_cache.get(&key) {
            return s.clone();
        }
        let yy = (year.abs() % 100).to_string();
        let yy_padded = if yy.len() < 2 {
            format!("0{}", yy)
        } else {
            yy
        };
        let s = format!("{} '{}", MONTH_NAMES[month], yy_padded);
        self.month_short_year_cache.insert(key, s.clone());
        s
    }

    pub fn generate_ticks(
        &mut self,
        time_array: &[f64],
        count: usize,
        from_index: f64,
        bars_visible: f64,
        viewport_width_px: f64,
        day_starts: Option<&[u32]>,
        day_count: Option<usize>,
    ) -> &[TimeTick] {
        self.tick_pool.clear();
        if count == 0 || viewport_width_px <= 0.0 || bars_visible <= 0.0 {
            return &self.tick_pool;
        }

        let start_idx = 0.max(from_index.floor() as usize);
        let end_idx = (count - 1).min((from_index + bars_visible).ceil() as usize);
        if start_idx >= end_idx {
            return &self.tick_pool;
        }

        let px_per_bar = viewport_width_px / bars_visible;
        let visible_time_span_ms = time_array[end_idx] - time_array[start_idx];

        let one_day_ms = 24.0 * 3600_000.0;
        let sixty_days_ms = 60.0 * one_day_ms;

        let level = if visible_time_span_ms >= sixty_days_ms {
            TimeZoomLevel::Monthly
        } else if visible_time_span_ms >= one_day_ms {
            TimeZoomLevel::Daily
        } else {
            TimeZoomLevel::Intraday
        };

        let min_bars_between_ticks = 1.max((self.min_tick_spacing_px / px_per_bar).ceil() as usize);
        let min_step_threshold = (min_bars_between_ticks as f64) * 0.7;

        // Ensure we have day_starts
        let (d_starts_storage, d_starts, d_count) = if let (Some(starts), Some(dc)) = (day_starts, day_count) {
            (Vec::new(), starts, dc)
        } else {
            // Fast fallback pass
            let mut starts = Vec::with_capacity(32.max((count + 99) / 100));
            let mut prev_d = i64::MIN;
            for i in 0..count {
                let local_d = ((time_array[i] + self.utc_offset_ms) / 86_400_000.0).floor() as i64;
                if local_d != prev_d {
                    starts.push(i as u32);
                    prev_d = local_d;
                }
            }
            let dc = starts.len();
            (starts, &[][..], dc)
        };

        let active_starts: &[u32] = if !d_starts_storage.is_empty() {
            &d_starts_storage
        } else {
            d_starts
        };

        let first_day_idx = binary_search_day_starts(active_starts, d_count, start_idx);
        let mut last_day_idx = binary_search_day_starts(active_starts, d_count, end_idx);
        if last_day_idx < d_count && (active_starts[last_day_idx] as usize) <= end_idx {
            // include
        } else if d_count > 0 {
            last_day_idx = (d_count - 1).min(last_day_idx);
        }

        match level {
            TimeZoomLevel::Monthly => {
                let mut last_tick_bar = f64::NEG_INFINITY;
                let mut prev_month = -1i32;

                if first_day_idx > 0 {
                    let prev_bar = active_starts[first_day_idx - 1] as usize;
                    let prev_day_num = ((time_array[prev_bar] + self.utc_offset_ms) / 86_400_000.0).floor() as i64;
                    prev_month = civil_from_days(prev_day_num).month as i32;
                }

                for d in first_day_idx..=last_day_idx {
                    if d >= d_count {
                        break;
                    }
                    let bar_idx = active_starts[d] as usize;
                    if bar_idx < start_idx || bar_idx > end_idx {
                        continue;
                    }

                    let day_num = ((time_array[bar_idx] + self.utc_offset_ms) / 86_400_000.0).floor() as i64;
                    let civil = civil_from_days(day_num);
                    let is_month_start = (civil.month as i32) != prev_month;
                    prev_month = civil.month as i32;

                    if is_month_start && ((bar_idx as f64) - last_tick_bar) >= min_step_threshold {
                        let label = self.get_month_year_string(civil.year, civil.month);
                        self.tick_pool.push(TimeTick {
                            bar_index: bar_idx,
                            time_ms: time_array[bar_idx],
                            label,
                            is_major: true,
                        });
                        last_tick_bar = bar_idx as f64;
                    }
                }
            }
            TimeZoomLevel::Daily => {
                let mut last_tick_bar = f64::NEG_INFINITY;
                let mut prev_month = -1i32;

                if first_day_idx > 0 {
                    let prev_bar = active_starts[first_day_idx - 1] as usize;
                    let prev_day_num = ((time_array[prev_bar] + self.utc_offset_ms) / 86_400_000.0).floor() as i64;
                    prev_month = civil_from_days(prev_day_num).month as i32;
                }

                for d in first_day_idx..=last_day_idx {
                    if d >= d_count {
                        break;
                    }
                    let bar_idx = active_starts[d] as usize;
                    if bar_idx < start_idx || bar_idx > end_idx {
                        continue;
                    }

                    let day_num = ((time_array[bar_idx] + self.utc_offset_ms) / 86_400_000.0).floor() as i64;
                    let civil = civil_from_days(day_num);
                    let is_month_start = (civil.month as i32) != prev_month;
                    prev_month = civil.month as i32;

                    if ((bar_idx as f64) - last_tick_bar) >= min_step_threshold {
                        let label = if is_month_start {
                            self.get_daily_month_start_string(civil.year, civil.month)
                        } else {
                            self.day_month_strings[civil.day][civil.month].clone()
                        };

                        self.tick_pool.push(TimeTick {
                            bar_index: bar_idx,
                            time_ms: time_array[bar_idx],
                            label,
                            is_major: is_month_start,
                        });
                        last_tick_bar = bar_idx as f64;
                    }
                }
            }
            TimeZoomLevel::Intraday => {
                let mut next_day_start_idx = first_day_idx;
                let mut i = start_idx;

                while i <= end_idx {
                    let mut is_day_start = false;
                    if next_day_start_idx < d_count && (active_starts[next_day_start_idx] as usize) <= i {
                        if (active_starts[next_day_start_idx] as usize) >= start_idx {
                            is_day_start = true;
                        }
                        next_day_start_idx += 1;
                    }

                    let local_ms = time_array[i] + self.utc_offset_ms;
                    let day_number = (local_ms / 86_400_000.0).floor() as i64;
                    let civil = civil_from_days(day_number);

                    let label = if is_day_start
                        || (i == start_idx && first_day_idx < d_count && active_starts[first_day_idx] as usize == i)
                    {
                        self.day_month_strings[civil.day][civil.month].clone()
                    } else {
                        let ms_in_day = local_ms.rem_euclid(86_400_000.0);
                        let minute_index = ((ms_in_day / 60_000.0).floor() as usize).min(1439);
                        self.minute_strings[minute_index].clone()
                    };

                    self.tick_pool.push(TimeTick {
                        bar_index: i,
                        time_ms: time_array[i],
                        label,
                        is_major: is_day_start,
                    });

                    i += min_bars_between_ticks;
                }
            }
        }

        &self.tick_pool
    }
}
