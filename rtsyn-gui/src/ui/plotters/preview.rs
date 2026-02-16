//! Plotter UI components and window management for RTSyn GUI.
//!
//! This module provides the user interface components for managing and displaying
//! real-time plotting windows in the RTSyn application. It handles:
//!
//! - Plotter window rendering and viewport management
//! - Interactive controls for plot customization (knobs, wheels, timebase)
//! - Series data visualization with scaling and offset controls
//! - Export functionality for plot images (PNG/SVG)
//! - Settings dialogs for plot appearance and behavior
//! - Connection management for plotter plugins
//! - Notification display for plotter-specific messages
//!
//! The module implements a comprehensive plotting interface that allows users to
//! visualize real-time data streams from various plugins, customize appearance,
//! and export plots for documentation or analysis purposes.

use super::*;

impl GuiApp {
/// Renders the plotter preview and export dialog window.
    ///
    /// This function displays a comprehensive dialog that allows users to customize
    /// plot appearance, configure series settings, and export plots as images.
    /// It provides real-time preview of changes and extensive customization options.
    ///
    /// # Parameters
    /// - `ctx`: The egui context for rendering the dialog
    ///
    /// # Dialog Sections
    /// 1. **Basic Settings**: Title, axes visibility, legend, grid, theme
    /// 2. **Axis Configuration**: X and Y axis labels and formatting
    /// 3. **Timebase Controls**: Time window and division settings
    /// 4. **Series Customization**: Individual series names, colors, and transforms
    /// 5. **Live Preview**: Real-time plot preview with current settings
    /// 6. **Export Options**: Resolution, format, and quality settings
    ///
    /// # Interactive Features
    /// - **Real-time Preview**: Shows immediate feedback for all setting changes
    /// - **Series Tuning**: Individual scale and offset controls for each series
    /// - **Color Picker**: Full color customization for each data series
    /// - **Resolution Control**: Automatic aspect ratio maintenance for exports
    /// - **Format Selection**: PNG or SVG export options
    ///
    /// # State Management
    /// - Synchronizes with live plugin data automatically
    /// - Preserves user customizations across dialog sessions
    /// - Handles dynamic series count changes gracefully
    /// - Maintains consistent state between preview and export
    ///
    /// # Export Configuration
    /// - **Resolution**: Configurable width/height with aspect ratio locking
    /// - **Format**: PNG (raster) or SVG (vector) output options
    /// - **Quality**: High-quality rendering options for publication use
    /// - **Aspect Ratio**: Automatic 16:9 ratio maintenance with manual override
    ///
    /// # User Experience
    /// - **Responsive Layout**: Adapts to different window sizes
    /// - **Scrollable Content**: Handles large numbers of series gracefully
    /// - **Immediate Feedback**: All changes reflected in preview instantly
    /// - **Intuitive Controls**: Familiar UI patterns for all interactions
    ///
    /// # Implementation Details
    /// - Uses `build_plotter_preview_state()` for initialization
    /// - Applies `sync_series_controls_from_seed()` for consistency
    /// - Triggers screenshot requests through `request_plotter_screenshot()`
    /// - Maintains dialog state in `self.plotter_preview`
    ///
    /// # Performance Considerations
    /// - Efficient preview rendering with minimal overhead
    /// - Optimized for real-time interaction feedback
    /// - Handles multiple concurrent dialogs efficiently
    pub(crate) fn render_plotter_preview_dialog(&mut self, ctx: &egui::Context) {
        if !self.plotter_preview.open {
            return;
        }

        let Some(plugin_id) = self.plotter_preview.target else {
            return;
        };
        let seed = self.build_plotter_preview_state(plugin_id);
        Self::sync_series_controls_from_seed(&mut self.plotter_preview, &seed);

        let mut save_requested = false;

        egui::Window::new("Plot Preview & Export")
            .resizable(true)
            .default_size(egui::vec2(600.0, 500.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Title:");
                    ui.text_edit_singleline(&mut self.plotter_preview.title);
                });

                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.plotter_preview.show_axes, "Show axes");
                    ui.checkbox(&mut self.plotter_preview.show_legend, "Show legend");
                    ui.checkbox(&mut self.plotter_preview.show_grid, "Show grid");
                    ui.checkbox(&mut self.plotter_preview.dark_theme, "Dark theme");
                });

                ui.horizontal(|ui| {
                    ui.label("X-axis:");
                    ui.text_edit_singleline(&mut self.plotter_preview.x_axis_name);
                    ui.label("Y-axis:");
                    ui.text_edit_singleline(&mut self.plotter_preview.y_axis_name);
                });
                Self::render_timebase_controls(
                    ui,
                    &mut self.plotter_preview.window_ms,
                    &mut self.plotter_preview.timebase_divisions,
                );

                ui.separator();
                ui.label("Series customization:");

                egui::ScrollArea::vertical()
                    .max_height(150.0)
                    .show(ui, |ui| {
                        while self.plotter_preview.series_scales.len()
                            < self.plotter_preview.series_names.len()
                        {
                            self.plotter_preview.series_scales.push(1.0);
                        }
                        while self.plotter_preview.series_offsets.len()
                            < self.plotter_preview.series_names.len()
                        {
                            self.plotter_preview.series_offsets.push(0.0);
                        }
                        for i in 0..self.plotter_preview.series_names.len() {
                            ui.horizontal(|ui| {
                                ui.label(format!("Series {}:", i + 1));
                                ui.text_edit_singleline(&mut self.plotter_preview.series_names[i]);
                                ui.menu_button("Tune", |ui| {
                                    ui.set_min_width(220.0);
                                    Self::render_series_wheels(
                                        ui,
                                        &mut self.plotter_preview.series_scales[i],
                                        &mut self.plotter_preview.series_offsets[i],
                                    );
                                });
                                ui.color_edit_button_srgba(&mut self.plotter_preview.colors[i]);
                            });
                        }
                    });

                ui.separator();

                // Preview area
                ui.label("Preview:");
                let preview_rect = ui.available_rect_before_wrap();
                let preview_size = egui::vec2(preview_rect.width(), 200.0);

                if let Some(plotter) = self.plotter_manager.plotters.get(&plugin_id) {
                    if let Ok(mut plotter) = plotter.lock() {
                        let series_transforms = Self::build_series_transforms(
                            &self.plotter_preview.series_scales,
                            &self.plotter_preview.series_offsets,
                            self.plotter_preview.series_names.len(),
                        );
                        ui.allocate_ui(preview_size, |ui| {
                            let input_count = plotter.input_count;
                            let refresh_hz = plotter.refresh_hz;
                            plotter.set_window_ms(self.plotter_preview.window_ms);
                            plotter.update_config(
                                input_count,
                                refresh_hz,
                                self.state_sync.logic_period_seconds,
                            );
                            plotter.render_with_settings(
                                ui,
                                "Preview",
                                &self.state_sync.logic_time_label,
                                self.plotter_preview.show_axes,
                                self.plotter_preview.show_legend,
                                self.plotter_preview.show_grid,
                                Some(&self.plotter_preview.title),
                                Some(&self.plotter_preview.series_names),
                                Some(&series_transforms),
                                Some(&self.plotter_preview.colors),
                                self.plotter_preview.dark_theme,
                                Some(&self.plotter_preview.x_axis_name),
                                Some(&self.plotter_preview.y_axis_name),
                                Some(self.plotter_preview.window_ms),
                            );
                        });
                    }
                }

                ui.separator();

                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.plotter_preview.export_svg, "Export as SVG");
                });

                ui.horizontal(|ui| {
                    ui.label("Resolution:");
                    let old_width = self.plotter_preview.width;
                    ui.add_enabled(
                        !self.plotter_preview.export_svg,
                        egui::DragValue::new(&mut self.plotter_preview.width)
                            .clamp_range(400..=4000)
                            .suffix("px"),
                    );

                    // Update height proportionally if width changed
                    if self.plotter_preview.width != old_width && !self.plotter_preview.export_svg {
                        let ratio = 16.0 / 9.0;
                        self.plotter_preview.height =
                            (self.plotter_preview.width as f32 / ratio) as u32;
                    }

                    ui.label("×");
                    let old_height = self.plotter_preview.height;
                    ui.add_enabled(
                        !self.plotter_preview.export_svg,
                        egui::DragValue::new(&mut self.plotter_preview.height)
                            .clamp_range(300..=3000)
                            .suffix("px"),
                    );

                    // Update width proportionally if height changed
                    if self.plotter_preview.height != old_height && !self.plotter_preview.export_svg
                    {
                        let ratio = 16.0 / 9.0;
                        self.plotter_preview.width =
                            (self.plotter_preview.height as f32 * ratio) as u32;
                    }

                    if styled_button(ui, "16:9").clicked() && !self.plotter_preview.export_svg {
                        let ratio = 16.0 / 9.0;
                        self.plotter_preview.height =
                            (self.plotter_preview.width as f32 / ratio) as u32;
                    }
                });

                ui.horizontal(|ui| {
                    if styled_button(ui, "Save").clicked() {
                        save_requested = true;
                    }
                    if styled_button(ui, "Cancel").clicked() {
                        self.plotter_preview.open = false;
                    }
                });
            });

        // Save settings only on explicit action
        if save_requested {
            if let Some(plugin_id) = self.plotter_preview.target {
                self.plotter_manager.plotter_preview_settings.insert(
                    plugin_id,
                    (
                        self.plotter_preview.show_axes,
                        self.plotter_preview.show_legend,
                        self.plotter_preview.show_grid,
                        self.plotter_preview.series_names.clone(),
                        self.plotter_preview.series_scales.clone(),
                        self.plotter_preview.series_offsets.clone(),
                        self.plotter_preview.colors.clone(),
                        self.plotter_preview.title.clone(),
                        self.plotter_preview.dark_theme,
                        self.plotter_preview.x_axis_name.clone(),
                        self.plotter_preview.y_axis_name.clone(),
                        self.plotter_preview.window_ms,
                        self.plotter_preview.timebase_divisions.clamp(1, 200),
                        self.plotter_preview.high_quality,
                        self.plotter_preview.export_svg,
                    ),
                );
            }
        }

        if save_requested {
            self.request_plotter_screenshot(plugin_id);
            // Keep dialog open after saving
        }
    }
}
