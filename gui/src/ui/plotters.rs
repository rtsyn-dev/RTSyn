use super::*;
use std::time::Duration;

impl GuiApp {
    pub(crate) fn render_plotter_windows(&mut self, ctx: &egui::Context) {
        let mut closed = Vec::new();
        let mut capture_requested = Vec::new();
        let name_by_id: HashMap<u64, String> = self
            .workspace
            .plugins
            .iter()
            .map(|plugin| (plugin.id, self.plugin_display_name(plugin.id)))
            .collect();
        let plotter_ids: Vec<u64> = self
            .plotters
            .iter()
            .filter_map(|(id, plotter)| {
                plotter
                    .lock()
                    .ok()
                    .and_then(|plotter| if plotter.open { Some(*id) } else { None })
            })
            .collect();

        for plugin_id in plotter_ids {
            let display_name = name_by_id
                .get(&plugin_id)
                .cloned()
                .unwrap_or_else(|| "plotter".to_string());
            let title = format!("Plotter #{} {}", plugin_id, display_name);
            let viewport_id = egui::ViewportId::from_hash_of(("plotter", plugin_id));
            let builder = egui::ViewportBuilder::default()
                .with_title(title.clone())
                .with_inner_size([900.0, 520.0]);

            let plotter = self
                .plotters
                .get(&plugin_id)
                .cloned()
                .expect("plotter exists");
            let plotter_for_viewport = plotter.clone();
            let time_label = self.logic_time_label.clone();

            ctx.show_viewport_deferred(viewport_id, builder, move |ctx, class| {
                if class == egui::ViewportClass::Embedded {
                    return;
                }
                egui::TopBottomPanel::top("plotter_toolbar").show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("📷 Capture").clicked() {
                            ctx.data_mut(|d| d.insert_temp(egui::Id::new(("capture_request", plugin_id)), true));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("✕").clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        });
                    });
                });
                egui::CentralPanel::default().show(ctx, |ui| {
                    if let Ok(mut plotter) = plotter_for_viewport.lock() {
                        let label = format!("Inputs: {}", plotter.input_count);
                        plotter.render(ui, &label, &time_label);
                        let refresh_hz = plotter.refresh_hz.max(1.0);
                        ctx.request_repaint_after(Duration::from_secs_f64(1.0 / refresh_hz));
                    }
                });
            });

            // Check for capture request
            if ctx.data(|d| d.get_temp::<bool>(egui::Id::new(("capture_request", plugin_id))).unwrap_or(false)) {
                capture_requested.push(plugin_id);
                ctx.data_mut(|d| d.remove::<bool>(egui::Id::new(("capture_request", plugin_id))));
            }

            if ctx.embed_viewports() {
                let response = egui::Window::new(title)
                    .resizable(true)
                    .default_size(egui::vec2(900.0, 520.0))
                    .show(ctx, |ui| {
                        if let Ok(mut plotter) = plotter.lock() {
                            let label = format!("Inputs: {}", plotter.input_count);
                            plotter.render(ui, &label, &self.logic_time_label);
                        }
                    });
                if let Some(response) = response {
                    self.window_rects.push(response.response.rect);
                    if !self.confirm_dialog_open
                        && (response.response.clicked() || response.response.dragged())
                    {
                        ctx.move_to_top(response.response.layer_id);
                    }
                }
            }

            let close_requested = ctx.input_for(viewport_id, |i| i.viewport().close_requested());
            if close_requested {
                closed.push(plugin_id);
            }
        }

        for id in closed {
            if let Some(plotter) = self.plotters.get(&id) {
                if let Ok(mut plotter) = plotter.lock() {
                    plotter.open = false;
                }
            }
            // Stop the plugin when plot window closes
            if let Some(plugin) = self.workspace.plugins.iter_mut().find(|p| p.id == id) {
                if plugin.running {
                    plugin.running = false;
                    let _ = self.logic_tx.send(LogicMessage::SetPluginRunning(id, false));
                    self.mark_workspace_dirty();
                }
            }
        }

        // Handle capture requests
        for plugin_id in capture_requested {
            self.open_plotter_preview(plugin_id);
        }
    }

    fn open_plotter_preview(&mut self, plugin_id: u64) {
        self.plotter_preview_target = Some(plugin_id);
        self.plotter_preview_open = true;
        
        // Load existing settings or create defaults
        if let Some((show_axes, show_legend, show_grid, series_names, colors, title, dark_theme, x_axis, y_axis, high_quality, export_svg)) = self.plotter_preview_settings.get(&plugin_id).cloned() {
            self.plotter_preview_show_axes = show_axes;
            self.plotter_preview_show_legend = show_legend;
            self.plotter_preview_show_grid = show_grid;
            self.plotter_preview_series_names = series_names;
            self.plotter_preview_colors = colors;
            self.plotter_preview_title = title;
            self.plotter_preview_dark_theme = dark_theme;
            self.plotter_preview_x_axis_name = x_axis;
            self.plotter_preview_y_axis_name = y_axis;
            self.plotter_preview_high_quality = high_quality;
            self.plotter_preview_export_svg = export_svg;
        } else {
            // Initialize default settings - find connected plugin names
            let connected_plugin_names: Vec<String> = self.workspace.connections
                .iter()
                .filter(|conn| conn.to_plugin == plugin_id)
                .filter_map(|conn| {
                    self.workspace.plugins
                        .iter()
                        .find(|p| p.id == conn.from_plugin)
                        .map(|p| self.plugin_display_name(p.id))
                })
                .collect();
                
            if let Some(plotter) = self.plotters.get(&plugin_id) {
                if let Ok(plotter) = plotter.lock() {
                    self.plotter_preview_show_axes = true;
                    self.plotter_preview_show_legend = true;
                    self.plotter_preview_show_grid = true;
                    self.plotter_preview_title = String::new(); // Empty by default
                    self.plotter_preview_dark_theme = true;
                    self.plotter_preview_x_axis_name = self.logic_time_label.clone();
                    self.plotter_preview_y_axis_name = "value".to_string();
                    self.plotter_preview_high_quality = false;
                    
                    self.plotter_preview_series_names = (0..plotter.input_count)
                        .map(|i| {
                            connected_plugin_names.get(i)
                                .cloned()
                                .unwrap_or_else(|| format!("Series {}", i + 1))
                        })
                        .collect();
                    self.plotter_preview_colors = (0..plotter.input_count)
                        .map(|i| {
                            // Use the same palette as live plotter
                            match i % 8 {
                                0 => egui::Color32::from_rgb(86, 156, 214),
                                1 => egui::Color32::from_rgb(220, 122, 95),
                                2 => egui::Color32::from_rgb(181, 206, 168),
                                3 => egui::Color32::from_rgb(220, 220, 170),
                                4 => egui::Color32::from_rgb(197, 134, 192),
                                5 => egui::Color32::from_rgb(78, 201, 176),
                                6 => egui::Color32::from_rgb(156, 220, 254),
                                _ => egui::Color32::from_rgb(255, 206, 84),
                            }
                        })
                        .collect();
                }
            }
        }
    }

    pub(crate) fn render_plotter_preview_dialog(&mut self, ctx: &egui::Context) {
        if !self.plotter_preview_open {
            return;
        }

        let Some(plugin_id) = self.plotter_preview_target else {
            return;
        };

        let mut save_requested = false;
        
        egui::Window::new("Plot Preview & Export")
            .resizable(true)
            .default_size(egui::vec2(600.0, 500.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Title:");
                    ui.text_edit_singleline(&mut self.plotter_preview_title);
                });
                
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.plotter_preview_show_axes, "Show axes");
                    ui.checkbox(&mut self.plotter_preview_show_legend, "Show legend");
                    ui.checkbox(&mut self.plotter_preview_show_grid, "Show grid");
                    ui.checkbox(&mut self.plotter_preview_dark_theme, "Dark theme");
                });

                ui.horizontal(|ui| {
                    ui.label("X-axis:");
                    ui.text_edit_singleline(&mut self.plotter_preview_x_axis_name);
                    ui.label("Y-axis:");
                    ui.text_edit_singleline(&mut self.plotter_preview_y_axis_name);
                });

                ui.separator();
                ui.label("Series customization:");

                egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                    for (i, (name, color)) in self.plotter_preview_series_names
                        .iter_mut()
                        .zip(self.plotter_preview_colors.iter_mut())
                        .enumerate()
                    {
                        ui.horizontal(|ui| {
                            ui.label(format!("Series {}:", i + 1));
                            ui.text_edit_singleline(name);
                            ui.color_edit_button_srgba(color);
                        });
                    }
                });

                ui.separator();

                // Preview area
                ui.label("Preview:");
                let preview_rect = ui.available_rect_before_wrap();
                let preview_size = egui::vec2(preview_rect.width(), 200.0);
                
                if let Some(plotter) = self.plotters.get(&plugin_id) {
                    if let Ok(mut plotter) = plotter.lock() {
                        ui.allocate_ui(preview_size, |ui| {
                            plotter.render_with_settings(
                                ui, 
                                "Preview", 
                                &self.logic_time_label,
                                self.plotter_preview_show_axes,
                                self.plotter_preview_show_legend,
                                self.plotter_preview_show_grid,
                                Some(&self.plotter_preview_title),
                                Some(&self.plotter_preview_series_names),
                                Some(&self.plotter_preview_colors),
                                self.plotter_preview_dark_theme,
                                Some(&self.plotter_preview_x_axis_name),
                                Some(&self.plotter_preview_y_axis_name),
                            );
                        });
                    }
                }

                ui.separator();

                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.plotter_preview_export_svg, "Export as SVG");
                });
                
                ui.horizontal(|ui| {
                    ui.label("Resolution:");
                    let old_width = self.plotter_preview_width;
                    ui.add_enabled(!self.plotter_preview_export_svg, 
                        egui::DragValue::new(&mut self.plotter_preview_width).clamp_range(400..=4000).suffix("px"));
                    
                    // Update height proportionally if width changed
                    if self.plotter_preview_width != old_width && !self.plotter_preview_export_svg {
                        let ratio = 16.0 / 9.0;
                        self.plotter_preview_height = (self.plotter_preview_width as f32 / ratio) as u32;
                    }
                    
                    ui.label("×");
                    let old_height = self.plotter_preview_height;
                    ui.add_enabled(!self.plotter_preview_export_svg, 
                        egui::DragValue::new(&mut self.plotter_preview_height).clamp_range(300..=3000).suffix("px"));
                    
                    // Update width proportionally if height changed
                    if self.plotter_preview_height != old_height && !self.plotter_preview_export_svg {
                        let ratio = 16.0 / 9.0;
                        self.plotter_preview_width = (self.plotter_preview_height as f32 * ratio) as u32;
                    }
                    
                    if ui.button("16:9").clicked() && !self.plotter_preview_export_svg {
                        let ratio = 16.0 / 9.0;
                        self.plotter_preview_height = (self.plotter_preview_width as f32 / ratio) as u32;
                    }
                });

                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        save_requested = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.plotter_preview_open = false;
                    }
                });
            });

        // Save settings when dialog closes
        if save_requested || !self.plotter_preview_open {
            if let Some(plugin_id) = self.plotter_preview_target {
                self.plotter_preview_settings.insert(
                    plugin_id,
                    (
                        self.plotter_preview_show_axes,
                        self.plotter_preview_show_legend,
                        self.plotter_preview_show_grid,
                        self.plotter_preview_series_names.clone(),
                        self.plotter_preview_colors.clone(),
                        self.plotter_preview_title.clone(),
                        self.plotter_preview_dark_theme,
                        self.plotter_preview_x_axis_name.clone(),
                        self.plotter_preview_y_axis_name.clone(),
                        self.plotter_preview_high_quality,
                        self.plotter_preview_export_svg,
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
