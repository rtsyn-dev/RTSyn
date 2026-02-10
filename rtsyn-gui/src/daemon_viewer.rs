use crate::plotter::LivePlotter;
use crate::{GuiConfig, GuiError};
use eframe::egui;
use rtsyn_cli::client;
use rtsyn_cli::protocol::{DaemonRequest, DaemonResponse, RuntimePluginState};
use rtsyn_core::plugin::PluginManager;
use std::time::{Duration, Instant};

pub fn run_daemon_plugin_viewer(
    config: GuiConfig,
    plugin_id: u64,
    socket_path: String,
) -> Result<(), GuiError> {
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([config.width, config.height]),
        ..Default::default()
    };
    options.vsync = false;

    eframe::run_native(
        &format!("RTSyn Viewer - {plugin_id}"),
        options,
        Box::new(move |cc| {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "fa".to_string(),
                egui::FontData::from_static(include_bytes!("../assets/fonts/fa-solid-900.ttf")),
            );
            let family = fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default();
            if !family.contains(&"fa".to_string()) {
                family.push("fa".to_string());
            }
            cc.egui_ctx.set_fonts(fonts);
            Box::new(DaemonPluginViewer::new(plugin_id, socket_path))
        }),
    )
    .map_err(|err| GuiError::Gui(err.to_string()))
}

struct DaemonPluginView {
    kind: String,
    state: RuntimePluginState,
    period_seconds: f64,
    time_scale: f64,
    time_label: String,
    samples: Vec<(u64, Vec<f64>)>,
    series_names: Vec<String>,
}

struct DaemonPluginViewer {
    plugin_id: u64,
    socket_path: String,
    last_fetch: Instant,
    last_settings_fetch: Instant,
    view: Option<DaemonPluginView>,
    display_state: Option<RuntimePluginState>,
    display_kind: Option<String>,
    display_running: Option<bool>,
    plotter: LivePlotter,
    error: Option<String>,
    running: Option<bool>,
    last_sample_tick: Option<u64>,
    last_refresh_hz: f64,
}

fn kv_row_wrapped(
    ui: &mut egui::Ui,
    label: &str,
    label_w: f32,
    value_ui: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        let label_response = ui.allocate_ui_with_layout(
            egui::vec2(label_w, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.add(egui::Label::new(label).wrap(true));
            },
        );

        let used_width = label_response.response.rect.width();
        if used_width < label_w {
            ui.add_space(label_w - used_width);
        }

        ui.add_space(8.0);
        value_ui(ui);
    });
}

impl DaemonPluginViewer {
    fn new(plugin_id: u64, socket_path: String) -> Self {
        Self {
            plugin_id,
            socket_path,
            last_fetch: Instant::now() - Duration::from_secs(1),
            last_settings_fetch: Instant::now() - Duration::from_secs(1),
            view: None,
            plotter: LivePlotter::new(plugin_id),
            error: None,
            running: None,
            last_sample_tick: None,
            last_refresh_hz: 60.0,
            display_state: None,
            display_kind: None,
            display_running: None,
        }
    }

    fn fetch_view(&mut self) {
        match client::send_request_to(
            &self.socket_path,
            &DaemonRequest::RuntimePluginView { id: self.plugin_id },
        ) {
            Ok(DaemonResponse::RuntimePluginView {
                kind,
                state,
                samples,
                series_names,
                period_seconds,
                time_scale,
                time_label,
                ..
            }) => {
                self.running = state
                    .internal_variables
                    .iter()
                    .find(|(key, _)| key == "running")
                    .and_then(|(_, value)| value.as_bool());
                self.view = Some(DaemonPluginView {
                    kind,
                    state,
                    period_seconds,
                    time_scale,
                    time_label,
                    samples,
                    series_names,
                });
                self.error = None;
            }
            Ok(DaemonResponse::Error { message }) => {
                self.error = Some(message);
            }
            Err(err) => {
                self.error = Some(err);
            }
            _ => {}
        }
    }

    fn variable_map(state: &RuntimePluginState) -> std::collections::HashMap<&str, f64> {
        let mut map = std::collections::HashMap::new();
        for (key, value) in &state.variables {
            if let Some(num) = value.as_f64() {
                map.insert(key.as_str(), num);
            }
        }
        for (key, value) in &state.internal_variables {
            if let Some(num) = value.as_f64() {
                map.insert(key.as_str(), num);
            }
        }
        map
    }

    fn plotter_config(
        state: &RuntimePluginState,
        samples: &[(u64, Vec<f64>)],
    ) -> (usize, f64, f64, f64) {
        let vars = Self::variable_map(state);
        let mut input_count = vars.get("input_count").copied().unwrap_or(0.0) as usize;
        if input_count == 0 {
            if let Some((_, values)) = samples.last() {
                input_count = values.len();
            }
        }
        let refresh_hz = vars.get("refresh_hz").copied().unwrap_or(60.0);
        let window_ms = if let Some(window_ms) = vars.get("window_ms") {
            *window_ms
        } else {
            let multiplier = vars.get("window_multiplier").copied().unwrap_or(10000.0);
            let value = vars.get("window_value").copied().unwrap_or(10.0);
            multiplier * value
        };
        let amplitude = vars.get("amplitude").copied().unwrap_or(0.0);
        (input_count, refresh_hz, window_ms, amplitude)
    }

