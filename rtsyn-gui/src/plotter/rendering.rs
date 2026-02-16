use crate::plotter::core::LivePlotter;
use crate::plotter::data::SeriesTransform;
use crate::plotter::transform_value;
use egui_plot::{Line, Plot, PlotPoints};
use plotters::backend::SVGBackend;
use plotters::prelude::*;
use std::path::Path;

impl LivePlotter {
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

        if !display_title.is_empty() {
            ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(display_title).strong().size(16.0));
            });
        }

        if !dark_theme {
            ui.style_mut().visuals = egui::Visuals::light();
        }

        plot.show(ui, |plot_ui| {
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
        self.flush_pending_bucket();
        let original_bucket_size = self.bucket_size;
        self.bucket_size = 1;

        let (min_time, max_time, min_y, max_y) =
            self.compute_bounds(Some(series_transforms), Some(window_ms));
        if !min_time.is_finite() || !max_time.is_finite() {
            self.bucket_size = original_bucket_size;
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
                    let mut filtered = Vec::with_capacity(points.len());
                    filtered.push(points[0]);

                    for i in 1..points.len() - 1 {
                        let prev = points[i - 1];
                        let curr = points[i];
                        let next = points[i + 1];

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
}