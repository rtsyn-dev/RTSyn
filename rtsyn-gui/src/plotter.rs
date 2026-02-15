use egui::Color32;
use egui_plot::{Line, Plot, PlotPoints};
use plotters::backend::SVGBackend;
use plotters::prelude::*;
use std::collections::VecDeque;
use std::path::Path;

const MAX_SERIES: usize = 32;

#[derive(Clone, Copy)]
pub(crate) struct SeriesTransform {
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

pub(crate) struct LivePlotter {
    pub(crate) plugin_id: u64,
    pub(crate) open: bool,
    pub(crate) input_count: usize,
    pub(crate) refresh_hz: f64,
    pub(crate) window_ms: f64,
    max_points: usize,
    max_points_effective: usize,
    bucket_size: u64,
    bucket_count: u64,
    bucket_minmax: Vec<SeriesMinMax>,
    last_tick: Option<u64>,
    last_time_x: Option<f64>,
    last_time_scale: f64,
    series: Vec<PlotSeries>,
    raw_series: Vec<VecDeque<(f64, f64)>>, // Raw data for smooth exports
}

struct PlotSeries {
    name: String,
    color: Color32,
    points: VecDeque<(f64, f64)>,
}

#[derive(Clone, Copy, Default)]
struct SeriesMinMax {
    min: Option<(f64, f64)>,
    max: Option<(f64, f64)>,
}

impl LivePlotter {
    pub(crate) fn new(plugin_id: u64) -> Self {
        Self {
            plugin_id,
            open: false,
            input_count: 0,
            refresh_hz: 60.0,
            window_ms: 10_000.0,
            max_points: 200,
            max_points_effective: 200,
            bucket_size: 1,
            bucket_count: 0,
            bucket_minmax: Vec::new(),
            last_tick: None,
            last_time_x: None,
            last_time_scale: 1000.0,
            series: Vec::new(),
            raw_series: Vec::new(),
        }
    }

    pub(crate) fn update_config(&mut self, input_count: usize, refresh_hz: f64, period_s: f64) {
        self.input_count = input_count.min(MAX_SERIES);
        self.refresh_hz = if refresh_hz <= 0.0 { 60.0 } else { refresh_hz };
        let period_s = if period_s <= 0.0 { 0.0 } else { period_s };
        let expected_points = if period_s > 0.0 {
            (self.window_ms / (period_s * 1000.0)).ceil() as usize
        } else {
            0
        };
        const MIN_POINTS: usize = 200;
        const MAX_POINTS: usize = 100000;
        self.max_points = if expected_points == 0 {
            MIN_POINTS
        } else {
            expected_points.clamp(MIN_POINTS, MAX_POINTS)
        };
        self.bucket_size = if expected_points > self.max_points && expected_points > 0 {
            1 // Force no bucketing to avoid min-max artifacts
        } else {
            1
        };
        self.bucket_size = self.bucket_size.max(1);
        self.bucket_count = 0;
        self.max_points_effective = if self.bucket_size > 1 {
            self.max_points.saturating_mul(2)
        } else {
            self.max_points
        };
        if self.series.len() != self.input_count {
            self.series = (0..self.input_count)
                .map(|idx| PlotSeries {
                    name: format!("in_{idx}"),
                    color: palette_color(idx),
                    points: VecDeque::new(),
                })
                .collect();
        }
        if self.raw_series.len() != self.input_count {
            self.raw_series = vec![VecDeque::new(); self.input_count];
        }
        if self.bucket_minmax.len() != self.input_count {
            self.bucket_minmax = vec![SeriesMinMax::default(); self.input_count];
        }
    }

    pub(crate) fn set_window_ms(&mut self, window_ms: f64) {
        self.window_ms = if window_ms <= 0.0 { 1.0 } else { window_ms };
    }

    pub(crate) fn set_series_names(&mut self, names: Vec<String>) {
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for (idx, name) in names.into_iter().enumerate() {
            if let Some(series) = self.series.get_mut(idx) {
                let entry = counts.entry(name.clone()).or_insert(0);
                *entry += 1;
                if *entry == 1 {
                    series.name = name;
                } else {
                    series.name = format!("{name} ({})", *entry);
                }
            }
        }
    }