    fn render_section_values(
        ui: &mut egui::Ui,
        title: &str,
        icon: &str,
        items: &[(String, serde_json::Value)],
    ) {
        if items.is_empty() {
            return;
        }
        egui::CollapsingHeader::new(
            egui::RichText::new(format!("{icon}  {title}"))
                .size(13.0)
                .strong(),
        )
        .default_open(true)
        .show(ui, |ui| {
            ui.add_space(4.0);
            let filtered: Vec<_> = items
                .iter()
                .filter(|(name, _)| name != "library_path")
                .collect();
            if !filtered.is_empty() {
                for (name, value) in filtered {
                    let mut value_text = match value {
                        serde_json::Value::Number(num) => {
                            if let Some(i) = num.as_i64() {
                                i.to_string()
                            } else if let Some(u) = num.as_u64() {
                                u.to_string()
                            } else {
                                num.as_f64()
                                    .map(|v| format!("{:.4}", v))
                                    .unwrap_or_else(|| value.to_string())
                            }
                        }
                        _ => value.to_string(),
                    };
                    kv_row_wrapped(ui, name, 140.0, |ui| {
                        ui.add_enabled_ui(false, |ui| {
                            ui.add_sized([80.0, 0.0], egui::TextEdit::singleline(&mut value_text));
                        });
                    });
                    ui.add_space(4.0);
                }
            }
        });
    }

    fn render_section_numbers(ui: &mut egui::Ui, title: &str, icon: &str, items: &[(String, f64)]) {
        if items.is_empty() {
            return;
        }
        egui::CollapsingHeader::new(
            egui::RichText::new(format!("{icon}  {title}"))
                .size(13.0)
                .strong(),
        )
        .default_open(true)
        .show(ui, |ui| {
            ui.add_space(4.0);
            if !items.is_empty() {
                for (name, value) in items {
                    let mut value_text = if (value.fract() - 0.0).abs() < f64::EPSILON {
                        format!("{value:.0}")
                    } else {
                        format!("{value:.4}")
                    };
                    kv_row_wrapped(ui, name, 140.0, |ui| {
                        ui.add_enabled_ui(false, |ui| {
                            ui.add_sized([80.0, 0.0], egui::TextEdit::singleline(&mut value_text));
                        });
                    });
                    ui.add_space(4.0);
                }
            }
        });
    }

    fn render_section_inputs(ui: &mut egui::Ui, title: &str, icon: &str, items: &[(String, f64)]) {
        if items.is_empty() {
            return;
        }
        egui::CollapsingHeader::new(
            egui::RichText::new(format!("{icon}  {title}"))
                .size(13.0)
                .strong(),
        )
        .default_open(true)
        .show(ui, |ui| {
            ui.add_space(4.0);
            if !items.is_empty() {
                for (name, value) in items {
                    let mut value_text = format!("{value:.4}");
                    kv_row_wrapped(ui, name, 140.0, |ui| {
                        ui.add_enabled_ui(false, |ui| {
                            ui.add_sized([80.0, 0.0], egui::TextEdit::singleline(&mut value_text));
                        });
                    });
                    ui.add_space(4.0);
                }
            }
        });
    }

    fn render_plugin_card(&self, ui: &mut egui::Ui, view: &DaemonPluginView) {
        let frame = egui::Frame::none()
            .fill(egui::Color32::from_gray(30))
            .rounding(egui::Rounding::same(6.0))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(50)))
            .inner_margin(egui::Margin::same(12.0))
            .outer_margin(egui::Margin::ZERO);

        frame.show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let (id_rect, _) =
                        ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(id_rect, 8.0, egui::Color32::from_gray(60));
                    ui.painter().text(
                        id_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        self.plugin_id.to_string(),
                        egui::FontId::proportional(12.0),
                        egui::Color32::from_rgb(200, 200, 210),
                    );

                    ui.add_space(8.0);

                    let display_name = PluginManager::display_kind(&view.kind);
                    ui.label(egui::RichText::new(display_name).size(15.0).strong());

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(running) = self.running {
                            let status = if running { "Running" } else { "Stopped" };
                            let color = if running {
                                egui::Color32::from_rgb(80, 200, 120)
                            } else {
                                egui::Color32::from_rgb(220, 100, 100)
                            };
                            ui.label(egui::RichText::new(status).color(color));
                        }
                    });
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                ui.style_mut().spacing.item_spacing = egui::vec2(0.0, 6.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        Self::render_section_values(
                            ui,
                            "Variables",
                            "\u{f013}",
                            &view.state.variables,
                        );
                        Self::render_section_numbers(
                            ui,
                            "Outputs",
                            "\u{f08b}",
                            &view.state.outputs,
                        );
                        Self::render_section_inputs(ui, "Inputs", "\u{f090}", &view.state.inputs);
                        Self::render_section_values(
                            ui,
                            "Internal variables",
                            "\u{f085}",
                            &view.state.internal_variables,
                        );
                    });
            });
        });
    }
}

