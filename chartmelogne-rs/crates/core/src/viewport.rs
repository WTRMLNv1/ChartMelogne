use crate::types::{ViewportMode, ViewportState};

pub struct ViewportOptions {
    pub min_bars_visible: Option<f64>,
    pub right_padding_ratio: Option<f64>, // up to 0.5 (50% of viewport width)
    pub y_padding_ratio: Option<f64>,     // 0.05 (5%)
}

impl Default for ViewportOptions {
    fn default() -> Self {
        Self {
            min_bars_visible: Some(10.0),
            right_padding_ratio: Some(0.5),
            y_padding_ratio: Some(0.05),
        }
    }
}

pub struct Viewport {
    pub from_index: f64,
    pub bars_visible: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub y_mode: ViewportMode,

    pub target_y_min: f64,
    pub target_y_max: f64,

    pub width_px: f64,
    pub height_px: f64,
    pub dpr: f64,
    pub total_bars: usize,

    pub min_bars_visible: f64,
    pub right_padding_ratio: f64,
    pub y_padding_ratio: f64,
}

impl Viewport {
    pub fn new(options: ViewportOptions) -> Self {
        Self {
            from_index: 0.0,
            bars_visible: 100.0,
            y_min: 0.0,
            y_max: 1.0,
            y_mode: ViewportMode::Auto,

            target_y_min: 0.0,
            target_y_max: 1.0,

            width_px: 800.0,
            height_px: 600.0,
            dpr: 1.0,
            total_bars: 0,

            min_bars_visible: options.min_bars_visible.unwrap_or(10.0),
            right_padding_ratio: options.right_padding_ratio.unwrap_or(0.5),
            y_padding_ratio: options.y_padding_ratio.unwrap_or(0.05),
        }
    }

    #[inline]
    pub fn px_per_bar(&self) -> f64 {
        if self.bars_visible > 0.0 {
            self.width_px / self.bars_visible
        } else {
            1.0
        }
    }

    #[inline]
    pub fn y_scale(&self) -> f64 {
        let span = self.y_max - self.y_min;
        if span > 0.0 {
            self.height_px / span
        } else {
            1.0
        }
    }

    pub fn get_state(&self) -> ViewportState {
        ViewportState {
            from_index: self.from_index,
            bars_visible: self.bars_visible,
            y_min: self.y_min,
            y_max: self.y_max,
            y_mode: Some(self.y_mode),
        }
    }

    pub fn set_size(&mut self, width_px: f64, height_px: f64, dpr: f64) {
        self.width_px = width_px.max(1.0);
        self.height_px = height_px.max(1.0);
        self.dpr = dpr;
        self.clamp();
    }

    pub fn set_total_bars(&mut self, total: usize) {
        self.total_bars = total;
        self.clamp();
    }

    pub fn fit_content(&mut self) {
        if self.total_bars == 0 {
            self.from_index = 0.0;
            self.bars_visible = 100.0;
            return;
        }
        let total = self.total_bars as f64;
        let padding_bars = (total * 0.1).min(50.0);
        self.bars_visible = self.min_bars_visible.max(total + padding_bars);
        self.from_index = 0.0;
        self.clamp();
    }

    pub fn pan_by_pixels(&mut self, delta_screen_px: f64) {
        let delta_bars = -delta_screen_px / self.px_per_bar();
        self.from_index += delta_bars;
        self.clamp();
    }

    pub fn zoom_at(&mut self, cursor_screen_px: f64, zoom_factor: f64) {
        let anchor_bar = self.screen_px_to_bar_index(cursor_screen_px);
        let new_bars_visible = self.bars_visible * zoom_factor;

        self.bars_visible = self.clamp_bars_visible(new_bars_visible);
        let new_px_per_bar = self.width_px / self.bars_visible;
        self.from_index = anchor_bar - cursor_screen_px / new_px_per_bar;
        self.clamp();
    }

    pub fn set_visible_range(&mut self, from_index: f64, to_index: f64) {
        let span = self.min_bars_visible.max(to_index - from_index);
        self.bars_visible = span;
        self.from_index = from_index;
        self.clamp();
    }

    pub fn update_auto_y(&mut self, raw_min: f64, raw_max: f64, immediate: bool) {
        if self.y_mode != ViewportMode::Auto {
            return;
        }

        let mut span = raw_max - raw_min;
        if span <= 0.0 {
            span = 1.0f64.max(raw_min.abs() * 0.1);
        }

        let pad = span * self.y_padding_ratio;
        self.target_y_min = raw_min - pad;
        self.target_y_max = raw_max + pad;

        if immediate || (self.y_min == 0.0 && self.y_max == 1.0) {
            self.y_min = self.target_y_min;
            self.y_max = self.target_y_max;
        } else {
            // Exponential smoothing step
            let alpha = 0.25;
            self.y_min += (self.target_y_min - self.y_min) * alpha;
            self.y_max += (self.target_y_max - self.y_max) * alpha;
        }
    }

    #[inline]
    pub fn screen_px_to_bar_index(&self, screen_px: f64) -> f64 {
        self.from_index + screen_px / self.px_per_bar()
    }

    #[inline]
    pub fn bar_index_to_screen_px(&self, bar_index: f64) -> f64 {
        (bar_index - self.from_index) * self.px_per_bar()
    }

    #[inline]
    pub fn price_to_screen_px(&self, price: f64) -> f64 {
        (price - self.y_min) * self.y_scale()
    }

    #[inline]
    pub fn screen_px_to_price(&self, screen_px: f64) -> f64 {
        self.y_min + screen_px / self.y_scale()
    }

    pub fn is_last_bar_visible(&self) -> bool {
        if self.total_bars == 0 {
            return true;
        }
        let last_bar = (self.total_bars - 1) as f64;
        last_bar >= self.from_index && last_bar <= self.from_index + self.bars_visible
    }

    fn clamp_bars_visible(&self, bars: f64) -> f64 {
        let max_bars = self.min_bars_visible.max(if self.total_bars > 0 {
            (self.total_bars * 2) as f64
        } else {
            10000.0
        });
        bars.clamp(self.min_bars_visible, max_bars)
    }

    fn clamp(&mut self) {
        self.bars_visible = self.clamp_bars_visible(self.bars_visible);

        if self.total_bars == 0 {
            return;
        }

        let padding_bars = self.bars_visible * self.right_padding_ratio;
        let min_from_index = -self.bars_visible * 0.5;
        let max_from_index = (self.total_bars as f64) - self.bars_visible + padding_bars;

        if self.from_index < min_from_index {
            self.from_index = min_from_index;
        } else if self.from_index > max_from_index {
            self.from_index = max_from_index;
        }
    }
}
