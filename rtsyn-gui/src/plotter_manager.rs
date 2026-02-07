use crate::plotter::LivePlotter;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct PlotterManager {
    pub plotters: HashMap<u64, Arc<Mutex<LivePlotter>>>,
    pub plotter_preview_settings: HashMap<u64, (bool, bool, bool, Vec<String>, Vec<egui::Color32>, String, bool, String, String, bool, bool)>,
}

impl PlotterManager {
    pub fn new() -> Self {
        Self {
            plotters: HashMap::new(),
            plotter_preview_settings: HashMap::new(),
        }
    }
}

impl Default for PlotterManager {
    fn default() -> Self {
        Self::new()
    }
}