impl eframe::App for DaemonPluginViewer {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let fetch_interval = if self.last_refresh_hz > 0.0 {
            Duration::from_secs_f64(1.0 / self.last_refresh_hz.max(1.0))
        } else {
            Duration::from_millis(50)
        };
        if self.last_fetch.elapsed() >= fetch_interval {
            self.fetch_view();
            self.last_fetch = Instant::now();
        }
        if self.last_settings_fetch.elapsed() >= Duration::from_secs(1) {
            if let Some(view) = self.view.as_ref() {
                self.display_state = Some(view.state.clone());
                self.display_kind = Some(view.kind.clone());
                self.display_running = self.running;
            }
            self.last_settings_fetch = Instant::now();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let refresh_hz = self.last_refresh_hz.max(1.0);
        ctx.request_repaint_after(Duration::from_secs_f64(1.0 / refresh_hz));

        egui::TopBottomPanel::bottom("viewer_bottom")
            .min_height(48.0)
            .show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new("Press Esc to exit view")
                            .size(18.0)
                            .strong()
                            .color(egui::Color32::from_rgb(220, 220, 220)),
                    );
                });
            });

        if let Some(err) = self.error.as_ref() {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.colored_label(egui::Color32::LIGHT_RED, err);
            });
            return;
        }
        let Some(view) = self.view.as_ref() else {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("Waiting for runtime data...");
            });
            return;
        };

        if view.kind == "live_plotter" {
            if let Some(state) = self.display_state.as_ref() {
                let display_view = DaemonPluginView {
                    kind: self
                        .display_kind
                        .clone()
                        .unwrap_or_else(|| view.kind.clone()),
                    state: state.clone(),
                    period_seconds: view.period_seconds,
                    time_scale: view.time_scale,
                    time_label: view.time_label.clone(),
                    samples: Vec::new(),
                    series_names: Vec::new(),
                };
                egui::SidePanel::left("viewer_card")
                    .default_width(320.0)
                    .resizable(false)
                    .show(ctx, |ui| {
                        self.render_plugin_card(ui, &display_view);
                    });
            }

            egui::CentralPanel::default().show(ctx, |ui| {
                let (input_count, refresh_hz, window_ms, amplitude) =
                    Self::plotter_config(&view.state, &view.samples);
                self.last_refresh_hz = refresh_hz;
                let period_seconds = view.period_seconds;
                self.plotter.update_config(
                    input_count,
                    refresh_hz,
                    window_ms,
                    amplitude,
                    period_seconds,
                );
                if !view.series_names.is_empty() {
                    self.plotter.set_series_names(view.series_names.clone());
                } else {
                    let series_names = (0..input_count).map(|i| format!("in_{i}")).collect();
                    self.plotter.set_series_names(series_names);
                }
                let time_scale = view.time_scale;
                if let Some(latest_tick) = view.samples.last().map(|(tick, _)| *tick) {
                    if let Some(last_tick) = self.last_sample_tick {
                        if latest_tick < last_tick {
                            self.plotter = LivePlotter::new(self.plugin_id);
                            self.last_sample_tick = None;
                        }
                    }
                }
                let mut latest_tick = self.last_sample_tick.unwrap_or(0);
                let mut has_samples = false;
                for (tick, values) in &view.samples {
                    if self.last_sample_tick.map_or(true, |last| *tick > last) {
                        let time_s = *tick as f64 * period_seconds;
                        self.plotter.push_sample(*tick, time_s, time_scale, values);
                        if *tick > latest_tick {
                            latest_tick = *tick;
                        }
                        has_samples = true;
                    }
                }
                if has_samples {
                    self.last_sample_tick = Some(latest_tick);
                }
                self.plotter.render(ui, "", &view.time_label);
            });
        } else {
            if let Some(state) = self.display_state.as_ref() {
                let display_view = DaemonPluginView {
                    kind: self
                        .display_kind
                        .clone()
                        .unwrap_or_else(|| view.kind.clone()),
                    state: state.clone(),
                    period_seconds: view.period_seconds,
                    time_scale: view.time_scale,
                    time_label: view.time_label.clone(),
                    samples: Vec::new(),
                    series_names: Vec::new(),
                };
                egui::CentralPanel::default().show(ctx, |ui| {
                    self.render_plugin_card(ui, &display_view);
                });
            }
        }
    }
}
