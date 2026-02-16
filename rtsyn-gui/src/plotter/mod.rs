pub mod core;
pub mod data;
pub mod rendering;

pub use core::LivePlotter;
pub use data::SeriesTransform;

const MAX_SERIES: usize = 32;

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

fn transform_value(value: f64, idx: usize, transforms: Option<&[SeriesTransform]>) -> Option<f64> {
    transforms
        .and_then(|ts| ts.get(idx))
        .map(|t| value * t.scale + t.offset)
}