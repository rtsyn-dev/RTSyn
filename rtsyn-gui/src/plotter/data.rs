use egui::Color32;
use std::collections::VecDeque;

/// Transformation parameters for scaling and offsetting data series values.
///
/// Used to apply linear transformations to data points before display,
/// allowing for unit conversions, calibration adjustments, and scaling.
#[derive(Clone, Copy)]
pub struct SeriesTransform {
    /// Multiplicative scaling factor applied to values
    pub scale: f64,
    /// Additive offset applied after scaling
    pub offset: f64,
}

impl Default for SeriesTransform {
    /// Creates a SeriesTransform with no transformation (scale=1.0, offset=0.0).
    fn default() -> Self {
        Self {
            scale: 1.0,
            offset: 0.0,
        }
    }
}

/// A data series for plotting, containing display metadata and time-series points.
///
/// Each series represents one channel of data with its own name, color, and
/// collection of (time, value) points stored in a deque for efficient
/// front/back operations during windowing.
pub(crate) struct PlotSeries {
    /// Display name for the series (shown in legend)
    pub name: String,
    /// Color used for rendering the series line
    pub color: Color32,
    /// Time-ordered data points as (time, value) pairs
    pub points: VecDeque<(f64, f64)>,
}

/// Min/max tracking structure for bucketing operations.
///
/// When data rates exceed display capacity, multiple samples are aggregated
/// into buckets. This structure tracks the minimum and maximum values
/// within each bucket to preserve important signal characteristics.
#[derive(Clone, Copy, Default)]
pub(crate) struct SeriesMinMax {
    /// Point with minimum value in current bucket
    pub min: Option<(f64, f64)>,
    /// Point with maximum value in current bucket
    pub max: Option<(f64, f64)>,
}

impl PlotSeries {
    /// Creates a new PlotSeries with the specified name and color.
    ///
    /// # Arguments
    /// * `name` - Display name for the series
    /// * `color` - Color for rendering the series line
    ///
    /// # Returns
    /// A new PlotSeries with an empty points collection
    pub fn new(name: String, color: Color32) -> Self {
        Self {
            name,
            color,
            points: VecDeque::new(),
        }
    }
}
