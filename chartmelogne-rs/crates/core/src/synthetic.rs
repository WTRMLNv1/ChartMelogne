use crate::types::CandleData;

/// Mulberry32 pseudo-random number generator for reproducible benchmarks.
pub struct Mulberry32 {
    state: u32,
}

impl Mulberry32 {
    pub fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    #[inline]
    pub fn next_f64(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x6d2b79f5);
        let mut t = (self.state ^ (self.state >> 15)).wrapping_mul(1 | self.state);
        t = (t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t))) ^ t;
        ((t ^ (t >> 14)) as f64) / 4294967296.0
    }
}

pub struct SyntheticDataOptions {
    pub count: usize,
    pub start_price: f64,
    pub start_time_ms: f64,
    pub interval_ms: f64,
    pub volatility: f64,
    pub seed: u32,
}

impl Default for SyntheticDataOptions {
    fn default() -> Self {
        Self {
            count: 450_000,
            start_price: 1000.0,
            start_time_ms: 1577870100000.0, // Date.UTC(2020, 0, 1, 9, 15, 0)
            interval_ms: 60_000.0,
            volatility: 0.002,
            seed: 1337,
        }
    }
}

/// Generates reproducible synthetic OHLCV data using a seeded random walk.
pub fn generate_synthetic_candles(options: SyntheticDataOptions) -> CandleData {
    let mut rng = Mulberry32::new(options.seed);

    let mut time = Vec::with_capacity(options.count);
    let mut open = Vec::with_capacity(options.count);
    let mut high = Vec::with_capacity(options.count);
    let mut low = Vec::with_capacity(options.count);
    let mut close = Vec::with_capacity(options.count);
    let mut volume = Vec::with_capacity(options.count);
    let flags = vec![0u8; options.count];

    let mut current_price = options.start_price;
    let mut current_time = options.start_time_ms;

    for _ in 0..options.count {
        time.push(current_time);

        // Irwin-Hall normal approximation: 4 uniform draws
        let r0 = rng.next_f64();
        let r1 = rng.next_f64();
        let r2 = rng.next_f64();
        let r3 = rng.next_f64();
        let delta = (r0 + r1 + r2 + r3 - 2.0) * options.volatility * current_price;

        let o = current_price;
        let mut c = o + delta;
        if c <= 1.0 {
            c = 1.0;
        }

        let max_oc = if o > c { o } else { c };
        let min_oc = if o < c { o } else { c };

        let upper_wick = rng.next_f64() * options.volatility * current_price * 1.5;
        let lower_wick = rng.next_f64() * options.volatility * current_price * 1.5;

        let h = max_oc + upper_wick;
        let l = (0.1_f64).max(min_oc - lower_wick);

        let vol = (100.0 + rng.next_f64() * 5000.0).floor() as u32;

        open.push(o);
        high.push(h);
        low.push(l);
        close.push(c);
        volume.push(vol);

        current_price = c;
        current_time += options.interval_ms;
    }

    CandleData {
        time,
        open,
        high,
        low,
        close,
        volume,
        flags: Some(flags),
    }
}
