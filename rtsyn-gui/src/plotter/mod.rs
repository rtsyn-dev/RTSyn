/// Real-time data plotting module for RTSyn GUI.
///
/// This module provides efficient visualization of high-frequency data streams
/// with automatic bucketing, time windowing, and export capabilities.
///
/// # Key Components
/// - `LivePlotter`: Main plotting engine with bucketing and windowing
/// - `SeriesTransform`: Value transformation for scaling and calibration
/// - Rendering functions for egui integration and image export
/// - Color palette and utility functions
pub mod core;
pub mod data;
pub mod rendering;

pub use core::LivePlotter;
pub use data::SeriesTransform;

/// Maximum number of data series that can be displayed simultaneously.
/// This limit prevents excessive memory usage and maintains rendering performance.
const MAX_SERIES: usize = 32;

/// Returns a color from the predefined palette for the given series index.
///
/// The palette provides visually distinct colors that work well in both
/// light and dark themes. Colors cycle when the index exceeds palette size.
///
/// # Arguments
/// * `idx` - Series index (0-based)
///
/// # Returns
/// An egui Color32 value from the predefined palette
fn palette_color(idx: usize) -> egui::Color32 {
    const COLORS: [egui::Color32; 10] = [
        egui::Color32::from_rgb(86, 156, 214),
        egui::Color32::from_rgb(220, 122, 95),
        egui::Color32::from_rgb(181, 206, 168),
        egui::Color32::from_rgb(197, 134, 192),
        egui::Color32::from_rgb(220, 220, 170),
        egui::Color32::from_rgb(156, 220, 254),
        egui::Color32::from_rgb(255, 204, 102),
        egui::Color32::from_rgb(206, 145, 120),
        egui::Color32::from_rgb(78, 201, 176),
        egui::Color32::from_rgb(214, 157, 133),
    ];
    COLORS[idx % COLORS.len()]
}

/// Applies a linear transformation to a data value if a transform is available.
///
/// This function looks up the transformation for the specified series index
/// and applies the formula: `transformed_value = value * scale + offset`
///
/// # Arguments
/// * `value` - Original data value to transform
/// * `idx` - Series index to look up transformation
/// * `transforms` - Optional array of transformations per series
///
/// # Returns
/// `Some(transformed_value)` if a transform exists for the series,
/// `None` if no transforms are provided or the index is out of bounds
///
/// # Example
/// ```rust
/// let transforms = [SeriesTransform { scale: 2.0, offset: 1.0 }];
/// let result = transform_value(5.0, 0, Some(&transforms));
/// assert_eq!(result, Some(11.0)); // 5.0 * 2.0 + 1.0
/// ```
fn transform_value(value: f64, idx: usize, transforms: Option<&[SeriesTransform]>) -> Option<f64> {
    transforms
        .and_then(|ts| ts.get(idx))
        .map(|t| value * t.scale + t.offset)
}
