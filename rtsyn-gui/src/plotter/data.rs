use egui::Color32;
use std::collections::VecDeque;

#[derive(Clone, Copy)]
pub struct SeriesTransform {
    pub scale: f64,
    pub offset: f64,
}

impl Default for SeriesTransform {
    fn default() -> Self {
        Self {
            scale: 1.0,
            offset: 0.0,
        }
    }
}

pub(crate) struct PlotSeries {
    pub name: String,
    pub color: Color32,
    pub points: VecDeque<(f64, f64)>,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct SeriesMinMax {
    pub min: Option<(f64, f64)>,
    pub max: Option<(f64, f64)>,
}

impl PlotSeries {
    pub fn new(name: String, color: Color32) -> Self {
        Self {
            name,
            color,
            points: VecDeque::new(),
        }
    }
}