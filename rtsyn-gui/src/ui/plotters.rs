use super::*;
use crate::ui_state::PlotterPreviewState;
use std::time::Duration;

impl GuiApp {
    fn truncate_tab_label(name: &str, max_chars: usize) -> String {
        let mut out = String::new();
        for ch in name.chars().take(max_chars) {
            out.push(ch);
        }
        if name.chars().count() > max_chars {
            out.push_str("...");
        }
        out
    }

    fn knob_control(
        ui: &mut egui::Ui,
        label: &str,
        value: &mut f64,
        min: f64,
        max: f64,
        sensitivity: f64,
        _decimals: usize,
    ) {
        ui.vertical_centered(|ui| {
            ui.add_space(2.0);
            ui.label(egui::RichText::new(label).strong());
            let size = egui::vec2(96.0, 96.0);
            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::drag());
            let center = rect.center();
            let radius = rect.width().min(rect.height()) * 0.46;
            let start = -std::f32::consts::PI * 0.75;
            let end = std::f32::consts::PI * 0.75;

            let origin_key = response.id.with("drag_origin");
            let dx_key = response.id.with("drag_dx");
            if response.drag_started() {
                if let Some(pos) = response.interact_pointer_pos() {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(origin_key, pos);
                        d.insert_temp(dx_key, 0.0_f32);
                    });
                }
            }
            if response.dragged() {
                let pointer_pos = response
                    .interact_pointer_pos()
                    .or_else(|| ui.ctx().input(|i| i.pointer.latest_pos()));
                if let Some(pos) = pointer_pos {
                    let origin = ui
                        .ctx()
                        .data(|d| d.get_temp::<egui::Pos2>(origin_key))
                        .unwrap_or(pos);
                    let dx = pos.x - origin.x;
                    ui.ctx().data_mut(|d| d.insert_temp(dx_key, dx));
                    let deadzone = 2.0_f32;
                    if dx.abs() > deadzone {
                        let dir = dx.signum() as f64;
                        let strength = (dx.abs() - deadzone) as f64;
                        // Very fine control near the knob, progressively faster farther away.
                        let t = (strength / 120.0).clamp(0.0, 1.0);
                        let ramp = (0.03 + t.powf(2.2) * 18.0).clamp(0.03, 18.0);
                        let dt = ui.ctx().input(|i| i.stable_dt).max(1.0 / 240.0) as f64;
                        let fine = if ui.ctx().input(|i| i.modifiers.shift) {
                            0.2
                        } else {
                            1.0
                        };
                        *value =
                            (*value + dir * sensitivity * ramp * dt * 60.0 * fine).clamp(min, max);
                        ui.ctx().request_repaint();
                    }
                } else {
                    let dx = ui.ctx().data(|d| d.get_temp::<f32>(dx_key)).unwrap_or(0.0);
                    let deadzone = 2.0_f32;
                    if dx.abs() > deadzone {
                        let dir = dx.signum() as f64;
                        let strength = (dx.abs() - deadzone) as f64;
                        let t = (strength / 120.0).clamp(0.0, 1.0);
                        let ramp = (0.03 + t.powf(2.2) * 18.0).clamp(0.03, 18.0);
                        let dt = ui.ctx().input(|i| i.stable_dt).max(1.0 / 240.0) as f64;
                        let fine = if ui.ctx().input(|i| i.modifiers.shift) {
                            0.2
                        } else {
                            1.0
                        };
                        *value =
                            (*value + dir * sensitivity * ramp * dt * 60.0 * fine).clamp(min, max);
                        ui.ctx().request_repaint();
                    }
                }
            }
            if response.drag_stopped() {
                ui.ctx().data_mut(|d| {
                    d.remove::<egui::Pos2>(origin_key);
                    d.remove::<f32>(dx_key);
                });
            }

            if response.hovered() {
                let wheel = ui.ctx().input(|i| i.smooth_scroll_delta.y);
                if wheel.abs() > f32::EPSILON {
                    let strength = wheel.abs() as f64;
                    let ramp = (1.0 + (strength / 4.0).powf(1.1)).clamp(1.0, 10.0);
                    let dir = wheel.signum() as f64;
                    let fine = if ui.ctx().input(|i| i.modifiers.shift) {
                        0.2
                    } else {
                        1.0
                    };
                    *value = (*value + dir * sensitivity * 0.6 * ramp * fine).clamp(min, max);
                    ui.ctx().request_repaint();
                }
            }