    pub(crate) fn push_sample(&mut self, tick: u64, time_s: f64, time_scale: f64, values: &[f64]) {
        if self.last_tick == Some(tick) {
            return;
        }
        self.last_tick = Some(tick);
        let time_x = time_s * time_scale;
        if let (Some(_prev_time), prev_scale) = (self.last_time_x, self.last_time_scale) {
            let scale_shift = (prev_scale - time_scale).abs() > f64::EPSILON;
            if scale_shift {
                for series in &mut self.series {
                    series.points.clear();
                }
                for raw_series in &mut self.raw_series {
                    raw_series.clear();
                }
                self.bucket_count = 0;
                for entry in &mut self.bucket_minmax {
                    *entry = SeriesMinMax::default();
                }
            }
        }
        self.last_time_x = Some(time_x);
        self.last_time_scale = time_scale;

        // Always store raw data for smooth exports
        for (idx, value) in values.iter().copied().enumerate() {
            if let Some(raw_series) = self.raw_series.get_mut(idx) {
                raw_series.push_back((time_x, value));
            }
        }

        if self.bucket_size == 1 {
            for (idx, value) in values.iter().copied().enumerate() {
                if let Some(series) = self.series.get_mut(idx) {
                    series.points.push_back((time_x, value));
                }
            }
        } else {
            for (idx, value) in values.iter().copied().enumerate() {
                if let Some(entry) = self.bucket_minmax.get_mut(idx) {
                    let next = (time_x, value);
                    entry.min = Some(match entry.min {
                        Some(prev) if prev.1 <= value => prev,
                        _ => next,
                    });
                    entry.max = Some(match entry.max {
                        Some(prev) if prev.1 >= value => prev,
                        _ => next,
                    });
                }
            }
            self.bucket_count += 1;
            if self.bucket_count >= self.bucket_size {
                self.bucket_count = 0;
                for (idx, entry) in self.bucket_minmax.iter_mut().enumerate() {
                    if let Some(series) = self.series.get_mut(idx) {
                        let min = entry.min.take();
                        let max = entry.max.take();
                        match (min, max) {
                            (Some(a), Some(b)) => {
                                if a.0 <= b.0 {
                                    series.points.push_back(a);
                                    if a.0 != b.0 || a.1 != b.1 {
                                        series.points.push_back(b);
                                    }
                                } else {
                                    series.points.push_back(b);
                                    if a.0 != b.0 || a.1 != b.1 {
                                        series.points.push_back(a);
                                    }
                                }
                            }
                            (Some(a), None) | (None, Some(a)) => {
                                series.points.push_back(a);
                            }
                            (None, None) => {}
                        }
                    }
                }
            }
        }
        self.prune_old(time_x, time_scale);
    }

    pub(crate) fn render(&mut self, ui: &mut egui::Ui, title: &str, time_label: &str) {
        self.render_with_settings(
            ui, title, time_label, true, true, true, None, None, None, None, true, None, None, None,
        );
    }

