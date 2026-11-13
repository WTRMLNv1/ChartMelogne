use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandleData {
    pub time: Vec<f64>,
    pub open: Vec<f64>,
    pub high: Vec<f64>,
    pub low: Vec<f64>,
    pub close: Vec<f64>,
    pub volume: Vec<u32>,
    pub flags: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bar {
    pub time: f64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: u32,
    pub flags: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewportMode {
    Auto,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewportState {
    pub from_index: f64,
    pub bars_visible: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub y_mode: Option<ViewportMode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Theme {
    pub background: String,
    pub grid: String,
    pub axis_text: String,
    pub crosshair: String,
    pub up_color: [f32; 4],
    pub down_color: [f32; 4],
    pub up_color_hex: Option<String>,
    pub down_color_hex: Option<String>,
    pub font_family: String,
    pub font_size: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ChartType {
    #[default]
    Candlestick,
    Line,
    Area,
    None,
}

impl std::str::FromStr for ChartType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "candlestick" | "candlesticks" | "candle" | "candles" => Ok(ChartType::Candlestick),
            "line" | "lines" => Ok(ChartType::Line),
            "area" | "mountain" => Ok(ChartType::Area),
            "none" | "hidden" => Ok(ChartType::None),
            other => Err(format!(
                "Unknown chart type '{other}'. Expected 'candlestick', 'line', 'area', or 'none'."
            )),
        }
    }
}

impl std::fmt::Display for ChartType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChartType::Candlestick => write!(f, "candlestick"),
            ChartType::Line => write!(f, "line"),
            ChartType::Area => write!(f, "area"),
            ChartType::None => write!(f, "none"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartOptions {
    pub theme: Option<Theme>,
    pub utc_offset_minutes: Option<i32>,
    pub price_decimals: Option<usize>,
    pub dpr: Option<f64>,
    pub allow_zoom: Option<bool>,
    pub show_axes: Option<bool>,
    pub chart_type: Option<ChartType>,
    pub show_volume: Option<bool>,
    pub show_indicators: Option<bool>,
}

impl Default for ChartOptions {
    fn default() -> Self {
        Self {
            theme: None,
            utc_offset_minutes: None,
            price_decimals: None,
            dpr: None,
            allow_zoom: Some(true),
            show_axes: Some(true),
            chart_type: Some(ChartType::Candlestick),
            show_volume: Some(true),
            show_indicators: Some(false),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationError {
    pub index: usize,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceTick {
    pub price: f64,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeTick {
    pub bar_index: usize,
    pub time_ms: f64,
    pub label: String,
    pub is_major: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimeZoomLevel {
    Intraday,
    Daily,
    Monthly,
}

/// Host-calculated technical indicator data rendered as an overlay line.
///
/// ChartMelogne does not compute indicator formulas internally; the host
/// application computes values and supplies them to the chart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostIndicator {
    /// Unique identifier for this indicator series (e.g. "ema_20")
    pub id: String,
    /// Human-readable display label for HUD and legends (e.g. "EMA 20")
    pub name: String,
    /// RGBA line color with components in 0.0..1.0
    pub color: [f32; 4],
    /// Stroke width in logical pixels (default: 1.5)
    pub line_width: f32,
    /// Pre-calculated values per bar index. None represents warmup/missing periods.
    pub values: Vec<Option<f64>>,
    /// Whether this indicator is currently visible
    pub visible: bool,
}

impl HostIndicator {
    pub fn new(name: impl Into<String>, color: [f32; 4], values: Vec<Option<f64>>) -> Self {
        let name_str = name.into();
        Self {
            id: name_str.clone(),
            name: name_str,
            color,
            line_width: 1.5,
            values,
            visible: true,
        }
    }

    pub fn from_f64_slice(name: impl Into<String>, color: [f32; 4], slice: &[f64]) -> Self {
        let values = slice
            .iter()
            .map(|&v| if v.is_nan() { None } else { Some(v) })
            .collect();
        Self::new(name, color, values)
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn with_line_width(mut self, width: f32) -> Self {
        self.line_width = width;
        self
    }

    pub fn with_visibility(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }
}

/// Direction for chart marker triangles (Up = Buy / Green, Down = Sell / Red).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MarkerDirection {
    Up,
    Down,
}

pub const DEFAULT_MARKER_UP_COLOR: [f32; 4] = [0.149, 0.651, 0.604, 1.0]; // Emerald Green (#26a69a)
pub const DEFAULT_MARKER_DOWN_COLOR: [f32; 4] = [0.937, 0.325, 0.314, 1.0]; // Coral Red (#ef5350)

/// A visual signal or trade marker placed at a specific candle index or timestamp.
/// Upwards triangles are placed a little bit below the LOW of the candle.
/// Downwards triangles are placed a little bit above the HIGH of the candle.
/// Multiple triangles on the same candle are stacked with space between them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartMarker {
    /// Candle index where the marker is placed (if pre-mapped by host)
    pub candle_index: Option<usize>,
    /// Timestamp in epoch seconds/ms (if time-based; auto-mapped to closest candle by store)
    pub time: Option<f64>,
    /// Optional price level in currency units (e.g. fill/execution price)
    pub price: Option<f64>,
    /// Up (triangle pointing up, green) or Down (triangle pointing down, red)
    pub direction: MarkerDirection,
    /// Optional RGBA color override. If None, Up defaults to green and Down defaults to red.
    pub color: Option<[f32; 4]>,
    /// Marker visual size in logical pixels (default: 10.0)
    pub size: f32,
    /// Optional textual label/tag (e.g. "BUY", "SELL", order quantity)
    pub text: Option<String>,
}

impl ChartMarker {
    /// Create a marker pre-mapped to a specific candle index
    pub fn new(candle_index: usize, direction: MarkerDirection) -> Self {
        Self {
            candle_index: Some(candle_index),
            time: None,
            price: None,
            direction,
            color: None,
            size: 10.0,
            text: None,
        }
    }

    /// Create a marker pre-mapped to a specific candle index and fill price
    pub fn new_with_price(candle_index: usize, price: f64, direction: MarkerDirection) -> Self {
        Self {
            candle_index: Some(candle_index),
            time: None,
            price: Some(price),
            direction,
            color: None,
            size: 10.0,
            text: None,
        }
    }

    /// Create a marker at a specific timestamp (auto-mapped to the closest candle bar)
    pub fn new_with_time(time: f64, direction: MarkerDirection) -> Self {
        Self {
            candle_index: None,
            time: Some(time),
            price: None,
            direction,
            color: None,
            size: 10.0,
            text: None,
        }
    }

    /// Helper to create an upward-pointing green triangle marker below the candle low
    pub fn up(candle_index: usize) -> Self {
        Self::new(candle_index, MarkerDirection::Up)
    }

    /// Helper to create a downward-pointing red triangle marker above the candle high
    pub fn down(candle_index: usize) -> Self {
        Self::new(candle_index, MarkerDirection::Down)
    }

    /// Helper to create an upward-pointing green triangle marker with fill price below candle low
    pub fn up_with_price(candle_index: usize, price: f64) -> Self {
        Self::new_with_price(candle_index, price, MarkerDirection::Up)
    }

    /// Helper to create a downward-pointing red triangle marker with fill price above candle high
    pub fn down_with_price(candle_index: usize, price: f64) -> Self {
        Self::new_with_price(candle_index, price, MarkerDirection::Down)
    }

    pub fn up_at_price(candle_index: usize, price: f64) -> Self {
        Self::up_with_price(candle_index, price)
    }

    pub fn down_at_price(candle_index: usize, price: f64) -> Self {
        Self::down_with_price(candle_index, price)
    }

    /// Helper to create an upward-pointing green triangle marker at a timestamp
    pub fn up_at_time(time: f64) -> Self {
        Self::new_with_time(time, MarkerDirection::Up)
    }

    /// Helper to create a downward-pointing red triangle marker at a timestamp
    pub fn down_at_time(time: f64) -> Self {
        Self::new_with_time(time, MarkerDirection::Down)
    }

    pub fn up_at_time_and_price(time: f64, price: f64) -> Self {
        Self::up_at_time(time).with_price(price)
    }

    pub fn down_at_time_and_price(time: f64, price: f64) -> Self {
        Self::down_at_time(time).with_price(price)
    }

    pub fn with_candle_index(mut self, candle_index: usize) -> Self {
        self.candle_index = Some(candle_index);
        self
    }

    pub fn with_time(mut self, time: f64) -> Self {
        self.time = Some(time);
        self
    }

    pub fn with_price(mut self, price: f64) -> Self {
        self.price = Some(price);
        self
    }

    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = Some(color);
        self
    }

    pub fn with_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Get the effective RGBA color (custom color if specified, or default green/red based on direction).
    pub fn effective_color(&self) -> [f32; 4] {
        self.color.unwrap_or(match self.direction {
            MarkerDirection::Up => DEFAULT_MARKER_UP_COLOR,
            MarkerDirection::Down => DEFAULT_MARKER_DOWN_COLOR,
        })
    }
}

/// GPU instance layout for instanced marker triangles (48 bytes per marker)
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MarkerGpuInstance {
    pub x: f32,           // Candle index (float)
    pub y_rel: f32,       // Candle low (for Up) or candle high (for Down) relative to base_price
    pub dir: f32,         // 0.0 = Up, 1.0 = Down
    pub size: f32,        // Marker visual size in logical pixels (e.g. 10.0)
    pub stack_index: f32, // Stacking level (0.0, 1.0, 2.0...) on this candle
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
    pub color: [f32; 4],  // RGBA
}