            let t = if max > min {
                ((*value - min) / (max - min)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let angle = start + (end - start) * t as f32;
            let tip = center + egui::vec2(angle.cos(), angle.sin()) * (radius * 0.75);

            let stroke = egui::Stroke::new(1.25, ui.visuals().widgets.active.bg_fill);
            ui.painter()
                .circle_filled(center, radius, ui.visuals().widgets.inactive.bg_fill);
            ui.painter().circle_stroke(center, radius, stroke);
            ui.painter().line_segment(
                [center, tip],
                egui::Stroke::new(2.0, ui.visuals().widgets.active.fg_stroke.color),
            );
            ui.add_space(2.0);
        });
    }

    fn wheel_value_box(
        ui: &mut egui::Ui,
        title: &str,
        value: &mut f64,
        decimals: usize,
        min: f64,
        max: f64,
    ) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_width(220.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new(title).strong());
                ui.add_space(4.0);
                let mut text = format!("{:.*}", decimals, *value);
                ui.scope(|ui| {
                    ui.style_mut().visuals.widgets.inactive.bg_fill = egui::Color32::from_gray(45);
                    ui.style_mut().visuals.widgets.hovered.bg_fill = egui::Color32::from_gray(55);
                    ui.style_mut().visuals.widgets.active.bg_fill = egui::Color32::from_gray(60);
                    if ui
                        .add_sized(
                            [180.0, 0.0],
                            egui::TextEdit::singleline(&mut text)
                                .horizontal_align(egui::Align::Center),
                        )
                        .changed()
                    {
                        if let Ok(parsed) = text.trim().parse::<f64>() {
                            *value = parsed.clamp(min, max);
                        }
                    }
                });
            });
        });
    }

    fn step_125(value: f64, direction: i32) -> f64 {
        let seq = [1.0_f64, 2.0, 5.0];
        let mut v = value.max(0.1);
        let mut exp = v.log10().floor() as i32;
        let decade = 10_f64.powi(exp);
        v /= decade;

        let mut idx = 0usize;
        let mut best = f64::INFINITY;
        for (i, candidate) in seq.iter().enumerate() {
            let d = (v - *candidate).abs();
            if d < best {
                best = d;
                idx = i;
            }
        }

        if direction > 0 {
            if idx + 1 < seq.len() {
                idx += 1;
            } else {
                idx = 0;
                exp += 1;
            }
        } else if direction < 0 {
            if idx > 0 {
                idx -= 1;
            } else {
                idx = seq.len() - 1;
                exp -= 1;
            }
        }

        (seq[idx] * 10_f64.powi(exp)).clamp(0.1, 60_000.0)
    }

    fn render_timebase_controls(
        ui: &mut egui::Ui,
        window_ms: &mut f64,
        timebase_divisions: &mut u32,
    ) {
        let mut divisions = (*timebase_divisions).clamp(1, 200);
        let mut ms_div = (*window_ms / divisions as f64).clamp(0.1, 60_000.0);
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.label("Timebase:");

            if ui.small_button("◀").clicked() {
                ms_div = Self::step_125(ms_div, -1);
                changed = true;
            }
            if ui.small_button("▶").clicked() {
                ms_div = Self::step_125(ms_div, 1);
                changed = true;
            }

            let mut drag = ms_div;
            if ui
                .add(
                    egui::DragValue::new(&mut drag)
                        .clamp_range(0.1..=60_000.0)
                        .speed((ms_div * 0.05).max(0.1)),
                )
                .changed()
            {
                ms_div = drag.clamp(0.1, 60_000.0);
                changed = true;
            }

            ui.label("ms/div");
            ui.label("x");
            let mut div_drag = i64::from(divisions);
            if ui
                .add(
                    egui::DragValue::new(&mut div_drag)
                        .clamp_range(1..=200)
                        .speed(1.0),
                )
                .changed()
            {
                divisions = div_drag.clamp(1, 200) as u32;
                changed = true;
            }
            ui.label("div");
            ui.separator();
            ui.monospace(format!("{:.1} ms", ms_div * divisions as f64));
        });

        if changed {
            *timebase_divisions = divisions;
            *window_ms = (ms_div * divisions as f64).clamp(100.0, 600_000.0);
        }
    }

    fn render_series_wheels(ui: &mut egui::Ui, scale: &mut f64, offset: &mut f64) {
        Self::knob_control(ui, "Scale", scale, 0.001, 1_000_000.0, 0.08, 3);
        Self::knob_control(
            ui,
            "Offset",
            offset,
            -1_000_000_000.0,
            1_000_000_000.0,
            1.0,
            1,
        );
        if *scale <= 0.0 {
            *scale = 0.001;
        }
    }

    fn is_placeholder_series_name(name: &str) -> bool {
        let trimmed = name.trim();
        if !trimmed.starts_with("Series ") {
            return false;
        }
        trimmed["Series ".len()..].parse::<usize>().is_ok()
    }

    fn sync_series_controls_from_seed(state: &mut PlotterPreviewState, seed: &PlotterPreviewState) {
        let target = seed.series_names.len();
        if state.series_names.len() < target {
            for idx in state.series_names.len()..target {
                state.series_names.push(
                    seed.series_names
                        .get(idx)
                        .cloned()
                        .unwrap_or_else(|| format!("Series {}", idx + 1)),
                );
                state.series_scales.push(1.0);
                state.series_offsets.push(0.0);
                state.colors.push(
                    seed.colors
                        .get(idx)
                        .copied()
                        .unwrap_or(egui::Color32::from_rgb(86, 156, 214)),
                );
            }
        } else {
            state.series_names.truncate(target);
            state.series_scales.truncate(target);
            state.series_offsets.truncate(target);
            state.colors.truncate(target);
        }
        // Keep user-defined custom labels, but replace placeholder fallback labels
        // with concrete seed names once real connections/series metadata is available.
        for idx in 0..target {
            if let (Some(current), Some(seed_name)) =
                (state.series_names.get_mut(idx), seed.series_names.get(idx))
            {
                if (current.is_empty() || Self::is_placeholder_series_name(current))
                    && !seed_name.is_empty()
                {
                    *current = seed_name.clone();
                }
            }
        }
    }

    fn aligned_series_names(&self, plugin_id: u64, count: usize) -> Vec<String> {
        (0..count)
            .map(|i| {
                let port = format!("in_{i}");
                self.workspace_manager
                    .workspace
                    .connections
                    .iter()
                    .find(|c| c.to_plugin == plugin_id && c.to_port == port)
                    .map(|c| {
                        format!(
                            "{}:{}",
                            self.plugin_display_name(c.from_plugin),
                            c.from_port
                        )
                    })
                    .unwrap_or_else(|| format!("Series {}", i + 1))
            })
            .collect()
    }

    fn render_plotter_notifications(&mut self, ctx: &egui::Context, plugin_id: u64) {
        let Some(list) = self.notification_handler.get_plugin_notifications(plugin_id) else {
            return;
        };
        if list.is_empty() {
            return;
        }

        let now = std::time::Instant::now();
        let total = 2.8_f32;
        let max_width = 360.0;
        let mut y = ctx.screen_rect().min.y + 12.0;
        let x = ctx.screen_rect().max.x - 10.0;
        let mut shown = 0usize;

        for (idx, notification) in list.iter().enumerate() {
            let age = now.duration_since(notification.created_at).as_secs_f32();
            if age >= total {
                continue;
            }
            let slide_in = 0.35_f32;
            let slide_out = 0.45_f32;
            let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
            let slide = if age < slide_in {
                smooth((age / slide_in).clamp(0.0, 1.0))
            } else if age > total - slide_out {
                smooth(((total - age) / slide_out).clamp(0.0, 1.0))
            } else {
                1.0
            };

            let offscreen = max_width + 24.0;
            let x_pos = x + (1.0 - slide) * offscreen;
            egui::Area::new(egui::Id::new(("plotter_toast", plugin_id, idx)))
                .order(egui::Order::Foreground)
                .interactable(false)
                .pivot(egui::Align2::RIGHT_TOP)
                .fixed_pos(egui::pos2(x_pos, y))
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style())
                        .fill(egui::Color32::from_rgba_premultiplied(20, 20, 20, 220))
                        .stroke(egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgba_premultiplied(80, 80, 80, 220),
                        ))
                        .rounding(egui::Rounding::same(6.0))
                        .show(ui, |ui| {
                            ui.set_max_width(max_width);
                            ui.label(egui::RichText::new(&notification.title).strong().size(14.0));
                            ui.label(egui::RichText::new(&notification.message).size(13.0));
                        });
                });
            y += 62.0;
            shown += 1;
            if shown >= 4 {
                break;
            }
        }

        // Cleanup is handled by NotificationHandler
        if !list.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn toggle_plugin_running_from_plotter_window(
        &mut self,
        plugin_id: u64,
    ) -> Result<bool, String> {
        let plugin_index = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .position(|plugin| plugin.id == plugin_id)
            .ok_or_else(|| "Plugin not found".to_string())?;

        let plugin_kind = self.workspace_manager.workspace.plugins[plugin_index]
            .kind
            .clone();
        let currently_running = self.workspace_manager.workspace.plugins[plugin_index].running;
        self.ensure_plugin_behavior_cached(&plugin_kind);
        let behavior = self
            .behavior_manager
            .cached_behaviors
            .get(&plugin_kind)
            .cloned()
            .unwrap_or_default();

        if !behavior.supports_start_stop {
            return Err("Plugin does not support start/stop.".to_string());
        }

        if !currently_running {
            let connected_inputs: std::collections::HashSet<String> = self
                .workspace_manager
                .workspace
                .connections
                .iter()
                .filter(|conn| conn.to_plugin == plugin_id)
                .map(|conn| conn.to_port.clone())
                .collect();
            let connected_outputs: std::collections::HashSet<String> = self
                .workspace_manager
                .workspace
                .connections
                .iter()
                .filter(|conn| conn.from_plugin == plugin_id)
                .map(|conn| conn.from_port.clone())
                .collect();

            let missing_inputs: Vec<String> = behavior
                .start_requires_connected_inputs
                .iter()
                .filter(|port| !connected_inputs.contains(*port))
                .cloned()
                .collect();
            if !missing_inputs.is_empty() {
                return Err(format!(
                    "Cannot start: missing input connections on ports: {}",
                    missing_inputs.join(", ")
                ));
            }

            let missing_outputs: Vec<String> = behavior
                .start_requires_connected_outputs
                .iter()
                .filter(|port| !connected_outputs.contains(*port))
                .cloned()
                .collect();
            if !missing_outputs.is_empty() {
                return Err(format!(
                    "Cannot start: missing output connections on ports: {}",
                    missing_outputs.join(", ")
                ));
            }
        }

        let new_running = !currently_running;
        
        self.workspace_manager.workspace.plugins[plugin_index].running = new_running;
        let _ = self
            .state_sync
            .logic_tx
            .send(LogicMessage::SetPluginRunning(plugin_id, new_running));
        self.mark_workspace_dirty();

        if self.plugin_uses_plotter_viewport(&plugin_kind) && new_running {
            if let Some(plotter) = self.plotter_manager.plotters.get(&plugin_id) {
                if let Ok(mut plotter) = plotter.lock() {
                    plotter.open = true;
                }
            }
            self.recompute_plotter_ui_hz();
        }

        Ok(new_running)
    }

    pub(crate) fn render_plotter_windows(&mut self, ctx: &egui::Context) {
        let mut closed = Vec::new();
        let mut export_saved: Vec<u64> = Vec::new();
        let mut settings_saved: Vec<u64> = Vec::new();
        let mut start_toggle_requested: Vec<u64> = Vec::new();
        let mut add_connection_requested: Vec<u64> = Vec::new();
        let mut remove_connection_requested: Vec<u64> = Vec::new();
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
                    .and_then(|plotter| {
                        if plotter.open { Some(*id) } else { None }
                    })
            })
            .collect();
        

        for plugin_id in plotter_ids {
            let settings_seed = self.build_plotter_preview_state(plugin_id);
            let plugin_running = self
                .workspace_manager
                .workspace
                .plugins
                .iter()
                .find(|plugin| plugin.id == plugin_id)
                .map(|plugin| plugin.running)
                .unwrap_or(false);
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
            let logic_period_seconds = self.state_sync.logic_period_seconds;

            ctx.show_viewport_immediate(viewport_id, builder, |ctx, class| {
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
                                series_scales,
                                series_offsets,
                                colors,
                                title,
                                dark_theme,
                                x_axis,
                                y_axis,
                                window_ms,
                                _timebase_divisions,
                                _high_quality,
                                _export_svg,
                            )) = preview_settings.clone()
                            {
                                let series_transforms = Self::build_series_transforms(
                                    &series_scales,
                                    &series_offsets,
                                    series_names.len(),
                                );
                                plotter.render_with_settings(
                                    ui,
                                    "",
                                    &time_label,
                                    show_axes,
                                    show_legend,
                                    show_grid,
                                    Some(&title),
                                    Some(&series_names),
                                    Some(&series_transforms),
                                    Some(&colors),
                                    dark_theme,
                                    Some(&x_axis),
                                    Some(&y_axis),
                                    Some(window_ms),
                                );
                            } else {
                                plotter.render(ui, "", &time_label);
                            }
                        });
                        ui.allocate_space(egui::vec2(available.x, plot_h));
                        ui.add_space(gap_h);
                        let button_rect = egui::Rect::from_min_size(
                            egui::pos2(plot_rect.left() + plot_margin, plot_rect.bottom() + gap_h),
                            egui::vec2(
                                (plot_rect.width() - plot_margin * 2.0).max(0.0),
                                BUTTON_SIZE.y,
                            ),
                        );
                        ui.allocate_ui_at_rect(button_rect, |ui| {
                            ui.horizontal(|ui| {
                                let start_label = if plugin_running { "Stop" } else { "Start" };
                                if styled_button(ui, egui::RichText::new(start_label).size(12.0))
                                    .on_hover_text("Start/stop this live plotter plugin")
                                    .clicked()
                                {
                                    ctx.data_mut(|d| {
                                        d.insert_temp(
                                            egui::Id::new(("plotter_start_toggle", plugin_id)),
                                            true,
                                        )
                                    });
                                }
                                if styled_button(
                                    ui,
                                    egui::RichText::new("Add connections").size(12.0),
                                )
                                .on_hover_text("Add connections for this live plotter")
                                .clicked()
                                {
                                    ctx.data_mut(|d| {
                                        d.insert_temp(
                                            egui::Id::new(("plotter_add_connections", plugin_id)),
                                            true,
                                        )
                                    });
                                }
                                if styled_button(
                                    ui,
                                    egui::RichText::new("Remove connections").size(12.0),
                                )
                                .on_hover_text("Remove connections for this live plotter")
                                .clicked()
                                {
                                    ctx.data_mut(|d| {
                                        d.insert_temp(
                                            egui::Id::new((
                                                "plotter_remove_connections",
                                                plugin_id,
                                            )),
                                            true,
                                        )
                                    });
                                }
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
                                                    d.insert_temp(
                                                        export_state,
                                                        settings_seed.clone(),
                                                    );
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
                                            let state_id = egui::Id::new((
                                                "plotter_settings_state",
                                                plugin_id,
                                            ));
                                            ctx.data_mut(|d| {
                                                d.insert_temp(open_id, true);
                                                if d.get_temp::<PlotterPreviewState>(state_id)
                                                    .is_none()
                                                {
                                                    d.insert_temp(state_id, settings_seed.clone());
                                                }
                                            });
                                        }
                                    },
                                );
                            });
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
                        let mut open = ctx.data(|d| d.get_temp::<bool>(open_id).unwrap_or(false));
                        if open {
                            let mut state = ctx
                                .data(|d| d.get_temp::<PlotterPreviewState>(state_id))
                                .unwrap_or_else(|| settings_seed.clone());
                            Self::sync_series_controls_from_seed(&mut state, &settings_seed);
                            while state.series_scales.len() < state.series_names.len() {
                                state.series_scales.push(1.0);
                            }
                            while state.series_offsets.len() < state.series_names.len() {
                                state.series_offsets.push(0.0);
                            }
                            if state.series_names.is_empty() {
                                state.selected_series_tab = 0;
                                state.series_tab_start = 0;
                            } else {
                                state.selected_series_tab =
                                    state.selected_series_tab.min(state.series_names.len() - 1);
                                state.series_tab_start =
                                    state.series_tab_start.min(state.series_names.len() - 1);
                            }
                            let viewport_size = ctx.screen_rect().size();
                            let width_limit = (viewport_size.x - 24.0).max(420.0);
                            let height_limit = (viewport_size.y - 24.0).max(320.0);
                            // Keep a stable "dense" settings layout (no huge blank area on large monitors),
                            // while still fitting smaller windows. Target profile is close to the
                            // visually good minimized case.
                            let width_cap = width_limit.min(980.0);
                            let height_cap = height_limit.min(560.0);
                            let k = (width_cap / 16.0).min(height_cap / 9.0);
                            let fixed_size =
                                egui::vec2((k * 16.0).max(420.0), (k * 9.0).max(320.0));
                            egui::Window::new("Plot Settings")
                                .resizable(false)
                                .fixed_size(fixed_size)
                                .open(&mut open)
                                .show(ctx, |ui| {
                                    let apply_bar_h = BUTTON_SIZE.y + 6.0;
                                    let scroll_h = (ui.available_height() - apply_bar_h).max(120.0);
                                    egui::ScrollArea::vertical()
                                        .auto_shrink([false, false])
                                        .max_height(scroll_h)
                                        .show(ui, |ui| {
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
                                            Self::render_timebase_controls(
                                                ui,
                                                &mut state.window_ms,
                                                &mut state.timebase_divisions,
                                            );
                                            ui.horizontal(|ui| {
                                                ui.label("Refresh Hz:");
                                                ui.add(
                                                    egui::DragValue::new(&mut state.refresh_hz)
                                                        .clamp_range(1.0..=1000.0)
                                                        .speed(1.0),
                                                );
                                                ui.separator();
                                                ui.label("Priority:");
                                                ui.add(
                                                    egui::DragValue::new(&mut state.priority)
                                                        .clamp_range(-100..=1000)
                                                        .speed(1.0),
                                                );
                                            });
                                            ui.separator();

                                            ui.horizontal(|ui| {
                                                let total = state.series_names.len();
                                                let tab_w = 180.0_f32;
                                                let arrow_w = 28.0_f32;
                                                let available = ui.available_width().max(tab_w);
                                                let can_page = (total as f32 * tab_w) > available;
                                                let tabs_width = if can_page {
                                                    (available - arrow_w).max(tab_w)
                                                } else {
                                                    available
                                                };
                                                let visible =
                                                    ((tabs_width / tab_w).floor() as usize).max(1);
                                                let end =
                                                    (state.series_tab_start + visible).min(total);
                                                for i in state.series_tab_start..end {
                                                    let full = state.series_names[i].clone();
                                                    let text = Self::truncate_tab_label(&full, 20);
                                                    let selected = state.selected_series_tab == i;
                                                    let resp = ui.add_sized(
                                                        [180.0, 24.0],
                                                        egui::SelectableLabel::new(selected, text),
                                                    );
                                                    if resp.clicked() {
                                                        state.selected_series_tab = i;
                                                    }
                                                    if !selected {
                                                        let _ = resp.on_hover_text(full);
                                                    }
                                                }
                                                if can_page
                                                    && ui
                                                        .add_enabled(
                                                            state.series_tab_start + visible
                                                                < total,
                                                            egui::Button::new(">"),
                                                        )
                                                        .clicked()
                                                {
                                                    state.series_tab_start =
                                                        (state.series_tab_start + 1)
                                                            .min(total.saturating_sub(1));
                                                }
                                            });

                                            if !state.series_names.is_empty() {
                                                let i = state.selected_series_tab;
                                                ui.horizontal(|ui| {
                                                    ui.add_sized(
                                                        [
                                                            (ui.available_width() - 28.0)
                                                                .max(120.0),
                                                            22.0,
                                                        ],
                                                        egui::TextEdit::singleline(
                                                            &mut state.series_names[i],
                                                        ),
                                                    );
                                                    ui.color_edit_button_srgba(
                                                        &mut state.colors[i],
                                                    );
                                                });
                                            }
                                            ui.separator();
                                            let preview_height =
                                                (ui.available_height() * 0.5).clamp(170.0, 360.0);
                                            let preview_size =
                                                egui::vec2(ui.available_width(), preview_height);
                                            let series_transforms = Self::build_series_transforms(
                                                &state.series_scales,
                                                &state.series_offsets,
                                                state.series_names.len(),
                                            );
                                            ui.allocate_ui(preview_size, |ui| {
                                                let input_count = plotter.input_count;
                                                let refresh_hz = plotter.refresh_hz;
                                                plotter.set_window_ms(state.window_ms);
                                                plotter.update_config(
                                                    input_count,
                                                    refresh_hz,
                                                    logic_period_seconds,
                                                );
                                                plotter.render_with_settings(
                                                    ui,
                                                    "",
                                                    &time_label,
                                                    state.show_axes,
                                                    state.show_legend,
                                                    state.show_grid,
                                                    Some(&state.title),
                                                    Some(&state.series_names),
                                                    Some(&series_transforms),
                                                    Some(&state.colors),
                                                    state.dark_theme,
                                                    Some(&state.x_axis_name),
                                                    Some(&state.y_axis_name),
                                                    Some(state.window_ms),
                                                );
                                            });
                                            ui.separator();
                                            if !state.series_names.is_empty() {
                                                let i = state.selected_series_tab;
                                                let row_w = ui.available_width();
                                                let row_h = (ui.available_height() - 56.0)
                                                    .clamp(120.0, 180.0);
                                                ui.allocate_ui_with_layout(
                                                    egui::vec2(row_w, row_h),
                                                    egui::Layout::left_to_right(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        let left_w =
                                                            (row_w * 0.18).clamp(110.0, 180.0);
                                                        let right_w =
                                                            (row_w * 0.18).clamp(110.0, 180.0);
                                                        let center_w =
                                                            (row_w - left_w - right_w).max(260.0);

                                                        ui.allocate_ui_with_layout(
                                                            egui::vec2(left_w, row_h),
                                                            egui::Layout::top_down(
                                                                egui::Align::Center,
                                                            ),
                                                            |ui| {
                                                                Self::knob_control(
                                                                    ui,
                                                                    "DC Offset",
                                                                    &mut state.series_offsets[i],
                                                                    -1_000_000_000.0,
                                                                    1_000_000_000.0,
                                                                    1.0,
                                                                    1,
                                                                );
                                                            },
                                                        );

                                                        ui.allocate_ui_with_layout(
                                                            egui::vec2(center_w, row_h),
                                                            egui::Layout::top_down(
                                                                egui::Align::Center,
                                                            ),
                                                            |ui| {
                                                                ui.vertical_centered(|ui| {
                                                                    Self::wheel_value_box(
                                                                        ui,
                                                                        "Offset Value",
                                                                        &mut state.series_offsets
                                                                            [i],
                                                                        1,
                                                                        -1_000_000_000.0,
                                                                        1_000_000_000.0,
                                                                    );
                                                                    ui.add_space(8.0);
                                                                    Self::wheel_value_box(
                                                                        ui,
                                                                        "Scale Value",
                                                                        &mut state.series_scales[i],
                                                                        3,
                                                                        0.001,
                                                                        1_000_000.0,
                                                                    );
                                                                });
                                                            },
                                                        );

                                                        ui.allocate_ui_with_layout(
                                                            egui::vec2(right_w, row_h),
                                                            egui::Layout::top_down(
                                                                egui::Align::Center,
                                                            ),
                                                            |ui| {
                                                                Self::knob_control(
                                                                    ui,
                                                                    "Gain",
                                                                    &mut state.series_scales[i],
                                                                    0.001,
                                                                    1_000_000.0,
                                                                    0.08,
                                                                    3,
                                                                );
                                                            },
                                                        );
                                                    },
                                                );
                                            }
                                        });
                                    ui.add_space(2.0);
                                    if styled_button(ui, "Apply").clicked() {
                                        ctx.data_mut(|d| d.insert_temp(save_id, true));
                                        ctx.request_repaint();
                                    }
                                });
                            ctx.data_mut(|d| {
                                d.insert_temp(state_id, state);
                                d.insert_temp(open_id, open);
                            });
                        }
                        let refresh_hz = plotter.refresh_hz.max(1.0);
                        ctx.request_repaint_after(Duration::from_secs_f64(1.0 / refresh_hz));
                    }
                });
                if self.connection_editor_host == ConnectionEditorHost::PluginWindow(plugin_id) {
                    self.notification_handler.set_active_plugin(Some(plugin_id));
                    self.render_connection_editor(ctx);
                    self.notification_handler.set_active_plugin(None);
                }
                self.render_plotter_notifications(ctx, plugin_id);
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
                d.get_temp::<bool>(egui::Id::new(("plotter_start_toggle", plugin_id)))
                    .unwrap_or(false)
            }) {
                start_toggle_requested.push(plugin_id);
                ctx.data_mut(|d| {
                    d.remove::<bool>(egui::Id::new(("plotter_start_toggle", plugin_id)))
                });
            }
            if ctx.data(|d| {
                d.get_temp::<bool>(egui::Id::new(("plotter_add_connections", plugin_id)))
                    .unwrap_or(false)
            }) {
                add_connection_requested.push(plugin_id);
                ctx.data_mut(|d| {
                    d.remove::<bool>(egui::Id::new(("plotter_add_connections", plugin_id)));
                });
            }
            if ctx.data(|d| {
                d.get_temp::<bool>(egui::Id::new(("plotter_remove_connections", plugin_id)))
                    .unwrap_or(false)
            }) {
                remove_connection_requested.push(plugin_id);
                ctx.data_mut(|d| {
                    d.remove::<bool>(egui::Id::new(("plotter_remove_connections", plugin_id)));
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
                                series_scales,
                                series_offsets,
                                colors,
                                title,
                                dark_theme,
                                x_axis,
                                y_axis,
                                window_ms,
                                _timebase_divisions,
                                _high_quality,
                                _export_svg,
                            )) = self
                                .plotter_manager
                                .plotter_preview_settings
                                .get(&plugin_id)
                                .cloned()
                            {
                                let series_transforms = Self::build_series_transforms(
                                    &series_scales,
                                    &series_offsets,
                                    series_names.len(),
                                );
                                plotter.render_with_settings(
                                    ui,
                                    "",
                                    &self.state_sync.logic_time_label,
                                    show_axes,
                                    show_legend,
                                    show_grid,
                                    Some(&title),
                                    Some(&series_names),
                                    Some(&series_transforms),
                                    Some(&colors),
                                    dark_theme,
                                    Some(&x_axis),
                                    Some(&y_axis),
                                    Some(window_ms),
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
            let open_id = egui::Id::new(("plotter_settings_open", id));
            let state_id = egui::Id::new(("plotter_settings_state", id));
            let save_id = egui::Id::new(("plotter_settings_save", id));
            let export_open_id = egui::Id::new(("plotter_export_open", id));
            let export_state_id = egui::Id::new(("plotter_export_state", id));
            let export_save_id = egui::Id::new(("plotter_export_save", id));
            let export_close_id = egui::Id::new(("plotter_export_close", id));
            ctx.data_mut(|d| {
                d.remove::<bool>(open_id);
                d.remove::<PlotterPreviewState>(state_id);
                d.remove::<bool>(save_id);
                d.remove::<bool>(export_open_id);
                d.remove::<PlotterPreviewState>(export_state_id);
                d.remove::<bool>(export_save_id);
                d.remove::<bool>(export_close_id);
            });
            if self.connection_editor_host == ConnectionEditorHost::PluginWindow(id) {
                self.connection_editor.open = false;
                self.connection_editor.plugin_id = None;
                self.connection_editor_host = ConnectionEditorHost::Main;
            }
            // Just close the plotter window, don't remove the plugin
            if let Some(plotter) = self.plotter_manager.plotters.get(&id) {
                if let Ok(mut plotter) = plotter.lock() {
                    plotter.open = false;
                }
            }
            self.recompute_plotter_ui_hz();
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
        for plugin_id in start_toggle_requested {
            if let Err(err) = self.toggle_plugin_running_from_plotter_window(plugin_id) {
                self.show_plugin_info(plugin_id, "Plugin", &err);
            }
        }
        for plugin_id in add_connection_requested {
            self.selected_plugin_id = Some(plugin_id);
            self.open_connection_editor_in_plugin_window(
                plugin_id,
                plugin_id,
                ConnectionEditMode::Add,
            );
        }
        for plugin_id in remove_connection_requested {
            self.selected_plugin_id = Some(plugin_id);
            self.open_connection_editor_in_plugin_window(
                plugin_id,
                plugin_id,
                ConnectionEditMode::Remove,
            );
        }
    }

    fn build_plotter_preview_state(&self, plugin_id: u64) -> PlotterPreviewState {
        let mut state = PlotterPreviewState::default();
        state.target = Some(plugin_id);
        let live_count = self
            .plotter_manager
            .plotters
            .get(&plugin_id)
            .and_then(|p| p.lock().ok().map(|p| p.input_count))
            .unwrap_or(0);
        let live_names = self.aligned_series_names(plugin_id, live_count);
        if let Some((
            show_axes,
            show_legend,
            show_grid,
            series_names,
            series_scales,
            series_offsets,
            colors,
            title,
            dark_theme,
            x_axis,
            y_axis,
            window_ms,
            timebase_divisions,
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
            state.series_scales = series_scales;
            state.series_offsets = series_offsets;
            state.colors = colors;
            state.title = title;
            state.dark_theme = dark_theme;
            state.x_axis_name = x_axis;
            state.y_axis_name = y_axis;
            state.window_ms = window_ms;
            state.timebase_divisions = timebase_divisions.clamp(1, 200);
            state.high_quality = high_quality;
            state.export_svg = export_svg;
            if live_count > 0 {
                if state.series_names.len() < live_count {
                    for i in state.series_names.len()..live_count {
                        state.series_names.push(
                            live_names
                                .get(i)
                                .cloned()
                                .unwrap_or_else(|| format!("Series {}", i + 1)),
                        );
                        state.series_scales.push(1.0);
                        state.series_offsets.push(0.0);
                        state.colors.push(match i % 8 {
                            0 => egui::Color32::from_rgb(86, 156, 214),
                            1 => egui::Color32::from_rgb(220, 122, 95),
                            2 => egui::Color32::from_rgb(181, 206, 168),
                            3 => egui::Color32::from_rgb(220, 220, 170),
                            4 => egui::Color32::from_rgb(197, 134, 192),
                            5 => egui::Color32::from_rgb(78, 201, 176),
                            6 => egui::Color32::from_rgb(156, 220, 254),
                            _ => egui::Color32::from_rgb(255, 206, 84),
                        });
                    }
                } else if state.series_names.len() > live_count {
                    state.series_names.truncate(live_count);
                    state.series_scales.truncate(live_count);
                    state.series_offsets.truncate(live_count);
                    state.colors.truncate(live_count);
                }
                for (i, n) in live_names.into_iter().enumerate() {
                    if i < state.series_names.len() {
                        state.series_names[i] = n;
                    }
                }
            }
            if state.series_names.is_empty() {
                state.series_names.push("Series 1".to_string());
                state.series_scales.push(1.0);
                state.series_offsets.push(0.0);
                state.colors.push(egui::Color32::from_rgb(86, 156, 214));
            }
            return state;
        }
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
                        live_names
                            .get(i)
                            .cloned()
                            .unwrap_or_else(|| format!("Series {}", i + 1))
                    })
                    .collect();
                state.series_scales = vec![1.0; plotter.input_count];
                state.series_offsets = vec![0.0; plotter.input_count];
                state.window_ms = plotter.window_ms;
                state.timebase_divisions = 10;
                state.refresh_hz = plotter.refresh_hz;
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
        if let Some(plugin) = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .find(|plugin| plugin.id == plugin_id)
        {
            state.priority = plugin.priority;
            state.refresh_hz = plugin
                .config
                .get("refresh_hz")
                .and_then(|v| v.as_f64())
                .unwrap_or(state.refresh_hz);
        }
        if state.series_names.is_empty() {
            state.series_names.push("Series 1".to_string());
            state.series_scales.push(1.0);
            state.series_offsets.push(0.0);
            state.colors.push(egui::Color32::from_rgb(86, 156, 214));
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
                state.series_scales.clone(),
                state.series_offsets.clone(),
                state.colors.clone(),
                state.title.clone(),
                state.dark_theme,
                state.x_axis_name.clone(),
                state.y_axis_name.clone(),
                state.window_ms,
                state.timebase_divisions.clamp(1, 200),
                state.high_quality,
                state.export_svg,
            ),
        );
        if let Some(plugin) = self
            .workspace_manager
            .workspace
            .plugins
            .iter_mut()
            .find(|plugin| plugin.id == plugin_id)
        {
            plugin.priority = state.priority;
            if let Some(config) = plugin.config.as_object_mut() {
                config.insert(
                    "refresh_hz".to_string(),
                    Value::from(state.refresh_hz.max(1.0)),
                );
            }
            self.mark_workspace_dirty();
            let _ = self.state_sync.logic_tx.send(LogicMessage::UpdateWorkspace(
                self.workspace_manager.workspace.clone(),
            ));
        }
    }

    fn build_series_transforms(
        scales: &[f64],
        offsets: &[f64],
        count: usize,
    ) -> Vec<crate::plotter::SeriesTransform> {
        (0..count)
            .map(|i| crate::plotter::SeriesTransform {
                scale: *scales.get(i).unwrap_or(&1.0),
                offset: *offsets.get(i).unwrap_or(&0.0),
            })
            .collect()
    }

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