    pub(crate) fn render_with_settings(
        &mut self,
        ui: &mut egui::Ui,
        title: &str,
        time_label: &str,
        show_axes: bool,
        show_legend: bool,
        show_grid: bool,
        custom_title: Option<&str>,
        custom_series_names: Option<&[String]>,
        custom_series_transforms: Option<&[SeriesTransform]>,
        custom_colors: Option<&[egui::Color32]>,
        dark_theme: bool,
        x_axis_name: Option<&str>,
        y_axis_name: Option<&str>,
        custom_window_ms: Option<f64>,
    ) {
        self.flush_pending_bucket();
        let (min_time, max_time, min_y, max_y) =
            self.compute_bounds(custom_series_transforms, custom_window_ms);

        let display_title = custom_title.unwrap_or(title);

        let mut plot = Plot::new(format!("plot_{}", self.plugin_id))
            .allow_scroll(false)
            .allow_zoom(false)
            .allow_boxed_zoom(false)
            .allow_drag(false);

        if show_legend {
            plot = plot.legend(egui_plot::Legend::default());
        }

        if show_axes {
            let x_label = x_axis_name.unwrap_or(time_label);
            let y_label = y_axis_name.unwrap_or("value");
            plot = plot.x_axis_label(x_label).y_axis_label(y_label);
        }
        plot = plot.show_grid(show_grid);

        // Add title if provided
        if !display_title.is_empty() {
            ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(display_title).strong().size(16.0));
            });
        }

        // Apply theme
        if !dark_theme {
            ui.style_mut().visuals = egui::Visuals::light();
        }

        plot.show(ui, |plot_ui| {
            // Configure grid if needed
            if show_grid && show_axes {
                // Grid is handled by egui_plot automatically when axes are shown
            }

            for (i, series) in self.series.iter().enumerate() {
                if series.points.is_empty() {
                    continue;
                }
                let points: PlotPoints = series
                    .points
                    .iter()
                    .map(|(x, y)| {
                        [
                            *x,
                            transform_value(*y, i, custom_series_transforms).unwrap_or(*y),
                        ]
                    })
                    .collect();

                let series_name = custom_series_names
                    .and_then(|names| names.get(i))
                    .map(|s| s.as_str())
                    .unwrap_or(&series.name);

                let series_color = custom_colors
                    .and_then(|colors| colors.get(i))
                    .copied()
                    .unwrap_or(series.color);

                let line = Line::new(points).color(series_color).name(series_name);
                plot_ui.line(line);
            }
            if min_time.is_finite() && max_time.is_finite() {
                plot_ui.set_plot_bounds(egui_plot::PlotBounds::from_min_max(
                    [min_time, min_y],
                    [max_time, max_y],
                ));
            }
        });

        if !title.is_empty() {
            ui.label(title);
        }
    }

    pub(crate) fn export_png_with_settings(
        &mut self,
        path: &Path,
        _time_label: &str,
        show_axes: bool,
        show_legend: bool,
        show_grid: bool,
        title: &str,
        series_names: &[String],
        series_transforms: &[SeriesTransform],
        series_colors: &[egui::Color32],
        dark_theme: bool,
        x_axis_name: &str,
        y_axis_name: &str,
        window_ms: f64,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        // Force flush any pending bucket data first
        self.flush_pending_bucket();

        // Temporarily disable bucketing for smooth export
        let original_bucket_size = self.bucket_size;
        self.bucket_size = 1;

        let (min_time, max_time, min_y, max_y) =
            self.compute_bounds(Some(series_transforms), Some(window_ms));
        if !min_time.is_finite() || !max_time.is_finite() {
            self.bucket_size = original_bucket_size; // Restore
            return Err("No samples to export.".to_string());
        }

        let root = BitMapBackend::new(path, (width, height)).into_drawing_area();
        let bg_color = if dark_theme {
            RGBColor(24, 24, 24)
        } else {
            RGBColor(255, 255, 255)
        };
        let text_color = if dark_theme {
            RGBColor(220, 220, 220)
        } else {
            RGBColor(40, 40, 40)
        };

        root.fill(&bg_color).map_err(|e| e.to_string())?;

        let label_size = if show_axes { 40 } else { 0 };
        let mut chart = if !title.is_empty() {
            ChartBuilder::on(&root)
                .margin(20)
                .caption(title, ("sans-serif", 24).into_font().color(&text_color))
                .set_label_area_size(LabelAreaPosition::Left, label_size)
                .set_label_area_size(LabelAreaPosition::Bottom, label_size)
                .build_cartesian_2d(min_time..max_time, min_y..max_y)
                .map_err(|e| e.to_string())?
        } else {
            ChartBuilder::on(&root)
                .margin(20)
                .set_label_area_size(LabelAreaPosition::Left, label_size)
                .set_label_area_size(LabelAreaPosition::Bottom, label_size)
                .build_cartesian_2d(min_time..max_time, min_y..max_y)
                .map_err(|e| e.to_string())?
        };

        let mut mesh = chart.configure_mesh();
        let axis_color = if dark_theme {
            RGBColor(80, 80, 80)
        } else {
            RGBColor(120, 120, 120)
        };

        if show_axes {
            mesh.x_desc(x_axis_name)
                .y_desc(y_axis_name)
                .axis_desc_style(("sans-serif", 16).into_font().color(&text_color))
                .label_style(("sans-serif", 14).into_font().color(&text_color))
                .axis_style(&axis_color);
            if !show_grid {
                mesh.disable_mesh();
            } else {
                mesh.light_line_style(&axis_color)
                    .bold_line_style(&axis_color);
            }
        } else {
            mesh.disable_mesh().x_labels(0).y_labels(0);
        }
        mesh.draw().map_err(|e| e.to_string())?;

        for (i, raw_series) in self.raw_series.iter().enumerate() {
            if raw_series.is_empty() {
                // Fallback to bucketed data if raw data is empty
                if let Some(series) = self.series.get(i) {
                    if series.points.is_empty() {
                        continue;
                    }
                    let color = series_colors
                        .get(i)
                        .map(|c| RGBColor(c.r(), c.g(), c.b()))
                        .unwrap_or_else(|| {
                            RGBColor(series.color.r(), series.color.g(), series.color.b())
                        });
                    let name = series_names
                        .get(i)
                        .cloned()
                        .unwrap_or_else(|| series.name.clone());

                    let data = series
                        .points
                        .iter()
                        .filter(|(x, _)| *x >= min_time && *x <= max_time)
                        .map(|(x, y)| {
                            (
                                *x,
                                transform_value(*y, i, Some(series_transforms)).unwrap_or(*y),
                            )
                        });

                    let series_plot = chart
                        .draw_series(LineSeries::new(data, color.stroke_width(1)))
                        .map_err(|e| e.to_string())?;
                    if show_legend {
                        series_plot.label(name).legend(move |(x, y)| {
                            PathElement::new(vec![(x, y), (x + 20, y)], &color)
                        });
                    }
                }
                continue;
            }
            let color = series_colors
                .get(i)
                .map(|c| RGBColor(c.r(), c.g(), c.b()))
                .unwrap_or_else(|| {
                    let series_color = self
                        .series
                        .get(i)
                        .map(|s| s.color)
                        .unwrap_or(egui::Color32::BLUE);
                    RGBColor(series_color.r(), series_color.g(), series_color.b())
                });
            let name = series_names.get(i).cloned().unwrap_or_else(|| {
                self.series
                    .get(i)
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| format!("Series {}", i + 1))
            });

            let data: Vec<(f64, f64)> = raw_series
                .iter()
                .filter(|(x, _)| *x >= min_time && *x <= max_time)
                .map(|(x, y)| {
                    (
                        *x,
                        transform_value(*y, i, Some(series_transforms)).unwrap_or(*y),
                    )
                })
                .collect();

            let series_plot = chart
                .draw_series(LineSeries::new(data, color.stroke_width(1)))
                .map_err(|e| e.to_string())?;
            if show_legend {
                series_plot
                    .label(name)
                    .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &color));
            }
        }

        if show_legend {
            chart
                .configure_series_labels()
                .background_style(if dark_theme {
                    RGBColor(18, 18, 18)
                } else {
                    RGBColor(240, 240, 240)
                })
                .border_style(if dark_theme {
                    RGBColor(80, 80, 80)
                } else {
                    RGBColor(120, 120, 120)
                })
                .label_font(("sans-serif", 16).into_font().color(&text_color))
                .position(SeriesLabelPosition::UpperRight)
                .margin(12)
                .draw()
                .map_err(|e| e.to_string())?;
        }

        root.present().map_err(|e| e.to_string())?;

        // Restore original bucket size
        self.bucket_size = original_bucket_size;
        Ok(())
    }

    pub(crate) fn export_svg_with_settings(
        &mut self,
        path: &Path,
        _time_label: &str,
        show_axes: bool,
        show_legend: bool,
        show_grid: bool,
        title: &str,
        series_names: &[String],
        series_transforms: &[SeriesTransform],
        series_colors: &[egui::Color32],
        dark_theme: bool,
        x_axis_name: &str,
        y_axis_name: &str,
        window_ms: f64,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        self.flush_pending_bucket();
        let (min_time, max_time, min_y, max_y) =
            self.compute_bounds(Some(series_transforms), Some(window_ms));
        if !min_time.is_finite() || !max_time.is_finite() {
            return Err("No samples to export.".to_string());
        }

        // Use SVG backend for vector output
        let root = SVGBackend::new(path, (width, height)).into_drawing_area();
        let bg_color = if dark_theme {
            RGBColor(24, 24, 24)
        } else {
            RGBColor(255, 255, 255)
        };
        let text_color = if dark_theme {
            RGBColor(220, 220, 220)
        } else {
            RGBColor(40, 40, 40)
        };

        root.fill(&bg_color).map_err(|e| e.to_string())?;

        let label_size = if show_axes { 40 } else { 0 };
        let mut chart = if !title.is_empty() {
            ChartBuilder::on(&root)
                .margin(20)
                .caption(title, ("sans-serif", 24).into_font().color(&text_color))
                .set_label_area_size(LabelAreaPosition::Left, label_size)
                .set_label_area_size(LabelAreaPosition::Bottom, label_size)
                .build_cartesian_2d(min_time..max_time, min_y..max_y)
                .map_err(|e| e.to_string())?
        } else {
            ChartBuilder::on(&root)
                .margin(20)
                .set_label_area_size(LabelAreaPosition::Left, label_size)
                .set_label_area_size(LabelAreaPosition::Bottom, label_size)
                .build_cartesian_2d(min_time..max_time, min_y..max_y)
                .map_err(|e| e.to_string())?
        };

        let mut mesh = chart.configure_mesh();
        let axis_color = if dark_theme {
            RGBColor(80, 80, 80)
        } else {
            RGBColor(120, 120, 120)
        };

        if show_axes {
            mesh.x_desc(x_axis_name)
                .y_desc(y_axis_name)
                .axis_desc_style(("sans-serif", 16).into_font().color(&text_color))
                .label_style(("sans-serif", 14).into_font().color(&text_color))
                .axis_style(&axis_color);
            if !show_grid {
                mesh.disable_mesh();
            } else {
                mesh.light_line_style(&axis_color)
                    .bold_line_style(&axis_color);
            }
        } else {
            mesh.disable_mesh().x_labels(0).y_labels(0);
        }
        mesh.draw().map_err(|e| e.to_string())?;

        for (i, raw_series) in self.raw_series.iter().enumerate() {
            if raw_series.is_empty() {
                continue;
            }
            let color = series_colors
                .get(i)
                .map(|c| RGBColor(c.r(), c.g(), c.b()))
                .unwrap_or_else(|| {
                    let series_color = self
                        .series
                        .get(i)
                        .map(|s| s.color)
                        .unwrap_or(egui::Color32::BLUE);
                    RGBColor(series_color.r(), series_color.g(), series_color.b())
                });
            let name = series_names.get(i).cloned().unwrap_or_else(|| {
                self.series
                    .get(i)
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| format!("Series {}", i + 1))
            });

            let data: Vec<(f64, f64)> = raw_series
                .iter()
                .filter(|(x, _)| *x >= min_time && *x <= max_time)
                .map(|(x, y)| {
                    (
                        *x,
                        transform_value(*y, i, Some(series_transforms)).unwrap_or(*y),
                    )
                })
                .collect();

            let series_plot = chart
                .draw_series(LineSeries::new(data, color.stroke_width(3)))
                .map_err(|e| e.to_string())?;
            if show_legend {
                series_plot
                    .label(name)
                    .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &color));
            }
        }

        if show_legend {
            chart
                .configure_series_labels()
                .background_style(if dark_theme {
                    RGBColor(18, 18, 18)
                } else {
                    RGBColor(240, 240, 240)
                })
                .border_style(if dark_theme {
                    RGBColor(80, 80, 80)
                } else {
                    RGBColor(120, 120, 120)
                })
                .label_font(("sans-serif", 16).into_font().color(&text_color))
                .position(SeriesLabelPosition::UpperRight)
                .margin(12)
                .draw()
                .map_err(|e| e.to_string())?;
        }

        root.present().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub(crate) fn export_png(&mut self, path: &Path, time_label: &str) -> Result<(), String> {
        self.export_png_with_settings(
            path,
            time_label,
            true,
            true,
            true,
            "",
            &[],
            &[],
            &[],
            true,
            time_label,
            "value",
            self.window_ms,
            1200,
            700,
        )
    }

    pub(crate) fn export_png_hq_with_settings(
        &mut self,
        path: &Path,
        _time_label: &str,
        show_axes: bool,
        show_legend: bool,
        show_grid: bool,
        title: &str,
        series_names: &[String],
        series_transforms: &[SeriesTransform],
        series_colors: &[egui::Color32],
        dark_theme: bool,
        x_axis_name: &str,
        y_axis_name: &str,
        window_ms: f64,
    ) -> Result<(), String> {
        self.flush_pending_bucket();
        let (min_time, max_time, min_y, max_y) =
            self.compute_bounds(Some(series_transforms), Some(window_ms));
        if !min_time.is_finite() || !max_time.is_finite() {
            return Err("No samples to export.".to_string());
        }

        // High quality settings: 4K resolution
        let root = BitMapBackend::new(path, (3840, 2160)).into_drawing_area();
        let bg_color = if dark_theme {
            RGBColor(24, 24, 24)
        } else {
            RGBColor(255, 255, 255)
        };
        let text_color = if dark_theme {
            RGBColor(220, 220, 220)
        } else {
            RGBColor(40, 40, 40)
        };

        root.fill(&bg_color).map_err(|e| e.to_string())?;

        let label_size = if show_axes { 80 } else { 0 };
        let mut chart = if !title.is_empty() {
            ChartBuilder::on(&root)
                .margin(40)
                .caption(title, ("sans-serif", 48).into_font().color(&text_color))
                .set_label_area_size(LabelAreaPosition::Left, label_size)
                .set_label_area_size(LabelAreaPosition::Bottom, label_size)
                .build_cartesian_2d(min_time..max_time, min_y..max_y)
                .map_err(|e| e.to_string())?
        } else {
            ChartBuilder::on(&root)
                .margin(40)
                .set_label_area_size(LabelAreaPosition::Left, label_size)
                .set_label_area_size(LabelAreaPosition::Bottom, label_size)
                .build_cartesian_2d(min_time..max_time, min_y..max_y)
                .map_err(|e| e.to_string())?
        };

        let mut mesh = chart.configure_mesh();
        let axis_color = if dark_theme {
            RGBColor(80, 80, 80)
        } else {
            RGBColor(120, 120, 120)
        };

        if show_axes {
            mesh.x_desc(x_axis_name)
                .y_desc(y_axis_name)
                .axis_desc_style(("sans-serif", 32).into_font().color(&text_color))
                .label_style(("sans-serif", 28).into_font().color(&text_color))
                .axis_style(&axis_color);
            if !show_grid {
                mesh.disable_mesh();
            } else {
                mesh.light_line_style(&axis_color)
                    .bold_line_style(&axis_color);
            }
        } else {
            mesh.disable_mesh().x_labels(0).y_labels(0);
        }
        mesh.draw().map_err(|e| e.to_string())?;

        for (i, series) in self.series.iter().enumerate() {
            if series.points.is_empty() {
                continue;
            }
            let color = series_colors
                .get(i)
                .map(|c| RGBColor(c.r(), c.g(), c.b()))
                .unwrap_or_else(|| RGBColor(series.color.r(), series.color.g(), series.color.b()));
            let name = series_names
                .get(i)
                .cloned()
                .unwrap_or_else(|| series.name.clone());

            // Filter out min-max artifacts by removing rapid oscillations
            let filtered_data: Vec<(f64, f64)> = {
                let points: Vec<(f64, f64)> = series
                    .points
                    .iter()
                    .filter(|(x, _)| *x >= min_time && *x <= max_time)
                    .map(|(x, y)| {
                        (
                            *x,
                            transform_value(*y, i, Some(series_transforms)).unwrap_or(*y),
                        )
                    })
                    .collect();

                if points.len() > 3 {
                    // Remove points that create rapid up-down-up patterns (min-max artifacts)
                    let mut filtered = Vec::with_capacity(points.len());
                    filtered.push(points[0]);

                    for i in 1..points.len() - 1 {
                        let prev = points[i - 1];
                        let curr = points[i];
                        let next = points[i + 1];

                        // Skip if this creates a sharp spike (min-max artifact)
                        let is_spike = (curr.1 - prev.1).abs() > (next.1 - curr.1).abs() * 3.0
                            && (curr.1 - next.1).abs() > (prev.1 - curr.1).abs() * 3.0;

                        if !is_spike {
                            filtered.push(curr);
                        }
                    }
                    filtered.push(points[points.len() - 1]);
                    filtered
                } else {
                    points
                }
            };

            // Thicker lines for high quality export with smoother rendering
            let series_plot = chart
                .draw_series(LineSeries::new(filtered_data, color.stroke_width(1)))
                .map_err(|e| e.to_string())?;
            if show_legend {
                series_plot.label(name).legend(move |(x, y)| {
                    PathElement::new(vec![(x, y), (x + 40, y)], color.stroke_width(6))
                });
            }
        }

        if show_legend {
            chart
                .configure_series_labels()
                .background_style(if dark_theme {
                    RGBColor(18, 18, 18)
                } else {
                    RGBColor(240, 240, 240)
                })
                .border_style(if dark_theme {
                    RGBColor(80, 80, 80)
                } else {
                    RGBColor(120, 120, 120)
                })
                .label_font(("sans-serif", 32).into_font().color(&text_color))
                .position(SeriesLabelPosition::UpperRight)
                .margin(24)
                .draw()
                .map_err(|e| e.to_string())?;
        }

        root.present().map_err(|e| e.to_string())?;
        Ok(())
    }

    fn flush_pending_bucket(&mut self) {
        if self.bucket_size <= 1 || self.bucket_count == 0 {
            return;
        }
        self.bucket_count = 0;
        for (idx, entry) in self.bucket_minmax.iter_mut().enumerate() {
            if let Some(series) = self.series.get_mut(idx) {
                let min = entry.min.take();
                let max = entry.max.take();
                match (min, max) {
                    (Some(a), Some(b)) => {
                        if a.0 <= b.0 {
                            series.points.push_back(a);
                            if a.0 != b.0 || a.1 != b.1 {
                                series.points.push_back(b);
                            }
                        } else {
                            series.points.push_back(b);
                            if a.0 != b.0 || a.1 != b.1 {
                                series.points.push_back(a);
                            }
                        }
                    }
                    (Some(a), None) | (None, Some(a)) => {
                        series.points.push_back(a);
                    }
                    (None, None) => {}
                }
            }
        }
    }

    fn prune_old(&mut self, now_x: f64, time_scale: f64) {
        let window_units = (self.window_ms * time_scale / 1000.0).max(0.000_001);
        let min_time = now_x - window_units;
        for series in &mut self.series {
            while let Some((t, _)) = series.points.front().copied() {
                if t >= min_time {
                    break;
                }
                series.points.pop_front();
            }
            while series.points.len() > self.max_points_effective {
                series.points.pop_front();
            }
        }
        // Also prune raw data
        for raw_series in &mut self.raw_series {
            while let Some((t, _)) = raw_series.front().copied() {
                if t >= min_time {
                    break;
                }
                raw_series.pop_front();
            }
            while raw_series.len() > self.max_points_effective * 2 {
                raw_series.pop_front();
            }
        }
    }

    fn compute_bounds(
        &self,
        custom_series_transforms: Option<&[SeriesTransform]>,
        custom_window_ms: Option<f64>,
    ) -> (f64, f64, f64, f64) {
        let last_time = match self.last_time_x {
            Some(value) => value,
            None => return (f64::NAN, f64::NAN, f64::NAN, f64::NAN),
        };
        let window_ms = custom_window_ms.unwrap_or(self.window_ms).max(0.000_001);
        let window_units = (window_ms * self.last_time_scale / 1000.0).max(0.000_001);
        let min_time = last_time - window_units;
        let max_time = last_time;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for (series_idx, series) in self.series.iter().enumerate() {
            for (t, y) in &series.points {
                if *t < min_time {
                    continue;
                }
                if *t > max_time {
                    continue;
                }
                let transformed =
                    transform_value(*y, series_idx, custom_series_transforms).unwrap_or(*y);
                if transformed < min_y {
                    min_y = transformed;
                }
                if transformed > max_y {
                    max_y = transformed;
                }
            }
        }
        if min_y.is_infinite() || max_y.is_infinite() {
            return (min_time, max_time, -1.0, 1.0);
        }
        if min_y == max_y {
            min_y -= 1.0;
            max_y += 1.0;
        } else {
            let pad = (max_y - min_y) * 0.05;
            min_y -= pad;
            max_y += pad;
        }
        (min_time, max_time, min_y, max_y)
    }
}

fn palette_color(idx: usize) -> Color32 {
    const COLORS: [Color32; 10] = [
        Color32::from_rgb(86, 156, 214),
        Color32::from_rgb(220, 122, 95),
        Color32::from_rgb(181, 206, 168),
        Color32::from_rgb(197, 134, 192),
        Color32::from_rgb(220, 220, 170),
        Color32::from_rgb(156, 220, 254),
        Color32::from_rgb(255, 204, 102),
        Color32::from_rgb(206, 145, 120),
        Color32::from_rgb(78, 201, 176),
        Color32::from_rgb(214, 157, 133),
    ];
    COLORS[idx % COLORS.len()]
}

fn transform_value(value: f64, idx: usize, transforms: Option<&[SeriesTransform]>) -> Option<f64> {
    transforms
        .and_then(|ts| ts.get(idx))
        .map(|t| value * t.scale + t.offset)
}
