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
