use super::*;
use crate::ui_state::PlotterPreviewState;
use std::time::Duration;

impl GuiApp {
    pub(crate) fn render_plotter_windows(&mut self, ctx: &egui::Context) {
        let mut closed = Vec::new();
        let mut export_saved: Vec<u64> = Vec::new();
        let mut settings_saved: Vec<u64> = Vec::new();
        let mut settings_closed: Vec<u64> = Vec::new();
        let name_by_id: HashMap<u64, String> = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .map(|plugin| (plugin.id, self.plugin_display_name(plugin.id)))
            .collect();
        let plotter_ids: Vec<u64> = self
            .plotter_manager
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
            let settings_seed = self.build_plotter_preview_state(plugin_id);
            let display_name = name_by_id
                .get(&plugin_id)
                .cloned()
                .unwrap_or_else(|| "plotter".to_string());
            let title = format!("Plotter #{} {}", plugin_id, display_name);
            let viewport_id = egui::ViewportId::from_hash_of(("plotter", plugin_id));
            let builder = egui::ViewportBuilder::default()
                .with_title(title.clone())
                .with_inner_size([900.0, 520.0])
                .with_close_button(false);

            let plotter = self
                .plotter_manager
                .plotters
                .get(&plugin_id)
                .cloned()
                .expect("plotter exists");
            let plotter_for_viewport = plotter.clone();
            let preview_settings = self
                .plotter_manager
                .plotter_preview_settings
                .get(&plugin_id)
                .cloned();
            let time_label = self.state_sync.logic_time_label.clone();

            ctx.show_viewport_deferred(viewport_id, builder, move |ctx, class| {
                if class == egui::ViewportClass::Embedded {
                    return;
                }

                egui::CentralPanel::default().show(ctx, |ui| {
                    if let Ok(mut plotter) = plotter_for_viewport.lock() {
                        let button_h = BUTTON_SIZE.y;
                        let gap_h = 6.0;
                        let plot_margin = 10.0;
                        let available = ui.available_size();
                        let plot_h = (available.y - button_h - gap_h).max(0.0);
                        let plot_rect = egui::Rect::from_min_size(
                            ui.min_rect().min,
                            egui::vec2(available.x, plot_h),
                        );
                        let inner_rect = plot_rect.shrink2(egui::vec2(plot_margin, plot_margin));
                        ui.allocate_ui_at_rect(inner_rect, |ui| {
                            if let Some((
                                show_axes,
                                show_legend,
                                show_grid,
                                series_names,
                                colors,
                                title,
                                dark_theme,
                                x_axis,
                                y_axis,
                                _high_quality,
                                _export_svg,
                            )) = preview_settings.clone()
                            {
                                plotter.render_with_settings(
                                    ui,
                                    "",
                                    &time_label,
                                    show_axes,
                                    show_legend,
                                    show_grid,
                                    Some(&title),
                                    Some(&series_names),
                                    Some(&colors),
                                    dark_theme,
                                    Some(&x_axis),
                                    Some(&y_axis),
                                );
                            } else {
                                plotter.render(ui, "", &time_label);
                            }
                        });
                        ui.allocate_space(egui::vec2(available.x, plot_h));
                        ui.add_space(gap_h);
                        let button_gap = 8.0;
                        let button_rect = egui::Rect::from_min_size(
                            egui::pos2(
                                plot_rect.right()
                                    - (BUTTON_SIZE.x * 2.0 + button_gap)
                                    - plot_margin,
                                plot_rect.bottom() + gap_h,
                            ),
                            egui::vec2(BUTTON_SIZE.x * 2.0 + button_gap, BUTTON_SIZE.y),
                        );
                        ui.allocate_ui_at_rect(button_rect, |ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let capture = styled_button(
                                        ui,
                                        egui::RichText::new("Capture").size(12.0),
                                    )
                                    .on_hover_text("Save plot image");
                                    if capture.clicked() {
                                        let export_open =
                                            egui::Id::new(("plotter_export_open", plugin_id));
                                        let export_state =
                                            egui::Id::new(("plotter_export_state", plugin_id));
                                        ctx.data_mut(|d| {
                                            d.insert_temp(export_open, true);
                                            if d.get_temp::<PlotterPreviewState>(export_state)
                                                .is_none()
                                            {
                                                d.insert_temp(export_state, settings_seed.clone());
                                            }
                                        });
                                    }
                                    let settings = styled_button(
                                        ui,
                                        egui::RichText::new("Settings").size(12.0),
                                    )
                                    .on_hover_text("Plot settings");
                                    if settings.clicked() {
                                        let open_id =
                                            egui::Id::new(("plotter_settings_open", plugin_id));
                                        let state_id =
                                            egui::Id::new(("plotter_settings_state", plugin_id));
                                        ctx.data_mut(|d| {
                                            d.insert_temp(open_id, true);
                                            if d.get_temp::<PlotterPreviewState>(state_id).is_none()
                                            {
                                                d.insert_temp(state_id, settings_seed.clone());
                                            }
                                        });
                                    }
                                },
                            );
                        });

                        let export_open = egui::Id::new(("plotter_export_open", plugin_id));
                        let export_state = egui::Id::new(("plotter_export_state", plugin_id));
                        let export_save = egui::Id::new(("plotter_export_save", plugin_id));
                        let export_close = egui::Id::new(("plotter_export_close", plugin_id));
                        let mut export_is_open =
                            ctx.data(|d| d.get_temp::<bool>(export_open).unwrap_or(false));
                        if export_is_open {
                            let mut state = ctx
                                .data(|d| d.get_temp::<PlotterPreviewState>(export_state))
                                .unwrap_or_else(|| settings_seed.clone());
                            let mut save_requested = false;
                            egui::Window::new("Plot Export")
                                .resizable(false)
                                .default_size(egui::vec2(360.0, 180.0))
                                .open(&mut export_is_open)
                                .show(ctx, |ui| {
                                    ui.checkbox(&mut state.export_svg, "Export as SVG");
                                    ui.horizontal(|ui| {
                                        ui.label("Resolution:");
                                        let old_width = state.width;
                                        ui.add_enabled(
                                            !state.export_svg,
                                            egui::DragValue::new(&mut state.width)
                                                .clamp_range(400..=4000)
                                                .suffix("px"),
                                        );
                                        if state.width != old_width && !state.export_svg {
                                            let ratio = 16.0 / 9.0;
                                            state.height = (state.width as f32 / ratio) as u32;
                                        }
                                        ui.label("×");
                                        let old_height = state.height;
                                        ui.add_enabled(
                                            !state.export_svg,
                                            egui::DragValue::new(&mut state.height)
                                                .clamp_range(300..=3000)
                                                .suffix("px"),
                                        );
                                        if state.height != old_height && !state.export_svg {
                                            let ratio = 16.0 / 9.0;
                                            state.width = (state.height as f32 * ratio) as u32;
                                        }
                                    });
                                    if styled_button(ui, "Save").clicked() {
                                        save_requested = true;
                                    }
                                });
                            if save_requested {
                                export_is_open = false;
                                ctx.data_mut(|d| d.insert_temp(export_save, true));
                            }
                            ctx.data_mut(|d| {
                                d.insert_temp(export_state, state);
                                d.insert_temp(export_open, export_is_open);
                                if !export_is_open {
                                    d.insert_temp(export_close, true);
                                }
                            });
                        }

                        let open_id = egui::Id::new(("plotter_settings_open", plugin_id));
                        let state_id = egui::Id::new(("plotter_settings_state", plugin_id));
                        let save_id = egui::Id::new(("plotter_settings_save", plugin_id));
                        let close_id = egui::Id::new(("plotter_settings_close", plugin_id));
                        let mut open = ctx.data(|d| d.get_temp::<bool>(open_id).unwrap_or(false));
                        if open {
                            let mut state = ctx
                                .data(|d| d.get_temp::<PlotterPreviewState>(state_id))
                                .unwrap_or_else(|| settings_seed.clone());
                            egui::Window::new("Plot Settings")
                                .resizable(false)
                                .default_size(egui::vec2(600.0, 500.0))
                                .open(&mut open)
                                .show(ctx, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label("Title:");
                                        ui.text_edit_singleline(&mut state.title);
                                    });
                                    ui.horizontal(|ui| {
                                        ui.checkbox(&mut state.show_axes, "Show axes");
                                        ui.checkbox(&mut state.show_legend, "Show legend");
                                        ui.checkbox(&mut state.show_grid, "Show grid");
                                        ui.checkbox(&mut state.dark_theme, "Dark theme");
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("X-axis:");
                                        ui.text_edit_singleline(&mut state.x_axis_name);
                                        ui.label("Y-axis:");
                                        ui.text_edit_singleline(&mut state.y_axis_name);
                                    });
                                    ui.separator();
                                    ui.label("Series customization:");
                                    egui::ScrollArea::vertical()
                                        .max_height(150.0)
                                        .show(ui, |ui| {
                                            for (i, (name, color)) in state
                                                .series_names
                                                .iter_mut()
                                                .zip(state.colors.iter_mut())
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
                                    let preview_rect = ui.available_rect_before_wrap();
                                    let preview_size = egui::vec2(preview_rect.width(), 200.0);
                                    ui.allocate_ui(preview_size, |ui| {
                                        plotter.render_with_settings(
                                            ui,
                                            "",
                                            &time_label,
                                            state.show_axes,
                                            state.show_legend,
                                            state.show_grid,
                                            Some(&state.title),
                                            Some(&state.series_names),
                                            Some(&state.colors),
                                            state.dark_theme,
                                            Some(&state.x_axis_name),
                                            Some(&state.y_axis_name),
                                        );
                                    });
                                    ui.separator();
                                    if styled_button(ui, "Apply").clicked() {
                                        ctx.data_mut(|d| d.insert_temp(save_id, true));
                                        ctx.request_repaint();
                                    }
                                });
                            ctx.data_mut(|d| {
                                d.insert_temp(state_id, state);
                                d.insert_temp(open_id, open);
                                if !open {
                                    d.insert_temp(close_id, true);
                                }
                            });
                        }
                        let refresh_hz = plotter.refresh_hz.max(1.0);
                        ctx.request_repaint_after(Duration::from_secs_f64(1.0 / refresh_hz));
                    }
                });
            });

            // Check for capture request
            if ctx.data(|d| {
                d.get_temp::<bool>(egui::Id::new(("plotter_export_save", plugin_id)))
                    .unwrap_or(false)
            }) {
                export_saved.push(plugin_id);
                ctx.data_mut(|d| {
                    d.remove::<bool>(egui::Id::new(("plotter_export_save", plugin_id)))
                });
            }
            if ctx.data(|d| {
                d.get_temp::<bool>(egui::Id::new(("plotter_settings_save", plugin_id)))
                    .unwrap_or(false)
            }) {
                settings_saved.push(plugin_id);
                ctx.data_mut(|d| {
                    d.remove::<bool>(egui::Id::new(("plotter_settings_save", plugin_id)))
                });
            }
            if ctx.data(|d| {
                d.get_temp::<bool>(egui::Id::new(("plotter_settings_close", plugin_id)))
                    .unwrap_or(false)
            }) {
                settings_closed.push(plugin_id);
                ctx.data_mut(|d| {
                    d.remove::<bool>(egui::Id::new(("plotter_settings_close", plugin_id)))
                });
            }

            if ctx.embed_viewports() {
                let response = egui::Window::new(title)
                    .resizable(true)
                    .default_size(egui::vec2(900.0, 520.0))
                    .show(ctx, |ui| {
                        if let Ok(mut plotter) = plotter.lock() {
                            if let Some((
                                show_axes,
                                show_legend,
                                show_grid,
                                series_names,
                                colors,
                                title,
                                dark_theme,
                                x_axis,
                                y_axis,
                                _high_quality,
                                _export_svg,
                            )) = self
                                .plotter_manager
                                .plotter_preview_settings
                                .get(&plugin_id)
                                .cloned()
                            {
                                plotter.render_with_settings(
                                    ui,
                                    "",
                                    &self.state_sync.logic_time_label,
                                    show_axes,
                                    show_legend,
                                    show_grid,
                                    Some(&title),
                                    Some(&series_names),
                                    Some(&colors),
                                    dark_theme,
                                    Some(&x_axis),
                                    Some(&y_axis),
                                );
                            } else {
                                plotter.render(ui, "", &self.state_sync.logic_time_label);
                            }
                        }
                    });
                if let Some(response) = response {
                    self.window_rects.push(response.response.rect);
                    if !self.confirm_dialog.open
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
            if let Some(plotter) = self.plotter_manager.plotters.get(&id) {
                if let Ok(mut plotter) = plotter.lock() {
                    plotter.open = false;
                }
            }
            // Stop the plugin when plot window closes
            if let Some(plugin) = self
                .workspace_manager
                .workspace
                .plugins
                .iter_mut()
                .find(|p| p.id == id)
            {
                if plugin.running {
                    plugin.running = false;
                    let _ = self
                        .state_sync
                        .logic_tx
                        .send(LogicMessage::SetPluginRunning(id, false));
                    self.mark_workspace_dirty();
                }
            }
        }

        for plugin_id in export_saved {
            let export_state = egui::Id::new(("plotter_export_state", plugin_id));
            if let Some(state) = ctx.data(|d| d.get_temp::<PlotterPreviewState>(export_state)) {
                self.plotter_preview = state.clone();
                self.apply_plotter_preview_state(plugin_id, &state);
                self.request_plotter_screenshot(plugin_id);
            }
        }
        for plugin_id in settings_saved.iter().copied() {
            let state_id = egui::Id::new(("plotter_settings_state", plugin_id));
            if let Some(state) = ctx.data(|d| d.get_temp::<PlotterPreviewState>(state_id)) {
                self.apply_plotter_preview_state(plugin_id, &state);
            }
        }
        for plugin_id in settings_closed.iter().copied() {
            let state_id = egui::Id::new(("plotter_settings_state", plugin_id));
            if let Some(state) = ctx.data(|d| d.get_temp::<PlotterPreviewState>(state_id)) {
                self.apply_plotter_preview_state(plugin_id, &state);
            }
        }
    }

    fn build_plotter_preview_state(&self, plugin_id: u64) -> PlotterPreviewState {
        let mut state = PlotterPreviewState::default();
        state.target = Some(plugin_id);
        if let Some((
            show_axes,
            show_legend,
            show_grid,
            series_names,
            colors,
            title,
            dark_theme,
            x_axis,
            y_axis,
            high_quality,
            export_svg,
        )) = self
            .plotter_manager
            .plotter_preview_settings
            .get(&plugin_id)
            .cloned()
        {
            state.show_axes = show_axes;
            state.show_legend = show_legend;
            state.show_grid = show_grid;
            state.series_names = series_names;
            state.colors = colors;
            state.title = title;
            state.dark_theme = dark_theme;
            state.x_axis_name = x_axis;
            state.y_axis_name = y_axis;
            state.high_quality = high_quality;
            state.export_svg = export_svg;
            return state;
        }
        let connected_plugin_names: Vec<String> = self
            .workspace_manager
            .workspace
            .connections
            .iter()
            .filter(|conn| conn.to_plugin == plugin_id)
            .filter_map(|conn| {
                self.workspace_manager
                    .workspace
                    .plugins
                    .iter()
                    .find(|p| p.id == conn.from_plugin)
                    .map(|p| self.plugin_display_name(p.id))
            })
            .collect();
        if let Some(plotter) = self.plotter_manager.plotters.get(&plugin_id) {
            if let Ok(plotter) = plotter.lock() {
                state.show_axes = true;
                state.show_legend = true;
                state.show_grid = true;
                state.title = String::new();
                state.dark_theme = true;
                state.x_axis_name = self.state_sync.logic_time_label.clone();
                state.y_axis_name = "value".to_string();
                state.high_quality = false;
                state.series_names = (0..plotter.input_count)
                    .map(|i| {
                        connected_plugin_names
                            .get(i)
                            .cloned()
                            .unwrap_or_else(|| format!("Series {}", i + 1))
                    })
                    .collect();
                state.colors = (0..plotter.input_count)
                    .map(|i| match i % 8 {
                        0 => egui::Color32::from_rgb(86, 156, 214),
                        1 => egui::Color32::from_rgb(220, 122, 95),
                        2 => egui::Color32::from_rgb(181, 206, 168),
                        3 => egui::Color32::from_rgb(220, 220, 170),
                        4 => egui::Color32::from_rgb(197, 134, 192),
                        5 => egui::Color32::from_rgb(78, 201, 176),
                        6 => egui::Color32::from_rgb(156, 220, 254),
                        _ => egui::Color32::from_rgb(255, 206, 84),
                    })
                    .collect();
            }
        }
        state
    }

    fn apply_plotter_preview_state(&mut self, plugin_id: u64, state: &PlotterPreviewState) {
        self.plotter_manager.plotter_preview_settings.insert(
            plugin_id,
            (
                state.show_axes,
                state.show_legend,
                state.show_grid,
                state.series_names.clone(),
                state.colors.clone(),
                state.title.clone(),
                state.dark_theme,
                state.x_axis_name.clone(),
                state.y_axis_name.clone(),
                state.high_quality,
                state.export_svg,
            ),
        );
    }

    pub(crate) fn render_plotter_preview_dialog(&mut self, ctx: &egui::Context) {
        if !self.plotter_preview.open {
            return;
        }

        let Some(plugin_id) = self.plotter_preview.target else {
            return;
        };

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

                ui.separator();
                ui.label("Series customization:");

                egui::ScrollArea::vertical()
                    .max_height(150.0)
                    .show(ui, |ui| {
                        for (i, (name, color)) in self
                            .plotter_preview
                            .series_names
                            .iter_mut()
                            .zip(self.plotter_preview.colors.iter_mut())
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

                if let Some(plotter) = self.plotter_manager.plotters.get(&plugin_id) {
                    if let Ok(mut plotter) = plotter.lock() {
                        ui.allocate_ui(preview_size, |ui| {
                            plotter.render_with_settings(
                                ui,
                                "Preview",
                                &self.state_sync.logic_time_label,
                                self.plotter_preview.show_axes,
                                self.plotter_preview.show_legend,
                                self.plotter_preview.show_grid,
                                Some(&self.plotter_preview.title),
                                Some(&self.plotter_preview.series_names),
                                Some(&self.plotter_preview.colors),
                                self.plotter_preview.dark_theme,
                                Some(&self.plotter_preview.x_axis_name),
                                Some(&self.plotter_preview.y_axis_name),
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

        // Save settings when dialog closes
        if save_requested || !self.plotter_preview.open {
            if let Some(plugin_id) = self.plotter_preview.target {
                self.plotter_manager.plotter_preview_settings.insert(
                    plugin_id,
                    (
                        self.plotter_preview.show_axes,
                        self.plotter_preview.show_legend,
                        self.plotter_preview.show_grid,
                        self.plotter_preview.series_names.clone(),
                        self.plotter_preview.colors.clone(),
                        self.plotter_preview.title.clone(),
                        self.plotter_preview.dark_theme,
                        self.plotter_preview.x_axis_name.clone(),
                        self.plotter_preview.y_axis_name.clone(),
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
