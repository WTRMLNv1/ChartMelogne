use crate::types::{Bar, CandleData, ValidationError};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MemoryDetails {
    pub time_bytes: usize,
    pub volume_bytes: usize,
    pub flags_bytes: usize,
    pub ohlc_bytes: usize,
    pub day_starts_bytes: usize,
    pub block_summary_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MemoryUsage {
    pub store_bytes: usize,
    pub cpu_instance_buffer_bytes: usize,
    pub total_bytes: usize,
    pub details: MemoryDetails,
}

pub fn validate_candle_data(data: &CandleData, count: usize) -> Option<ValidationError> {
    let n = count.min(data.time.len());
    for i in 0..n {
        let t = data.time[i];
        let o = data.open[i];
        let h = data.high[i];
        let l = data.low[i];
        let c = data.close[i];

        if o.is_nan() || h.is_nan() || l.is_nan() || c.is_nan() {
            return Some(ValidationError {
                index: i,
                reason: format!("NaN value in OHLC at index {}", i),
            });
        }
        if i > 0 && t <= data.time[i - 1] {
            return Some(ValidationError {
                index: i,
                reason: format!(
                    "Timestamp not strictly increasing at index {}: {} <= {}",
                    i, t, data.time[i - 1]
                ),
            });
        }
        let max_oc = if o > c { o } else { c };
        let min_oc = if o < c { o } else { c };

        if h < max_oc {
            return Some(ValidationError {
                index: i,
                reason: format!("High ({}) < max(Open, Close) ({}) at index {}", h, max_oc, i),
            });
        }
        if l > min_oc {
            return Some(ValidationError {
                index: i,
                reason: format!("Low ({}) > min(Open, Close) ({}) at index {}", l, min_oc, i),
            });
        }
    }
    None
}

pub struct CandleStore {
    capacity: usize,
    count: usize,
    base_price: f64,

    pub utc_offset_minutes: i32,
    pub day_starts: Vec<u32>,
    pub day_count: usize,
    last_local_day: i64,

    pub block_size: usize,
    pub block_min: Vec<f64>,
    pub block_max: Vec<f64>,
    pub block_max_vol: Vec<u32>,
    pub block_count: usize,

    pub time: Vec<f64>,
    pub volume: Vec<u32>,
    pub flags: Vec<u8>,

    /// Interleaved f32 buffer relative to base_price (16 bytes per candle)
    /// Format per candle: [open - base, high - base, low - base, close - base]
    pub ohlc: Vec<f32>,

    /// Interleaved f32 buffer for volume histogram (8 bytes per bar)
    /// Format per bar: [volume as f32, is_up (1.0 or 0.0)]
    pub volume_instances: Vec<f32>,

    pub scratch_instance: [f32; 4],
}

impl CandleStore {
    pub fn new(initial_capacity: usize, utc_offset_minutes: i32) -> Self {
        let cap = initial_capacity.max(64);
        let day_cap = 64.max((cap + 99) / 100);
        let block_cap = 16.max((cap + 255) / 256);

        Self {
            capacity: cap,
            count: 0,
            base_price: 0.0,

            utc_offset_minutes,
            day_starts: vec![0u32; day_cap],
            day_count: 0,
            last_local_day: i64::MIN,

            block_size: 256,
            block_min: vec![0.0; block_cap],
            block_max: vec![0.0; block_cap],
            block_max_vol: vec![0u32; block_cap],
            block_count: 0,

            time: vec![0.0; cap],
            volume: vec![0u32; cap],
            flags: vec![0u8; cap],
            ohlc: vec![0.0f32; cap * 4],
            volume_instances: vec![0.0f32; cap * 2],

            scratch_instance: [0.0; 4],
        }
    }

    #[inline]
    pub fn count(&self) -> usize {
        self.count
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    #[inline]
    pub fn base_price(&self) -> f64 {
        self.base_price
    }

    #[inline]
    pub fn get_open(&self, i: usize) -> f64 {
        self.base_price + self.ohlc[i * 4] as f64
    }

    #[inline]
    pub fn get_high(&self, i: usize) -> f64 {
        self.base_price + self.ohlc[i * 4 + 1] as f64
    }

    #[inline]
    pub fn get_low(&self, i: usize) -> f64 {
        self.base_price + self.ohlc[i * 4 + 2] as f64
    }

    #[inline]
    pub fn get_close(&self, i: usize) -> f64 {
        self.base_price + self.ohlc[i * 4 + 3] as f64
    }

    pub fn get_bar(&self, i: usize) -> Bar {
        let off = i * 4;
        Bar {
            time: self.time[i],
            open: self.base_price + self.ohlc[off] as f64,
            high: self.base_price + self.ohlc[off + 1] as f64,
            low: self.base_price + self.ohlc[off + 2] as f64,
            close: self.base_price + self.ohlc[off + 3] as f64,
            volume: self.volume[i],
            flags: Some(self.flags[i]),
        }
    }

    pub fn get_memory_usage(&self) -> MemoryUsage {
        let time_bytes = self.capacity * 8;
        let volume_bytes = self.capacity * 4;
        let flags_bytes = self.capacity * 1;
        let ohlc_bytes = self.capacity * 16;
        let day_starts_bytes = self.day_starts.len() * 4;
        let block_summary_bytes = (self.block_min.len() + self.block_max.len()) * 8;

        let store_bytes = time_bytes
            + volume_bytes
            + flags_bytes
            + ohlc_bytes
            + day_starts_bytes
            + block_summary_bytes;

        MemoryUsage {
            store_bytes,
            cpu_instance_buffer_bytes: 0,
            total_bytes: store_bytes,
            details: MemoryDetails {
                time_bytes,
                volume_bytes,
                flags_bytes,
                ohlc_bytes,
                day_starts_bytes,
                block_summary_bytes,
            },
        }
    }

    pub fn set_data(&mut self, data: &CandleData, debug: bool) -> Result<(), ValidationError> {
        let n = data.time.len();
        assert_eq!(data.open.len(), n, "Mismatched open length in setData");
        assert_eq!(data.high.len(), n, "Mismatched high length in setData");
        assert_eq!(data.low.len(), n, "Mismatched low length in setData");
        assert_eq!(data.close.len(), n, "Mismatched close length in setData");
        assert_eq!(data.volume.len(), n, "Mismatched volume length in setData");

        if debug {
            if let Some(err) = validate_candle_data(data, n) {
                return Err(err);
            }
        }

        if n > self.capacity {
            let mut new_cap = self.capacity;
            while new_cap < n {
                new_cap *= 2;
            }
            self.reallocate(new_cap);
        }

        self.count = n;
        self.time[..n].copy_from_slice(&data.time[..n]);
        self.volume[..n].copy_from_slice(&data.volume[..n]);

        if let Some(ref fl) = data.flags {
            self.flags[..n].copy_from_slice(&fl[..n]);
        } else {
            self.flags[..n].fill(0);
        }

        let base = if n > 0 { data.close[0] } else { 0.0 };
        self.base_price = base;

        // Prepare day_starts
        let utc_offset_ms = (self.utc_offset_minutes as f64) * 60_000.0;
        const INV_DAY_MS: f64 = 1.0 / 86_400_000.0;
        let day_cap = 128.max((n + 99) / 100);
        if self.day_starts.len() < day_cap {
            self.day_starts.resize(day_cap, 0);
        }

        // Prepare block min/max summaries (block size 256)
        let num_blocks = (n + 255) / 256;
        if self.block_min.len() < num_blocks {
            self.block_min.resize(num_blocks, 0.0);
            self.block_max.resize(num_blocks, 0.0);
            self.block_max_vol.resize(num_blocks, 0);
        }
        self.block_count = num_blocks;

        let mut days = 0;
        let mut prev_day = i64::MIN;
        let mut off = 0;
        let mut v_off = 0;

        // Fused cache-resident single pass over 256-bar blocks:
        // Packs OHLC & volume instances, tracks day boundaries, and accumulates block summaries
        for b in 0..num_blocks {
            let b_start = b << 8;
            let b_end = n.min(b_start + 256);
            let mut b_min_rel = f32::INFINITY;
            let mut b_max_rel = f32::NEG_INFINITY;
            let mut b_max_vol = 0u32;

            for i in b_start..b_end {
                let o = data.open[i];
                let h = data.high[i];
                let l = data.low[i];
                let c = data.close[i];
                let v = data.volume[i];

                let rel_o = (o - base) as f32;
                let rel_h = (h - base) as f32;
                let rel_l = (l - base) as f32;
                let rel_c = (c - base) as f32;

                self.ohlc[off] = rel_o;
                self.ohlc[off + 1] = rel_h;
                self.ohlc[off + 2] = rel_l;
                self.ohlc[off + 3] = rel_c;
                off += 4;

                self.volume_instances[v_off] = v as f32;
                self.volume_instances[v_off + 1] = if c >= o { 1.0 } else { 0.0 };
                v_off += 2;

                // Day boundary detection with reciprocal multiplication
                let local_day = ((self.time[i] + utc_offset_ms) * INV_DAY_MS).floor() as i64;
                if local_day != prev_day {
                    if days >= self.day_starts.len() {
                        self.day_starts.resize(self.day_starts.len() * 2, 0);
                    }
                    self.day_starts[days] = i as u32;
                    days += 1;
                    prev_day = local_day;
                }

                // Block min/max summary accumulation in relative space (exact bit parity with ohlc)
                if (self.flags[i] & 1) == 0 {
                    if rel_l < b_min_rel {
                        b_min_rel = rel_l;
                    }
                    if rel_h > b_max_rel {
                        b_max_rel = rel_h;
                    }
                    if v > b_max_vol {
                        b_max_vol = v;
                    }
                }
            }

            self.block_min[b] = if b_min_rel.is_infinite() {
                if b_start < n { base + self.ohlc[b_start * 4 + 2] as f64 } else { 0.0 }
            } else {
                base + b_min_rel as f64
            };
            self.block_max[b] = if b_max_rel.is_infinite() {
                if b_start < n { base + self.ohlc[b_start * 4 + 1] as f64 } else { 1.0 }
            } else {
                base + b_max_rel as f64
            };
            self.block_max_vol[b] = b_max_vol;
        }

        self.day_count = days;
        self.last_local_day = prev_day;

        Ok(())
    }

    pub fn append(&mut self, bar: Bar) -> usize {
        let idx = self.count;
        if idx >= self.capacity {
            let new_cap = self.capacity * 2;
            self.reallocate(new_cap);
        }

        if idx == 0 {
            self.base_price = bar.close;
        }

        let base = self.base_price;
        self.time[idx] = bar.time;
        self.volume[idx] = bar.volume;
        self.flags[idx] = bar.flags.unwrap_or(0);

        let off = idx * 4;
        let rel_o = (bar.open - base) as f32;
        let rel_h = (bar.high - base) as f32;
        let rel_l = (bar.low - base) as f32;
        let rel_c = (bar.close - base) as f32;

        self.ohlc[off] = rel_o;
        self.ohlc[off + 1] = rel_h;
        self.ohlc[off + 2] = rel_l;
        self.ohlc[off + 3] = rel_c;

        let v_off = idx * 2;
        self.volume_instances[v_off] = bar.volume as f32;
        self.volume_instances[v_off + 1] = if bar.close >= bar.open { 1.0 } else { 0.0 };

        self.scratch_instance[0] = rel_o;
        self.scratch_instance[1] = rel_h;
        self.scratch_instance[2] = rel_l;
        self.scratch_instance[3] = rel_c;

        self.count += 1;

        // Update day_starts
        let utc_offset_ms = (self.utc_offset_minutes as f64) * 60_000.0;
        let local_day = ((bar.time + utc_offset_ms) / 86_400_000.0).floor() as i64;
        if self.day_count == 0 || local_day != self.last_local_day {
            if self.day_count >= self.day_starts.len() {
                self.day_starts.resize(128.max(self.day_starts.len() * 2), 0);
            }
            self.day_starts[self.day_count] = idx as u32;
            self.day_count += 1;
            self.last_local_day = local_day;
        }

        // Update block min/max summaries
        let b = idx >> 8;
        if b >= self.block_count {
            if b >= self.block_min.len() {
                let next_cap = 16.max(self.block_min.len() * 2);
                self.block_min.resize(next_cap, 0.0);
                self.block_max.resize(next_cap, 0.0);
                self.block_max_vol.resize(next_cap, 0);
            }
            self.block_min[b] = bar.low;
            self.block_max[b] = bar.high;
            self.block_max_vol[b] = bar.volume;
            self.block_count = b + 1;
        } else {
            if bar.low < self.block_min[b] {
                self.block_min[b] = bar.low;
            }
            if bar.high > self.block_max[b] {
                self.block_max[b] = bar.high;
            }
            if bar.volume > self.block_max_vol[b] {
                self.block_max_vol[b] = bar.volume;
            }
        }

        idx
    }

    pub fn update_last(&mut self, bar: Bar) -> usize {
        if self.count == 0 {
            return self.append(bar);
        }
        let idx = self.count - 1;
        let base = self.base_price;

        let b = idx >> 8;
        let cur_min = self.block_min[b];
        let cur_max = self.block_max[b];
        let cur_max_vol = self.block_max_vol[b];

        let off = idx * 4;
        let prev_l = base + self.ohlc[off + 2] as f64;
        let prev_h = base + self.ohlc[off + 1] as f64;
        let prev_v = self.volume[idx];

        self.time[idx] = bar.time;
        self.volume[idx] = bar.volume;
        if let Some(fl) = bar.flags {
            self.flags[idx] = fl;
        }

        let rel_o = (bar.open - base) as f32;
        let rel_h = (bar.high - base) as f32;
        let rel_l = (bar.low - base) as f32;
        let rel_c = (bar.close - base) as f32;

        self.ohlc[off] = rel_o;
        self.ohlc[off + 1] = rel_h;
        self.ohlc[off + 2] = rel_l;
        self.ohlc[off + 3] = rel_c;

        let v_off = idx * 2;
        self.volume_instances[v_off] = bar.volume as f32;
        self.volume_instances[v_off + 1] = if bar.close >= bar.open { 1.0 } else { 0.0 };

        self.scratch_instance[0] = rel_o;
        self.scratch_instance[1] = rel_h;
        self.scratch_instance[2] = rel_l;
        self.scratch_instance[3] = rel_c;

        // Keep day_starts consistent
        let utc_offset_ms = (self.utc_offset_minutes as f64) * 60_000.0;
        let local_day = ((bar.time + utc_offset_ms) / 86_400_000.0).floor() as i64;
        if self.day_count > 0 && self.day_starts[self.day_count - 1] == idx as u32 {
            self.last_local_day = local_day;
        }

        // Fast-path O(1) block summary update
        let extreme_retracted = (prev_l == cur_min && bar.low > prev_l)
            || (prev_h == cur_max && bar.high < prev_h)
            || (prev_v == cur_max_vol && bar.volume < prev_v);

        if !extreme_retracted && b < self.block_count {
            if bar.low < cur_min {
                self.block_min[b] = bar.low;
            }
            if bar.high > cur_max {
                self.block_max[b] = bar.high;
            }
            if bar.volume > cur_max_vol {
                self.block_max_vol[b] = bar.volume;
            }
        } else {
            // Fallback: rescan the partial block only if an extreme retracted
            let b_start = b << 8;
            let b_end = idx + 1;
            let mut b_min = f64::INFINITY;
            let mut b_max = f64::NEG_INFINITY;
            let mut b_max_vol = 0u32;
            for i in b_start..b_end {
                if (self.flags[i] & 1) != 0 {
                    continue;
                }
                let i_off = i * 4;
                let l = base + self.ohlc[i_off + 2] as f64;
                let h = base + self.ohlc[i_off + 1] as f64;
                let v = self.volume[i];
                if l < b_min {
                    b_min = l;
                }
                if h > b_max {
                    b_max = h;
                }
                if v > b_max_vol {
                    b_max_vol = v;
                }
            }
            self.block_min[b] = if b_min.is_infinite() { bar.low } else { b_min };
            self.block_max[b] = if b_max.is_infinite() { bar.high } else { b_max };
            self.block_max_vol[b] = b_max_vol;
        }

        idx
    }

    pub fn get_visible_min_max(&self, from_index: f64, to_index: f64) -> (f64, f64) {
        if self.count == 0 {
            return (0.0, 1.0);
        }
        let start = 0.max(from_index.floor() as usize);
        let end = self.count.min(to_index.ceil() as usize);

        if start >= end {
            let idx = start.clamp(0, self.count - 1);
            return (self.get_low(idx), self.get_high(idx));
        }

        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        let base = self.base_price;

        let start_block = (start + 255) / 256;
        let end_block = end / 256;

        if start_block >= end_block {
            for i in start..end {
                if (self.flags[i] & 1) != 0 {
                    continue;
                }
                let off = i * 4;
                let l = base + self.ohlc[off + 2] as f64;
                let h = base + self.ohlc[off + 1] as f64;
                if l < min {
                    min = l;
                }
                if h > max {
                    max = h;
                }
            }
        } else {
            // 1. Partial head block
            let head_end = start_block << 8;
            for i in start..head_end {
                if (self.flags[i] & 1) != 0 {
                    continue;
                }
                let off = i * 4;
                let l = base + self.ohlc[off + 2] as f64;
                let h = base + self.ohlc[off + 1] as f64;
                if l < min {
                    min = l;
                }
                if h > max {
                    max = h;
                }
            }

            // 2. Full blocks
            for b in start_block..end_block {
                let bl = self.block_min[b];
                let bh = self.block_max[b];
                if bl < min {
                    min = bl;
                }
                if bh > max {
                    max = bh;
                }
            }

            // 3. Partial tail block
            let tail_start = end_block << 8;
            for i in tail_start..end {
                if (self.flags[i] & 1) != 0 {
                    continue;
                }
                let off = i * 4;
                let l = base + self.ohlc[off + 2] as f64;
                let h = base + self.ohlc[off + 1] as f64;
                if l < min {
                    min = l;
                }
                if h > max {
                    max = h;
                }
            }
        }

        if min.is_infinite() || max.is_infinite() {
            (0.0, 1.0)
        } else {
            (min, max)
        }
    }

    /// Direct byte slice of active packed OHLC instances for zero-copy GPU upload
    pub fn ohlc_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.ohlc[..self.count * 4])
    }

    /// Direct byte slice of active packed volume instances [volume as f32, is_up as f32]
    pub fn volume_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.volume_instances[..self.count * 2])
    }

    /// Maximum volume in the visible index range (block-accelerated)
    pub fn get_visible_max_volume(&self, from_index: f64, to_index: f64) -> u32 {
        if self.count == 0 {
            return 1;
        }
        let start = 0.max(from_index.floor() as usize);
        let end = self.count.min(to_index.ceil() as usize);

        if start >= end {
            let idx = start.clamp(0, self.count - 1);
            return self.volume[idx].max(1);
        }

        let mut max_vol = 1u32;
        let start_block = (start + 255) / 256;
        let end_block = end / 256;

        if start_block >= end_block {
            for i in start..end {
                let v = self.volume[i];
                if v > max_vol {
                    max_vol = v;
                }
            }
        } else {
            // Unaligned left fringe
            let left_end = (start_block << 8).min(end);
            for i in start..left_end {
                let v = self.volume[i];
                if v > max_vol {
                    max_vol = v;
                }
            }

            // Aligned blocks (256 bars per block)
            for b in start_block..end_block {
                let bv = self.block_max_vol[b];
                if bv > max_vol {
                    max_vol = bv;
                }
            }

            // Unaligned right fringe
            let right_start = end_block << 8;
            for i in right_start..end {
                let v = self.volume[i];
                if v > max_vol {
                    max_vol = v;
                }
            }
        }
        max_vol
    }

    fn reallocate(&mut self, new_cap: usize) {
        self.capacity = new_cap;
        self.time.resize(new_cap, 0.0);
        self.volume.resize(new_cap, 0);
        self.flags.resize(new_cap, 0);
        self.ohlc.resize(new_cap * 4, 0.0);
        self.volume_instances.resize(new_cap * 2, 0.0);

        let block_cap = 16.max((new_cap + 255) / 256);
        if self.block_min.len() < block_cap {
            self.block_min.resize(block_cap, 0.0);
            self.block_max.resize(block_cap, 0.0);
            self.block_max_vol.resize(block_cap, 0);
        }
    }
}
