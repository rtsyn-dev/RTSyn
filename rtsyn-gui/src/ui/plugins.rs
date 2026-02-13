use super::*;
use crate::utils::{format_f64_with_input, normalize_numeric_input, parse_f64_input};
use crate::WindowFocus;
use crate::{
    has_rt_capabilities, spawn_file_dialog_thread, zenity_file_dialog, BuildAction, LivePlotter,
    PluginFieldDraft,
};
use rtsyn_cli::plugin_creator::{
    create_plugin, CreatorBehavior, PluginCreateRequest, PluginKindType, PluginLanguage,
};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

fn kv_row_wrapped(
    ui: &mut egui::Ui,
    label: &str,
    label_w: f32,
    value_ui: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        // Label in fixed-width area
        let label_response = ui.allocate_ui_with_layout(
            egui::vec2(label_w, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.add(egui::Label::new(label).wrap(true));
            },
        );

        // Add spacing to reach fixed position
        let used_width = label_response.response.rect.width();
        if used_width < label_w {
            ui.add_space(label_w - used_width);
        }

        ui.add_space(8.0);
        value_ui(ui);
    });
}

impl GuiApp {
    const NEW_PLUGIN_TYPES: [&'static str; 6] = ["f64", "f32", "i64", "i32", "bool", "string"];

    fn plugin_creator_default_by_type(ty: &str) -> &'static str {
        match ty.trim().to_ascii_lowercase().as_str() {
            "bool" => "false",
            "i64" | "i32" => "0",
            "string" | "file" | "path" => "",
            _ => "0.0",
        }
    }

    fn open_install_dialog(&mut self) {
        if self.file_dialogs.install_dialog_rx.is_some() {
            self.status = "Plugin dialog already open".to_string();
            return;
        }

        let (tx, rx) = mpsc::channel();
        self.file_dialogs.install_dialog_rx = Some(rx);
        self.status = "Opening plugin folder dialog...".to_string();

        crate::spawn_file_dialog_thread(move || {
            let folder = if crate::has_rt_capabilities() {
                crate::zenity_file_dialog("folder", None)
            } else {
                rfd::FileDialog::new().pick_folder()
            };
            let _ = tx.send(folder);
        });
    }

    fn open_plugin_creator_folder_dialog(&mut self) {
        if self.file_dialogs.plugin_creator_dialog_rx.is_some() {
            self.status = "Plugin creator dialog already open".to_string();
            return;
        }

        let (tx, rx) = mpsc::channel();
        self.file_dialogs.plugin_creator_dialog_rx = Some(rx);
        self.status = "Select destination folder for new plugin".to_string();

        let start_dir = self.plugin_creator_last_path.clone();
        spawn_file_dialog_thread(move || {
            let folder = if has_rt_capabilities() {
                zenity_file_dialog("folder", None)
            } else {
                let mut dialog = rfd::FileDialog::new();
                if let Some(dir) = start_dir {
                    dialog = dialog.set_directory(dir);
                }
                dialog.pick_folder()
            };
            let _ = tx.send(folder);
        });
    }

    pub(crate) fn open_new_plugin_window(&mut self) {
        self.windows.new_plugin_open = true;
        self.pending_window_focus = Some(WindowFocus::NewPlugin);
    }

    fn plugin_creator_draft_to_spec(entries: &[PluginFieldDraft]) -> String {
        entries
            .iter()
            .filter_map(|entry| {
                let name = entry.name.trim();
                if name.is_empty() {
                    None
                } else {
                    Some(format!("{name}:{}", entry.type_name.trim()))
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn plugin_creator_parse_spec(spec: &str) -> Vec<(String, String)> {
        spec.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| {
                let mut parts = line.splitn(2, ':');
                let name = parts.next().unwrap_or("").trim().to_string();
                let ty = parts.next().unwrap_or("f64").trim().to_string();
                (name, ty)
            })
            .filter(|(name, _)| !name.is_empty())
            .collect()
    }

    pub(crate) fn create_plugin_from_draft(&self, parent: &Path) -> Result<PathBuf, String> {
        let name = self.new_plugin_draft.name.trim();
        if name.is_empty() {
            return Err("Plugin name is required".to_string());
        }
        let vars_spec = Self::plugin_creator_draft_to_spec(&self.new_plugin_draft.variables);
        let inputs_spec = Self::plugin_creator_draft_to_spec(&self.new_plugin_draft.inputs);
        let outputs_spec = Self::plugin_creator_draft_to_spec(&self.new_plugin_draft.outputs);
        let internal_spec =
            Self::plugin_creator_draft_to_spec(&self.new_plugin_draft.internal_variables);
        self.create_plugin_from_specs(
            name,
            &self.new_plugin_draft.language,
            &self.new_plugin_draft.main_characteristics,
            self.new_plugin_draft.autostart,
            self.new_plugin_draft.supports_start_stop,
            self.new_plugin_draft.supports_restart,
            self.new_plugin_draft.supports_apply,
            self.new_plugin_draft.external_window,
            self.new_plugin_draft.starts_expanded,
            &self.new_plugin_draft.required_input_ports_csv,
            &self.new_plugin_draft.required_output_ports_csv,
            &vars_spec,
            &inputs_spec,
            &outputs_spec,
            &internal_spec,
            parent,
        )
    }

    fn create_plugin_from_specs(
        &self,
        name: &str,
        language: &str,
        main: &str,
        autostart: bool,
        supports_start_stop: bool,
        supports_restart: bool,
        supports_apply: bool,
        external_window: bool,
        starts_expanded: bool,
        required_input_ports_csv: &str,
        required_output_ports_csv: &str,
        vars_spec: &str,
        inputs_spec: &str,
        outputs_spec: &str,
        internals_spec: &str,
        parent: &Path,
    ) -> Result<PathBuf, String> {
        let title = name.trim();
        if title.is_empty() {
            return Err("Plugin name is required".to_string());
        }
        let parsed_language = PluginLanguage::parse(language)?;

        let vars = Self::plugin_creator_parse_spec(vars_spec);
        let variables = vars
            .iter()
            .map(|(name, ty)| {
                let default = self
                    .new_plugin_draft
                    .variables
                    .iter()
                    .find(|v| v.name.trim() == name)
                    .map(|v| v.default_value.as_str())
                    .unwrap_or_else(|| Self::plugin_creator_default_by_type(ty));
                rtsyn_cli::plugin_creator::parse_variable_line(&format!(
                    "{name}:{ty}={default}"
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let inputs: Vec<String> = Self::plugin_creator_parse_spec(inputs_spec)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        let outputs: Vec<String> = Self::plugin_creator_parse_spec(outputs_spec)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        let internals: Vec<String> = Self::plugin_creator_parse_spec(internals_spec)
            .into_iter()
            .map(|(name, _)| name)
            .collect();

        let req = PluginCreateRequest {
            base_dir: parent.to_path_buf(),
            name: title.to_string(),
            description: if main.trim().is_empty() {
                "Generated by plugin_creator".to_string()
            } else {
                main.lines()
                    .next()
                    .unwrap_or("Generated by plugin_creator")
                    .to_string()
            },
            language: parsed_language,
            plugin_type: PluginKindType::Standard,
            behavior: CreatorBehavior {
                autostart,
                supports_start_stop,
                supports_restart,
                supports_apply,
                external_window,
                starts_expanded,
                required_input_ports: required_input_ports_csv
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect(),
                required_output_ports: required_output_ports_csv
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect(),
            },
            inputs,
            outputs,
            internal_variables: internals,
            variables,
        };

        create_plugin(&req)
    }

    pub(crate) fn open_manage_plugins(&mut self) {
        self.windows.manage_plugins_open = true;
        self.scan_detected_plugins();
        self.windows.manage_plugin_selected_index = None;
        self.pending_window_focus = Some(WindowFocus::ManagePlugins);
    }

    pub(crate) fn open_install_plugins(&mut self) {
        self.windows.install_plugins_open = true;
        self.scan_detected_plugins();
        self.windows.install_selected_index = None;
        self.pending_window_focus = Some(WindowFocus::InstallPlugins);
    }

    pub(crate) fn open_uninstall_plugins(&mut self) {
        self.windows.uninstall_plugins_open = true;
        self.windows.uninstall_selected_index = None;
        self.pending_window_focus = Some(WindowFocus::UninstallPlugins);
    }

    pub(crate) fn open_plugins(&mut self) {
        self.windows.plugins_open = true;
        self.windows.plugin_selected_index = None;
        self.pending_window_focus = Some(WindowFocus::Plugins);
    }

    pub(crate) fn render_plugin_cards(&mut self, ctx: &egui::Context, panel_rect: egui::Rect) {
        const CARD_WIDTH: f32 = 280.0;
        const CARD_FIXED_HEIGHT: f32 = 132.0;
        const PANEL_PAD: f32 = 8.0;
        let mut pending_info: Option<String> = None;
        let connected_input_ports: HashMap<u64, HashSet<String>> = self
            .workspace_manager
            .workspace
            .connections
            .iter()
            .fold(HashMap::new(), |mut acc, conn| {
                acc.entry(conn.to_plugin)
                    .or_insert_with(HashSet::new)
                    .insert(conn.to_port.clone());
                acc
            });
        let connected_output_ports: HashMap<u64, HashSet<String>> = self
            .workspace_manager
            .workspace
            .connections
            .iter()
            .fold(HashMap::new(), |mut acc, conn| {
                acc.entry(conn.from_plugin)
                    .or_insert_with(HashSet::new)
                    .insert(conn.from_port.clone());
                acc
            });
        let name_by_kind: HashMap<String, String> = self
            .plugin_manager
            .installed_plugins
            .iter()
            .map(|plugin| (plugin.manifest.kind.clone(), plugin.manifest.name.clone()))
            .collect();
        let metadata_by_kind: HashMap<String, Vec<(String, f64)>> = self
            .plugin_manager
            .installed_plugins
            .iter()
            .map(|plugin| {
                (
                    plugin.manifest.kind.clone(),
                    plugin.metadata_variables.clone(),
                )
            })
            .collect();
        let computed_outputs = self.state_sync.computed_outputs.clone();
        let input_values = self.state_sync.input_values.clone();
        let internal_variable_values = self.state_sync.internal_variable_values.clone();
        let viewer_values = self.state_sync.viewer_values.clone();
        let mut remove_id: Option<u64> = None;
        let mut pending_running: Vec<(u64, bool)> = Vec::new();
        let mut pending_restart: Vec<u64> = Vec::new();
        let mut pending_workspace_update = false;
        let mut pending_prune: Option<(u64, usize)> = None;
        let mut pending_enforce_connection = false;

        let mut index = 0usize;
        let max_per_row = ((panel_rect.width() / 240.0).floor() as usize).max(1);
        let mut workspace_changed = false;
        let mut recompute_plotter_needed = false;
        let right_down = ctx.input(|i| i.pointer.secondary_down());
        let card_height_cap = (panel_rect.height() - PANEL_PAD * 2.0).max(220.0);
        let scroll_max_height = (card_height_cap - CARD_FIXED_HEIGHT).max(72.0);
        for plugin in &mut self.workspace_manager.workspace.plugins {
            let behavior = self
                .plugin_manager
                .plugin_behaviors
                .get(&plugin.kind)
                .cloned()
                .unwrap_or_default();
            let opens_external_window = behavior.external_window;
            let starts_expanded = behavior.starts_expanded;

            if let Some(default_vars) = metadata_by_kind.get(&plugin.kind) {
                if let Value::Object(ref mut map) = plugin.config {
                    let mut injected_any = false;
                    for (name, value) in default_vars {
                        if !map.contains_key(name) {
                            map.insert(name.clone(), Value::from(*value));
                            injected_any = true;
                        }
                    }
                    if injected_any {
                        workspace_changed = true;
                    }
                }
            }

            if opens_external_window {
                self.plugin_rects.remove(&plugin.id);
                continue;
            }
            let col = index % max_per_row;
            let row = index / max_per_row;
            let default_pos = panel_rect.min
                + egui::vec2(12.0 + (col as f32 * 240.0), 12.0 + (row as f32 * 140.0));
            let requested_pos = self
                .plugin_positions
                .get(&plugin.id)
                .cloned()
                .unwrap_or(default_pos);
            let min_x = panel_rect.left() + PANEL_PAD;
            let max_x = (panel_rect.right() - CARD_WIDTH - PANEL_PAD).max(min_x);
            let min_y = panel_rect.top() + PANEL_PAD;
            let max_y = (panel_rect.bottom() - card_height_cap - PANEL_PAD).max(min_y);
            let pos = egui::pos2(
                requested_pos.x.clamp(min_x, max_x),
                requested_pos.y.clamp(min_y, max_y),
            );
            let area_id = egui::Id::new(("plugin_window", plugin.id));
            let mut plugin_changed = false;
            let current_id = self.connection_editor.plugin_id;
            let selected_id = self.connection_highlight_plugin_id;
            let tab_primary = match self.connection_editor.tab {
                ConnectionEditTab::Inputs => egui::Color32::from_rgb(255, 170, 80),
                ConnectionEditTab::Outputs => egui::Color32::from_rgb(80, 200, 120),
            };
            let tab_secondary = match self.connection_editor.tab {
                ConnectionEditTab::Inputs => egui::Color32::from_rgb(80, 200, 120),
                ConnectionEditTab::Outputs => egui::Color32::from_rgb(255, 170, 80),
            };
            let highlight_color = if current_id == Some(plugin.id) {
                Some(tab_primary)
            } else if selected_id == Some(plugin.id) {
                Some(tab_secondary)
            } else {
                None
            };
            let mut frame = egui::Frame::none()
                .fill(egui::Color32::from_gray(30))
                .rounding(egui::Rounding::same(6.0))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(50)))
                .inner_margin(egui::Margin::same(12.0))
                .outer_margin(egui::Margin::ZERO);
            if let Some(color) = highlight_color {
                frame = frame.stroke(egui::Stroke::new(2.0, color));
            }
            let response = egui::Area::new(area_id)
                .order(egui::Order::Middle)
                .default_pos(pos)
                .movable(!right_down)
                .constrain_to(panel_rect)
                .show(ctx, |ui| {
                    ui.set_width(CARD_WIDTH);
                    ui.set_max_height(card_height_cap);

                    frame.show(ui, |ui| {
                        ui.vertical(|ui| {
                            // Header
                            ui.horizontal(|ui| {
                                // ID badge
                                let (id_rect, _) = ui.allocate_exact_size(
                                    egui::vec2(24.0, 24.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(
                                    id_rect,
                                    8.0,
                                    egui::Color32::from_gray(60),
                                );
                                ui.painter().text(
                                    id_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    plugin.id.to_string(),
                                    egui::FontId::proportional(12.0),
                                    egui::Color32::from_rgb(200, 200, 210),
                                );

                                ui.add_space(8.0);

                                // Plugin name
                                let display_name = name_by_kind
                                    .get(&plugin.kind)
                                    .cloned()
                                    .unwrap_or_else(|| Self::display_kind(&plugin.kind));
                                let title_w = (ui.available_width() - 28.0).max(80.0);
                                ui.add_sized(
                                    [title_w, 0.0],
                                    egui::Label::new(RichText::new(display_name).size(15.0).strong())
                                        .truncate(true),
                                );

                                // Close button
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let (close_rect, close_resp) = ui.allocate_exact_size(
                                        egui::vec2(20.0, 20.0),
                                        egui::Sense::click(),
                                    );
                                    let close_color = if close_resp.hovered() {
                                        egui::Color32::WHITE
                                    } else {
                                        egui::Color32::from_gray(140)
                                    };
                                    ui.painter().text(
                                        close_rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "✕",
                                        egui::FontId::proportional(16.0),
                                        close_color,
                                    );
                                    if close_resp.clicked() {
                                        remove_id = Some(plugin.id);
                                    }
                                });
                            });

                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(4.0);

                            // Body with sections
                            ui.scope(|ui| {
                                // Set thin scrollbar BEFORE creating ScrollArea
                                let mut scroll_style = egui::style::ScrollStyle::solid();
                                scroll_style.bar_width = 4.0;
                                scroll_style.floating = true;  // Only show on hover
                                scroll_style.floating_width = 2.0;  // Thinner when not hovered
                                scroll_style.floating_allocated_width = 2.0;
                                ui.style_mut().spacing.scroll = scroll_style;

                                egui::ScrollArea::vertical()
                                    .max_height(scroll_max_height)
                                    .drag_to_scroll(false)
                                    .show(ui, |ui| {
                                        ui.push_id(("plugin_content", plugin.id), |ui| {
                                        ui.style_mut().spacing.item_spacing = egui::vec2(0.0, 6.0);

                                    let is_app_plugin = matches!(
                                        plugin.kind.as_str(),
                                        "csv_recorder"
                                            | "live_plotter"
                                            | "performance_monitor"
                                            | "comedi_daq"
                                    );
                                    if !is_app_plugin {
                                        match plugin.config {
                                            Value::Object(ref mut map) => {
                                                let mut vars = metadata_by_kind
                                                    .get(&plugin.kind)
                                                    .cloned()
                                                    .unwrap_or_default();
                                                if vars.is_empty() {
                                                    let reserved = [
                                                        "library_path",
                                                        "input_count",
                                                        "columns",
                                                        "path",
                                                        "path_autogen",
                                                        "scan_nonce",
                                                    ];
                                                    vars = map
                                                        .iter()
                                                        .filter_map(|(name, value)| {
                                                            if reserved.contains(&name.as_str()) {
                                                                return None;
                                                            }
                                                            value
                                                                .as_f64()
                                                                .map(|v| (name.clone(), v))
                                                        })
                                                        .collect();
                                                    vars.sort_by(|a, b| a.0.cmp(&b.0));
                                                }
                                                if !vars.is_empty() {
                                                    egui::CollapsingHeader::new(
                                                        RichText::new("\u{f013}  Variables").size(13.0).strong()  // gear icon
                                                    )
                                                    .default_open(starts_expanded)
                                                    .show(ui, |ui| {
                                                        ui.add_space(4.0);
                                                        for (name, _default_value) in vars {
                                                            let key = &name;
                                                            if !map.contains_key(key) {
                                                                map.insert(
                                                                    key.clone(),
                                                                    Value::from(_default_value),
                                                                );
                                                                plugin_changed = true;
                                                            }
                                                            if let Some(value) = map.get_mut(key) {
                                                                // Special handling for max_latency_us
                                                                if key == "max_latency_us" {
                                                                    let us_value = value.as_f64().unwrap_or(1000.0);
                                                                    let value_key = (plugin.id, "max_latency_value".to_string());
                                                                    let unit_key = (plugin.id, "max_latency_unit".to_string());

                                                                    // Determine display value and unit
                                                                    let (display_value, default_unit) = if us_value >= 1000.0 {
                                                                        (us_value / 1000.0, "ms")
                                                                    } else if us_value >= 1.0 {
                                                                        (us_value, "us")
                                                                    } else {
                                                                        (us_value * 1000.0, "ns")
                                                                    };

                                                                    if !self.number_edit_buffers.contains_key(&value_key) {
                                                                        self.number_edit_buffers.insert(value_key.clone(), display_value.to_string());
                                                                    }
                                                                    if !self.number_edit_buffers.contains_key(&unit_key) {
                                                                        self.number_edit_buffers.insert(unit_key.clone(), default_unit.to_string());
                                                                    }

                                                                    let mut drag_value = self.number_edit_buffers[&value_key].parse::<f64>().unwrap_or(display_value);
                                                                    let mut unit_clone = self.number_edit_buffers[&unit_key].clone();

                                                                    kv_row_wrapped(ui, "max_latency", 140.0, |ui| {
                                                                        let mut changed = false;
                                                                        if ui.add(egui::DragValue::new(&mut drag_value).speed(10.0).clamp_range(1.0..=f64::INFINITY).fixed_decimals(0)).changed() {
                                                                            changed = true;
                                                                        }
                                                                        ui.add_space(4.0);
                                                                        egui::ComboBox::from_id_source((plugin.id, "max_latency_unit"))
                                                                            .selected_text(&unit_clone)
                                                                            .width(40.0)
                                                                            .show_ui(ui, |ui| {
                                                                                if ui.selectable_label(unit_clone == "ns", "ns").clicked() {
                                                                                    unit_clone = "ns".to_string();
                                                                                    changed = true;
                                                                                }
                                                                                if ui.selectable_label(unit_clone == "us", "us").clicked() {
                                                                                    unit_clone = "us".to_string();
                                                                                    changed = true;
                                                                                }
                                                                                if ui.selectable_label(unit_clone == "ms", "ms").clicked() {
                                                                                    unit_clone = "ms".to_string();
                                                                                    changed = true;
                                                                                }
                                                                            });

                                                                        if changed {
                                                                            let us_val = match unit_clone.as_str() {
                                                                                "ms" => drag_value * 1000.0,
                                                                                "us" => drag_value,
                                                                                "ns" => drag_value / 1000.0,
                                                                                _ => drag_value,
                                                                            };
                                                                            *value = Value::from(us_val);
                                                                            plugin_changed = true;
                                                                        }
                                                                    });

                                                                    self.number_edit_buffers.insert(value_key, drag_value.to_string());
                                                                    self.number_edit_buffers.insert(unit_key, unit_clone);
                                                                } else {
                                                                    let buffer_key = (plugin.id, key.clone());
                                                                    let buffer = self
                                                                        .number_edit_buffers
                                                                        .entry(buffer_key)
                                                                        .or_insert_with(|| {
                                                                            let n =
                                                                                value.as_f64().unwrap_or(0.0);
                                                                            let mut text =
                                                                                format_f64_6(n);
                                                                            if !text.contains('.') {
                                                                                text.push_str(".0");
                                                                            }
                                                                            text
                                                                        });
                                                                    kv_row_wrapped(ui, key, 140.0, |ui| {
                                                                        ui.add_sized(
                                                                            [80.0, 0.0],
                                                                            egui::TextEdit::singleline(buffer)
                                                                        ).changed().then(|| {
                                                                            let _ = normalize_numeric_input(buffer);
                                                                            if let Some(parsed) = parse_f64_input(buffer) {
                                                                                let truncated = truncate_f64(parsed);
                                                                                *value = Value::from(truncated);
                                                                                *buffer = format_f64_with_input(buffer, truncated);
                                                                                plugin_changed = true;
                                                                            }
                                                                        });
                                                                    });
                                                                }
                                                            }
                                                        }
                                                    });
                                                }
                                            }
                                            _ => {
                                                ui.label("Config is not an object.");
                                            }
                                        }
                                    }

                                        let (display_schema, ui_schema) = self.plugin_manager.installed_plugins
                                            .iter()
                                            .find(|p| p.manifest.kind == plugin.kind)
                                            .map(|p| (p.display_schema.clone(), p.ui_schema.clone()))
                                            .unwrap_or((None, None));
                                        if let Some(schema) = display_schema.as_ref() {
                                                // Variables section for app plugins
                                                let vars: Vec<String> = if is_app_plugin {
                                                    ui_schema
                                                        .as_ref()
                                                        .map(|schema| {
                                                            schema.fields.iter().map(|f| f.key.clone()).collect()
                                                        })
                                                        .unwrap_or_default()
                                                } else {
                                                    schema.variables.clone()
                                                };
                                                if !vars.is_empty() && is_app_plugin {
                                                    egui::CollapsingHeader::new(
                                                        RichText::new("\u{f0ae}  Variables").size(13.0).strong()
                                                    )
                                                    .default_open(starts_expanded)
                                                    .show(ui, |ui| {
                                                        ui.add_space(4.0);
                                                        let label_w = 140.0;
                                                        let value_w = (ui.available_width() - label_w - 8.0).max(80.0);

                                                        for var_name in &vars {
                                                            let (tx, rx) = mpsc::channel();
                                                            let _ = self.state_sync.logic_tx.send(LogicMessage::GetPluginVariable(plugin.id, var_name.clone(), tx));

                                                            if let Ok(Some(value)) = rx.recv() {
                                                                if plugin.kind == "csv_recorder"
                                                                    && var_name == "columns"
                                                                    && matches!(value, Value::Array(ref arr) if arr.is_empty())
                                                                {
                                                                    continue;
                                                                }
                                                                let field_info = ui_schema.as_ref()
                                                                    .and_then(|schema| schema.fields.iter().find(|f| f.key == *var_name));
                                                                let label = field_info
                                                                    .map(|field| field.label.as_str())
                                                                    .unwrap_or(var_name.as_str());
                                                                let is_filepath = field_info
                                                                    .map(|field| matches!(field.field_type, rtsyn_plugin::ui::FieldType::FilePath { .. }))
                                                                    .unwrap_or(false);

                                                                kv_row_wrapped(ui, label, label_w, |ui| {
                                                                    match &value {
                                                                        Value::String(s) => {
                                                                            let mut text = s.clone();
                                                                            if is_filepath {
                                                                                if text.trim().is_empty() {
                                                                                    text = Self::default_csv_path();
                                                                                    let _ = self.state_sync.logic_tx.send(
                                                                                        LogicMessage::SetPluginVariable(
                                                                                            plugin.id,
                                                                                            var_name.clone(),
                                                                                            Value::String(text.clone()),
                                                                                        ),
                                                                                    );
                                                                                    if let Value::Object(ref mut map) = plugin.config {
                                                                                        map.insert("path".to_string(), Value::String(text.clone()));
                                                                                        map.insert("path_autogen".to_string(), Value::Bool(true));
                                                                                        plugin_changed = true;
                                                                                    }
                                                                                }
                                                                                ui.vertical(|ui| {
                                                                                    ui.add_enabled_ui(false, |ui| {
                                                                                        ui.add_sized(
                                                                                            [value_w, 0.0],
                                                                                            egui::TextEdit::singleline(&mut text),
                                                                                        );
                                                                                    });
                                                                                    if ui.add_sized([value_w, 0.0], egui::Button::new("Browse")).clicked() {
                                                                                        self.csv_path_target_plugin_id = Some(plugin.id);
                                                                                        let (tx, rx) = mpsc::channel();
                                                                                        self.file_dialogs.csv_path_dialog_rx = Some(rx);
                                                                                        spawn_file_dialog_thread(move || {
                                                                                            let file = if has_rt_capabilities() {
                                                                                                zenity_file_dialog("save", None)
                                                                                            } else {
                                                                                                rfd::FileDialog::new().save_file()
                                                                                            };
                                                                                            let _ = tx.send(file);
                                                                                        });
                                                                                    }
                                                                                });
                                                                            } else if let Some(field) = field_info {
                                                                                if let rtsyn_plugin::ui::FieldType::Choice { options } = &field.field_type {
                                                                                    let mut changed = false;
                                                                                    egui::ComboBox::from_id_source((plugin.id, var_name.clone(), "choice"))
                                                                                        .selected_text(text.clone())
                                                                                        .width(value_w)
                                                                                        .show_ui(ui, |ui| {
                                                                                            for option in options {
                                                                                                if ui
                                                                                                    .selectable_value(&mut text, option.clone(), option)
                                                                                                    .clicked()
                                                                                                {
                                                                                                    changed = true;
                                                                                                }
                                                                                            }
                                                                                        });
                                                                                    if changed {
                                                                                        let new_text = text.clone();
                                                                                        let _ = self.state_sync.logic_tx.send(LogicMessage::SetPluginVariable(
                                                                                            plugin.id,
                                                                                            var_name.clone(),
                                                                                            Value::String(new_text.clone())
                                                                                        ));
                                                                                        if let Value::Object(ref mut map) = plugin.config {
                                                                                            map.insert(var_name.clone(), Value::String(new_text));
                                                                                            plugin_changed = true;
                                                                                        }
                                                                                    }
                                                                                } else if ui.add_sized([value_w, 0.0], egui::TextEdit::singleline(&mut text)).changed() {
                                                                                    let new_text = text.clone();
                                                                                    let _ = self.state_sync.logic_tx.send(LogicMessage::SetPluginVariable(
                                                                                        plugin.id,
                                                                                        var_name.clone(),
                                                                                        Value::String(new_text.clone())
                                                                                    ));
                                                                                    if let Value::Object(ref mut map) = plugin.config {
                                                                                        map.insert(var_name.clone(), Value::String(new_text));
                                                                                        if var_name == "path" {
                                                                                            map.insert("path_autogen".to_string(), Value::from(false));
                                                                                        }
                                                                                        plugin_changed = true;
                                                                                    }
                                                                                }
                                                                            } else if ui.add_sized([value_w, 0.0], egui::TextEdit::singleline(&mut text)).changed() {
                                                                                let new_text = text.clone();
                                                                                let _ = self.state_sync.logic_tx.send(LogicMessage::SetPluginVariable(
                                                                                    plugin.id,
                                                                                    var_name.clone(),
                                                                                    Value::String(new_text.clone())
                                                                                ));
                                                                                if let Value::Object(ref mut map) = plugin.config {
                                                                                    map.insert(var_name.clone(), Value::String(new_text));
                                                                                    if var_name == "path" {
                                                                                        map.insert("path_autogen".to_string(), Value::from(false));
                                                                                    }
                                                                                    plugin_changed = true;
                                                                                }
                                                                            }
                                                                        }
                                                                        Value::Bool(b) => {
                                                                            let mut checked = *b;
                                                                            if ui.add_sized([value_w, 0.0], egui::Checkbox::new(&mut checked, "")).changed() {
                                                                                let _ = self.state_sync.logic_tx.send(LogicMessage::SetPluginVariable(plugin.id, var_name.clone(), Value::Bool(checked)));
                                                                                if let Value::Object(ref mut map) = plugin.config {
                                                                                    map.insert(var_name.clone(), Value::Bool(checked));
                                                                                    plugin_changed = true;
                                                                                }
                                                                            }
                                                                        }
                                                                        Value::Number(n) => {
                                                                            let field_info = ui_schema
                                                                                .as_ref()
                                                                                .and_then(|schema| schema.fields.iter().find(|f| f.key == *var_name));

                                                                            let mut handled = false;
                                                                            if let Some(field) = field_info {
                                                                                match &field.field_type {
                                                                                    rtsyn_plugin::ui::FieldType::Integer { min, max, step } => {
                                                                                        let min = *min;
                                                                                        let max = *max;
                                                                                        let mut val = n.as_i64().unwrap_or_else(|| n.as_f64().unwrap_or(0.0).round() as i64);
                                                                                        let range = match (min, max) {
                                                                                            (Some(mn), Some(mx)) => mn..=mx,
                                                                                            (Some(mn), None) => mn..=i64::MAX,
                                                                                            (None, Some(mx)) => i64::MIN..=mx,
                                                                                            (None, None) => i64::MIN..=i64::MAX,
                                                                                        };
                                                                                        if ui.add_sized([value_w, 0.0], egui::DragValue::new(&mut val).speed(*step as f64).clamp_range(range)).changed() {
                                                                                            let _ = self.state_sync.logic_tx.send(LogicMessage::SetPluginVariable(plugin.id, var_name.clone(), Value::from(val)));
                                                                                            if let Value::Object(ref mut map) = plugin.config {
                                                                                                map.insert(var_name.clone(), Value::from(val));
                                                                                                plugin_changed = true;
                                                                                            }
                                                                                        }
                                                                                        handled = true;
                                                                                    }
                                                                                    rtsyn_plugin::ui::FieldType::Float { min, max, step } => {
                                                                                        let min = *min;
                                                                                        let max = *max;
                                                                                        let mut val = n.as_f64().unwrap_or(0.0);
                                                                                        let range = match (min, max) {
                                                                                            (Some(mn), Some(mx)) => mn..=mx,
                                                                                            (Some(mn), None) => mn..=f64::INFINITY,
                                                                                            (None, Some(mx)) => f64::NEG_INFINITY..=mx,
                                                                                            (None, None) => f64::NEG_INFINITY..=f64::INFINITY,
                                                                                        };
                                                                                        if ui.add_sized([value_w, 0.0], egui::DragValue::new(&mut val).speed(*step).clamp_range(range)).changed() {
                                                                                            let _ = self.state_sync.logic_tx.send(LogicMessage::SetPluginVariable(plugin.id, var_name.clone(), Value::from(val)));
                                                                                            if let Value::Object(ref mut map) = plugin.config {
                                                                                                map.insert(var_name.clone(), Value::from(val));
                                                                                                plugin_changed = true;
                                                                                            }
                                                                                            if var_name == "refresh_hz" {
                                                                                                recompute_plotter_needed = true;
                                                                                            }
                                                                                        }
                                                                                        handled = true;
                                                                                    }
                                                                                    _ => {}
                                                                                }
                                                                            }

                                                                            if !handled {
                                                                                if let Some(f) = n.as_f64() {
                                                                                    let mut val = f;
                                                                                    if ui.add_sized([value_w, 0.0], egui::DragValue::new(&mut val)).changed() {
                                                                                        let _ = self.state_sync.logic_tx.send(LogicMessage::SetPluginVariable(plugin.id, var_name.clone(), Value::from(val)));
                                                                                        if let Value::Object(ref mut map) = plugin.config {
                                                                                            map.insert(var_name.clone(), Value::from(val));
                                                                                            plugin_changed = true;
                                                                                        }
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                        Value::Array(arr) => {
                                                                            if let Some(field) = field_info {
                                                                                if let rtsyn_plugin::ui::FieldType::DynamicList { item_type, add_label } = &field.field_type {
                                                                                    let mut items: Vec<String> = arr
                                                                                        .iter()
                                                                                        .map(|v| v.as_str().unwrap_or("").to_string())
                                                                                        .collect();
                                                                                    let mut list_changed = false;

                                                                                    ui.vertical(|ui| {
                                                                                        let mut idx = 0usize;
                                                                                        while idx < items.len() {
                                                                                            let mut value = items[idx].clone();
                                                                                            let mut remove_row = false;
                                                                                            ui.horizontal(|ui| {
                                                                                                match &**item_type {
                                                                                                    rtsyn_plugin::ui::FieldType::Text { .. } => {
                                                                                                        if ui.add_sized([value_w, 0.0], egui::TextEdit::singleline(&mut value)).changed() {
                                                                                                            items[idx] = value.clone();
                                                                                                            list_changed = true;
                                                                                                        }
                                                                                                    }
                                                                                                    _ => {
                                                                                                        ui.label("Unsupported list item type");
                                                                                                    }
                                                                                                }
                                                                                                if ui.small_button("X").clicked() {
                                                                                                    remove_row = true;
                                                                                                }
                                                                                            });
                                                                                            if remove_row {
                                                                                                items.remove(idx);
                                                                                                list_changed = true;
                                                                                            } else {
                                                                                                idx += 1;
                                                                                            }
                                                                                        }
                                                                                        if !(plugin.kind == "csv_recorder" && var_name == "columns") {
                                                                                            if ui.small_button(add_label).clicked() {
                                                                                                items.push(String::new());
                                                                                                list_changed = true;
                                                                                            }
                                                                                        }
                                                                                    });

                                                                                    if list_changed {
                                                                                        let new_value = Value::Array(
                                                                                            items.iter().cloned().map(Value::String).collect()
                                                                                        );
                                                                                        let _ = self.state_sync.logic_tx.send(
                                                                                            LogicMessage::SetPluginVariable(plugin.id, var_name.clone(), new_value.clone())
                                                                                        );
                                                                                        if let Value::Object(ref mut map) = plugin.config {
                                                                                            map.insert(var_name.clone(), new_value);
                                                                                            if var_name == "columns" {
                                                                                                map.insert("input_count".to_string(), Value::from(items.len() as u64));
                                                                                                pending_prune = Some((plugin.id, items.len()));
                                                                                                pending_enforce_connection = true;
                                                                                            }
                                                                                            plugin_changed = true;
                                                                                        }
                                                                                    }
                                                                            }
                                                                        }
                                                                        }
                                                                        _ => {}
                                                                    }
                                                                });
                                                                ui.add_space(4.0);
                                                            }
                                                        }
                                                    });
                                                }

                                                // Inputs second
                                                if !schema.inputs.is_empty() {
                                                    egui::CollapsingHeader::new(
                                                        RichText::new("\u{f090}  Inputs").size(13.0).strong()  // sign-in icon with space
                                                    )
                                                    .default_open(starts_expanded)
                                                    .show(ui, |ui| {
                                                        ui.add_space(4.0);
                                                        for input_name in &schema.inputs {
                                                            let value = input_values
                                                                .get(&(plugin.id, input_name.clone()))
                                                                .copied()
                                                                .unwrap_or(0.0);
                                                            let mut value_text = format!("{value:.4}");
                                                            kv_row_wrapped(ui, input_name, 140.0, |ui| {
                                                                ui.add_enabled_ui(false, |ui| {
                                                                    ui.add_sized(
                                                                        [80.0, 0.0],
                                                                        egui::TextEdit::singleline(&mut value_text)
                                                                    );
                                                                });
                                                            });
                                                            ui.add_space(4.0);
                                                        }
                                                    });
                                                }

                                                // Outputs third
                                                if !schema.outputs.is_empty() {
                                                    egui::CollapsingHeader::new(
                                                        RichText::new("\u{f08b}  Outputs").size(13.0).strong()  // sign-out icon with space
                                                    )
                                                    .default_open(starts_expanded)
                                                    .show(ui, |ui| {
                                                        ui.add_space(4.0);
                                                        for output_name in &schema.outputs {
                                                            let value = computed_outputs
                                                                .get(&(plugin.id, output_name.clone()))
                                                                .copied()
                                                                .unwrap_or(0.0);
                                                            let mut value_text = if value == 0.0 {
                                                                "0".to_string()
                                                            } else if (value.fract() - 0.0).abs() < f64::EPSILON {
                                                                format!("{value:.0}")
                                                            } else if value.abs() < 1e-3 {
                                                                format!("{value:.3e}")
                                                            } else {
                                                                format!("{value:.6}")
                                                            };
                                                            kv_row_wrapped(ui, output_name, 140.0, |ui| {
                                                                ui.add_enabled_ui(false, |ui| {
                                                                    ui.add_sized(
                                                                        [80.0, 0.0],
                                                                        egui::TextEdit::singleline(&mut value_text)
                                                                    );
                                                                });
                                                            });
                                                            ui.add_space(4.0);
                                                        }
                                                    });
                                                }

                                                if !schema.variables.is_empty() {
                                                    egui::CollapsingHeader::new(
                                                        RichText::new("\u{f085}  Internal variables").size(13.0).strong()
                                                    )
                                                    .default_open(starts_expanded)
                                                    .show(ui, |ui| {
                                                        ui.add_space(4.0);
                                                        for var_name in &schema.variables {
                                                            let value = internal_variable_values
                                                                .get(&(plugin.id, var_name.clone()))
                                                                .cloned()
                                                                .unwrap_or_else(|| {
                                                                    if matches!(plugin.kind.as_str(), "csv_recorder" | "live_plotter") {
                                                                        match var_name.as_str() {
                                                                            "input_count" => serde_json::Value::from(0),
                                                                            "running" => serde_json::Value::from(false),
                                                                            _ => serde_json::Value::from(0.0),
                                                                        }
                                                                    } else {
                                                                        serde_json::Value::from(0.0)
                                                                    }
                                                                });
                                                            let mut value_text = match value {
                                                                serde_json::Value::Bool(v) => v.to_string(),
                                                                serde_json::Value::Number(ref num) => {
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
                                                            kv_row_wrapped(ui, var_name, 140.0, |ui| {
                                                                ui.add_enabled_ui(false, |ui| {
                                                                    ui.add_sized(
                                                                        [80.0, 0.0],
                                                                        egui::TextEdit::singleline(&mut value_text)
                                                                    );
                                                                });
                                                            });
                                                            ui.add_space(4.0);
                                                        }
                                                    });
                                                }
                                            }

                                        if plugin.kind == "value_viewer" {
                                            let value =
                                                viewer_values.get(&plugin.id).copied().unwrap_or(0.0);
                                            ui.add_space(4.0);
                                            ui.separator();
                                            ui.label(RichText::new("Last value").strong());
                                            ui.add_space(4.0);
                                            let mut value_text = format!("{value:.4}");
                                            ui.add_enabled(
                                                false,
                                                egui::TextEdit::singleline(&mut value_text)
                                                    .desired_width(80.0),
                                            );
                                        }
                                        });  // close push_id
                                    });  // close ScrollArea.show
                            });  // close scope

                            // Controls at bottom
                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(8.0);

                            let mut controls_changed = false;
                            ui.horizontal(|ui| {
                                let mut blocked_start = false;
                                        let supports_start_stop = behavior.supports_start_stop;
                                        if supports_start_stop {
                                            let label = if plugin.running { "Stop" } else { "Start" };
                                            if styled_button(ui, label).clicked() {
                                                if !plugin.running {
                                                    let plugin_input_ports = connected_input_ports
                                                        .get(&plugin.id)
                                                        .cloned()
                                                        .unwrap_or_default();
                                                    let plugin_output_ports = connected_output_ports
                                                        .get(&plugin.id)
                                                        .cloned()
                                                        .unwrap_or_default();

                                                    let missing_inputs: Vec<String> = behavior
                                                        .start_requires_connected_inputs
                                                        .iter()
                                                        .filter(|port| !plugin_input_ports.contains(*port))
                                                        .cloned()
                                                        .collect();
                                                    if !missing_inputs.is_empty() {
                                                        pending_info = Some(format!(
                                                            "Cannot start: missing input connections on ports: {}",
                                                            missing_inputs.join(", ")
                                                        ));
                                                        blocked_start = true;
                                                    }

                                                    if !blocked_start {
                                                        let missing_outputs: Vec<String> = behavior
                                                            .start_requires_connected_outputs
                                                            .iter()
                                                            .filter(|port| !plugin_output_ports.contains(*port))
                                                            .cloned()
                                                            .collect();
                                                        if !missing_outputs.is_empty() {
                                                            pending_info = Some(format!(
                                                                "Cannot start: missing output connections on ports: {}",
                                                                missing_outputs.join(", ")
                                                            ));
                                                            blocked_start = true;
                                                        }
                                                    }
                                                }
                                                if !blocked_start
                                                    && plugin.kind == "csv_recorder"
                                                    && !plugin.running
                                                {
                                                    if let Value::Object(ref mut map) = plugin.config {
                                                        let mut path = map
                                                            .get("path")
                                                            .and_then(|v| v.as_str())
                                                            .unwrap_or("")
                                                            .to_string();
                                                        let path_autogen = map
                                                            .get("path_autogen")
                                                            .and_then(|v| v.as_bool())
                                                            .unwrap_or(true);
                                                        if path_autogen || path.trim().is_empty() {
                                                            path = Self::default_csv_path();
                                                        }
                                                        if let Some(parent) = Path::new(&path).parent() {
                                                            let _ = fs::create_dir_all(parent);
                                                        }
                                                        map.insert("path".to_string(), Value::String(path));
                                                    }
                                                }
                                                if !blocked_start {
                                                    plugin.running = !plugin.running;
                                                    pending_running.push((plugin.id, plugin.running));
                                                    controls_changed = true;

                                                    // Auto-open plotter when starting live_plotter
                                                    if plugin.kind == "live_plotter" && plugin.running {
                                                        let plotter = self.plotter_manager.plotters.entry(plugin.id).or_insert_with(|| {
                                                            Arc::new(Mutex::new(LivePlotter::new(plugin.id)))
                                                        });
                                                        if let Ok(mut plotter) = plotter.lock() {
                                                            plotter.open = true;
                                                        }
                                                        recompute_plotter_needed = true;
                                                    }

                                                    if plugin.kind == "csv_recorder" && plugin.running {
                                                        pending_workspace_update = true;
                                                    }
                                                }
                                            }
                                        }
                                        let supports_restart = behavior.supports_restart;
                                        if supports_restart {
                                            if styled_button(ui, "Restart").clicked() {
                                                if plugin.kind == "comedi_daq" {
                                                    if let Value::Object(ref mut map) = plugin.config {
                                                        let next_nonce = map
                                                            .get("scan_nonce")
                                                            .and_then(|v| v.as_u64())
                                                            .unwrap_or(0)
                                                            .saturating_add(1);
                                                        map.insert(
                                                            "scan_nonce".to_string(),
                                                            Value::from(next_nonce),
                                                        );
                                                        map.insert(
                                                            "scan_devices".to_string(),
                                                            Value::Bool(false),
                                                        );
                                                        workspace_changed = true;
                                                    }
                                                }
                                                pending_restart.push(plugin.id);
                                            }
                                        }
                                        if behavior.supports_apply {
                                            if styled_button(ui, "Modify").clicked() {
                                                pending_info = Some(
                                                    "Modify/apply behavior is declared but not implemented yet."
                                                        .to_string(),
                                                );
                                            }
                                        }
                                    });

                                    if controls_changed {
                                        workspace_changed = true;
                                    }
                        });
                    });
                });

            let clamped_pos = egui::pos2(
                response.response.rect.min.x.clamp(min_x, max_x),
                response.response.rect.min.y.clamp(min_y, max_y),
            );
            self.plugin_positions.insert(plugin.id, clamped_pos);
            self.plugin_rects.insert(plugin.id, response.response.rect);
            if ctx.input(|i| {
                i.pointer
                    .button_double_clicked(egui::PointerButton::Primary)
            }) {
                if response.response.hovered() && !self.confirm_dialog.open {
                    // Toggle selection
                    if self.selected_plugin_id == Some(plugin.id) {
                        self.selected_plugin_id = None;
                    } else {
                        self.selected_plugin_id = Some(plugin.id);
                    }
                }
            }
            if response.response.clicked() || response.response.dragged() {
                ctx.move_to_top(response.response.layer_id);
            }
            if ctx.input(|i| i.pointer.button_released(egui::PointerButton::Secondary)) {
                if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                    if response.response.rect.contains(pos) && response.response.hovered() {
                        if self.confirm_dialog.open {
                            self.plugin_context_menu = None;
                        } else {
                            self.plugin_context_menu = Some((plugin.id, pos, ctx.frame_nr()));
                        }
                    }
                }
            }
            if plugin_changed {
                workspace_changed = true;
            }
            index += 1;
        }
        if pending_workspace_update {
            let _ = self.state_sync.logic_tx.send(LogicMessage::UpdateWorkspace(
                self.workspace_manager.workspace.clone(),
            ));
        }
        for (plugin_id, running) in pending_running {
            // Mark plugin as stopped BEFORE sending message to prevent one more update
            if !running {
                if let Some(plugin) = self
                    .workspace_manager
                    .workspace
                    .plugins
                    .iter_mut()
                    .find(|p| p.id == plugin_id)
                {
                    plugin.running = false;
                }
            }

            let _ = self
                .state_sync
                .logic_tx
                .send(LogicMessage::SetPluginRunning(plugin_id, running));
        }
        if recompute_plotter_needed {
            self.recompute_plotter_ui_hz();
        }
        for plugin_id in pending_restart {
            self.restart_plugin(plugin_id);
        }
        if let Some((plugin_id, count)) = pending_prune {
            prune_extendable_inputs_plugin_connections(
                &mut self.workspace_manager.workspace.connections,
                plugin_id,
                count,
            );
        }
        if pending_enforce_connection {
            self.enforce_connection_dependent();
        }
        if workspace_changed {
            self.mark_workspace_dirty();
        }

        if let Some(id) = remove_id {
            let name_by_kind: HashMap<String, String> = self
                .plugin_manager
                .installed_plugins
                .iter()
                .map(|plugin| (plugin.manifest.kind.clone(), plugin.manifest.name.clone()))
                .collect();
            let label = self
                .workspace_manager
                .workspace
                .plugins
                .iter()
                .find(|plugin| plugin.id == id)
                .map(|plugin| {
                    let display_name = name_by_kind
                        .get(&plugin.kind)
                        .cloned()
                        .unwrap_or_else(|| Self::display_kind(&plugin.kind));
                    format!("#{} {}", plugin.id, display_name)
                })
                .unwrap_or_else(|| format!("#{id}"));
            self.show_confirm(
                "Confirm removal",
                &format!("Remove plugin {label} from the workspace?"),
                "Remove",
                ConfirmAction::RemovePlugin(id),
            );
        }

        if let Some(message) = pending_info {
            self.show_info("Plugin", &message);
        }
    }

    fn render_plugin_preview(
        ui: &mut egui::Ui,
        manifest: &PluginManifest,
        inputs_override: Option<Vec<String>>,
        plugin_kind: &str,
        _plugin_config: &serde_json::Value,
        _plugin_running: bool,
        installed_plugins: &[InstalledPlugin],
    ) {
        egui::Frame::none()
            .inner_margin(egui::Margin::symmetric(8.0, 6.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let name_w = (ui.available_width() - 64.0).max(120.0);
                    ui.add_sized(
                        [name_w, 0.0],
                        egui::Label::new(RichText::new(&manifest.name).strong().size(18.0))
                            .truncate(true),
                    );
                    if let Some(version) = &manifest.version {
                        ui.label(RichText::new(format!("v{version}")).color(egui::Color32::GRAY));
                    }
                });
                if let Some(description) = &manifest.description {
                    let description = Self::normalize_preview_description(description);
                    ui.add(egui::Label::new(RichText::new(description)).wrap(true));
                }

                ui.add_space(6.0);
                ui.label(RichText::new("Ports").strong());
                let inputs = inputs_override.unwrap_or_else(|| {
                    installed_plugins
                        .iter()
                        .find(|p| p.manifest.kind == manifest.kind)
                        .map(|p| {
                            p.display_schema
                                .as_ref()
                                .map(|s| s.inputs.clone())
                                .unwrap_or_else(|| p.metadata_inputs.clone())
                        })
                        .unwrap_or_default()
                });
                let mut inputs_label = inputs.join(", ");
                let is_extendable = matches!(plugin_kind, "csv_recorder" | "live_plotter");
                if is_extendable {
                    if inputs_label.is_empty() {
                        inputs_label = "incremental".to_string();
                    } else {
                        inputs_label = format!("{inputs_label} (incremental)");
                    }
                }
                let outputs = installed_plugins
                    .iter()
                    .find(|p| p.manifest.kind == manifest.kind)
                    .map(|p| {
                        p.display_schema
                            .as_ref()
                            .map(|s| s.outputs.join(", "))
                            .unwrap_or_else(|| p.metadata_outputs.join(", "))
                    })
                    .unwrap_or_default();
                egui::Grid::new(("plugin_preview_ports", manifest.kind.as_str()))
                    .num_columns(2)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("Inputs:");
                        ui.add(
                            egui::Label::new(if inputs_label.is_empty() {
                                "none"
                            } else {
                                &inputs_label
                            })
                            .wrap(true),
                        );
                        ui.end_row();
                        ui.label("Outputs:");
                        ui.add(
                            egui::Label::new(if outputs.is_empty() { "none" } else { &outputs })
                                .wrap(true),
                        );
                        ui.end_row();
                    });

                if let Some(plugin) = installed_plugins
                    .iter()
                    .find(|p| p.manifest.kind == manifest.kind)
                {
                    if !plugin.metadata_variables.is_empty() {
                        ui.add_space(6.0);
                        ui.label(RichText::new("Variables").strong());
                        for (name, value) in &plugin.metadata_variables {
                            ui.label(format!("{} = {}", name, value));
                        }
                    }
                }
            });
    }

    fn render_preview_action_panel(
        ui: &mut egui::Ui,
        full_h: f32,
        right_w: f32,
        body: impl FnOnce(&mut egui::Ui),
    ) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height(full_h)
            .min_scrolled_height(full_h)
            .show(ui, |ui| {
                // Keep a stable width baseline so button centering does not drift with preview content.
                ui.set_min_width(right_w);
                ui.set_max_width(right_w);
                body(ui);
            });
    }

    fn live_plotter_inputs_override(&self) -> Option<Vec<String>> {
        let plugin = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .find(|p| p.kind == "live_plotter")?;
        let count = plugin
            .config
            .get("input_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as usize;
        Some((0..count).map(|idx| format!("in_{idx}")).collect())
    }

    fn normalize_preview_description(description: &str) -> String {
        let tokens: Vec<&str> = description.split_whitespace().collect();
        if tokens.is_empty() {
            return String::new();
        }

        let mut rebuilt: Vec<String> = Vec::with_capacity(tokens.len());
        let mut spaced_letters: Vec<&str> = Vec::new();
        let flush_spaced = |spaced: &mut Vec<&str>, out: &mut Vec<String>| {
            if spaced.is_empty() {
                return;
            }
            if spaced.len() >= 3 {
                out.push(spaced.iter().copied().collect::<String>());
            } else {
                for token in spaced.iter() {
                    out.push((*token).to_string());
                }
            }
            spaced.clear();
        };

        for token in tokens {
            let is_single_letter =
                token.chars().count() == 1 && token.chars().all(|c| c.is_alphanumeric());
            if is_single_letter {
                spaced_letters.push(token);
                continue;
            }
            flush_spaced(&mut spaced_letters, &mut rebuilt);
            rebuilt.push(token.to_string());
        }
        flush_spaced(&mut spaced_letters, &mut rebuilt);

        rebuilt.join(" ")
    }

    fn truncate_plugin_list_name(name: &str, max_chars: usize) -> String {
        let mut chars = name.chars();
        let mut out = String::new();
        for _ in 0..max_chars {
            if let Some(ch) = chars.next() {
                out.push(ch);
            } else {
                return out;
            }
        }
        if chars.next().is_some() {
            out.push_str("...");
        }
        out
    }

    pub(crate) fn render_plugins_window(&mut self, ctx: &egui::Context) {
        if !self.windows.plugins_open {
            return;
        }

        let mut window_open = self.windows.plugins_open;
        let window_size = egui::vec2(760.0, 440.0);
        let default_pos = Self::center_window(ctx, window_size);
        let response = egui::Window::new("Add Plugin")
            .open(&mut window_open)
            .resizable(false)
            .default_pos(default_pos)
            .default_size(window_size)
            .min_size(window_size)
            .max_size(window_size)
            .fixed_size(window_size)
            .show(ctx, |ui| {
                let total_w = ui.available_width();
                let left_w = (total_w * 0.52).max(260.0);
                let right_w = (total_w - left_w - 10.0).max(220.0);
                let full_h = ui.available_height();
                let search_h = 34.0;
                let list_h = (full_h - search_h - 16.0).max(120.0);
                let mut selected: Option<usize> = None;

                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(left_w, full_h),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.scope(|ui| {
                                let mut style = ui.style().as_ref().clone();
                                style.visuals.extreme_bg_color = egui::Color32::from_gray(50);
                                style.visuals.widgets.inactive.bg_fill =
                                    egui::Color32::from_gray(50);
                                style.visuals.widgets.hovered.bg_fill =
                                    egui::Color32::from_gray(55);
                                style.visuals.widgets.active.bg_fill = egui::Color32::from_gray(60);
                                ui.set_style(style);
                                ui.add_sized(
                                    [200.0, 24.0],
                                    egui::TextEdit::singleline(&mut self.windows.plugin_search)
                                        .hint_text("Search plugins"),
                                );
                            });
                            ui.add_space(6.0);
                            ui.separator();
                            ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), list_h),
                                egui::Layout::top_down(egui::Align::LEFT),
                                |ui| {
                                    egui::ScrollArea::vertical()
                                        .auto_shrink([false, false])
                                        .max_height(list_h)
                                        .min_scrolled_height(list_h)
                                        .show(ui, |ui| {
                                            for (idx, installed) in self
                                                .plugin_manager
                                                .installed_plugins
                                                .iter()
                                                .enumerate()
                                            {
                                                let label = installed.manifest.name.clone();
                                                if !self.windows.plugin_search.trim().is_empty()
                                                    && !label.to_lowercase().contains(
                                                        &self.windows.plugin_search.to_lowercase(),
                                                    )
                                                {
                                                    continue;
                                                }
                                                let display_label =
                                                    Self::truncate_plugin_list_name(&label, 44);
                                                let response = ui
                                                    .allocate_ui_with_layout(
                                                        egui::vec2(ui.available_width(), 22.0),
                                                        egui::Layout::left_to_right(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            ui.add(egui::SelectableLabel::new(
                                                                self.windows.plugin_selected_index
                                                                    == Some(idx),
                                                                egui::RichText::new(display_label)
                                                                    .size(14.0),
                                                            ))
                                                        },
                                                    )
                                                    .inner;
                                                if response.clicked() {
                                                    selected = Some(idx);
                                                }
                                            }
                                        });
                                },
                            );
                        },
                    );

                    ui.add(egui::Separator::default().vertical());

                    ui.allocate_ui_with_layout(
                        egui::vec2(right_w, full_h),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            Self::render_preview_action_panel(ui, full_h, right_w, |ui| {
                                if let Some(idx) = self.windows.plugin_selected_index {
                                    if let Some(installed) =
                                        self.plugin_manager.installed_plugins.get(idx)
                                    {
                                        let inputs_override = self.live_plotter_inputs_override();
                                        Self::render_plugin_preview(
                                            ui,
                                            &installed.manifest,
                                            inputs_override,
                                            &installed.manifest.kind,
                                            &serde_json::Value::Object(serde_json::Map::new()),
                                            false,
                                            &self.plugin_manager.installed_plugins,
                                        );
                                        ui.add_space(12.0);
                                        ui.horizontal_centered(|ui| {
                                            if styled_button(ui, "Add to runtime").clicked() {
                                                self.add_installed_plugin(idx);
                                            }
                                        });
                                    }
                                } else {
                                    ui.label("Select a plugin to preview.");
                                }
                            });
                        },
                    );
                });

                if let Some(idx) = selected {
                    self.windows.plugin_selected_index = Some(idx);
                }
            });
        if let Some(response) = response {
            self.window_rects.push(response.response.rect);
            if !self.confirm_dialog.open
                && (response.response.clicked() || response.response.dragged())
            {
                ctx.move_to_top(response.response.layer_id);
            }
            if self.pending_window_focus == Some(WindowFocus::Plugins) {
                ctx.move_to_top(response.response.layer_id);
                self.pending_window_focus = None;
            }
        }
        self.windows.plugins_open = window_open;
    }

    fn render_new_plugin_fields_section(
        ui: &mut egui::Ui,
        id_key: &str,
        title: &str,
        add_label: &str,
        fields: &mut Vec<PluginFieldDraft>,
        show_default_value: bool,
    ) -> bool {
        let mut changed = false;
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(title).strong());
                    ui.add_space(8.0);
                    if ui.button(add_label).clicked() {
                        fields.push(PluginFieldDraft::default());
                        changed = true;
                    }
                });
                ui.add_space(6.0);
            });
        let mut idx = 0usize;
        while idx < fields.len() {
            let mut remove = false;
            ui.horizontal(|ui| {
                let mut style = ui.style().as_ref().clone();
                style.visuals.extreme_bg_color = egui::Color32::from_rgb(58, 58, 62);
                style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(58, 58, 62);
                style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(66, 66, 72);
                style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(72, 72, 78);
                ui.set_style(style);

                if ui
                    .add_sized(
                        [200.0, 26.0],
                        egui::TextEdit::singleline(&mut fields[idx].name).hint_text("Name"),
                    )
                    .changed()
                {
                    changed = true;
                }
                let previous_type = fields[idx].type_name.clone();
                egui::ComboBox::from_id_source((id_key, idx, "type"))
                    .width(96.0)
                    .selected_text(fields[idx].type_name.clone())
                    .show_ui(ui, |ui| {
                        for ty in Self::NEW_PLUGIN_TYPES {
                            if ui
                                .selectable_label(fields[idx].type_name == ty, ty)
                                .clicked()
                            {
                                fields[idx].type_name = ty.to_string();
                                changed = true;
                            }
                        }
                    });
                if previous_type != fields[idx].type_name {
                    let prev_default = Self::plugin_creator_default_by_type(&previous_type);
                    if fields[idx].default_value.trim().is_empty()
                        || fields[idx].default_value == prev_default
                    {
                        fields[idx].default_value =
                            Self::plugin_creator_default_by_type(&fields[idx].type_name)
                                .to_string();
                    }
                }
                if show_default_value {
                    let default_hint =
                        Self::plugin_creator_default_by_type(&fields[idx].type_name).to_string();
                    if ui
                        .add_sized(
                            [120.0, 26.0],
                            egui::TextEdit::singleline(&mut fields[idx].default_value)
                                .hint_text(default_hint),
                        )
                        .changed()
                    {
                        changed = true;
                    }
                }
                if ui.small_button("Remove").clicked() {
                    remove = true;
                }
            });
            if remove {
                fields.remove(idx);
                changed = true;
            } else {
                idx += 1;
            }
        }
        changed
    }

    pub(crate) fn render_new_plugin_window(&mut self, ctx: &egui::Context) {
        if !self.windows.new_plugin_open {
            return;
        }

        let viewport_id = egui::ViewportId::from_hash_of("new_plugin_window");
        let builder = egui::ViewportBuilder::default()
            .with_title("New Plugin")
            .with_inner_size([760.0, 620.0])
            .with_close_button(true);
        ctx.show_viewport_immediate(viewport_id, builder, |ctx, class| {
            if class == egui::ViewportClass::Embedded {
                return;
            }
            if ctx.input(|i| i.viewport().close_requested()) {
                self.windows.new_plugin_open = false;
                return;
            }

            egui::CentralPanel::default().show(ctx, |ui| {
                let mut changed = false;
                ui.heading(RichText::new("New Plugin").size(24.0));
                ui.label(
                    "Create a Rust/C/C++ scaffold with structured inputs, outputs and runtime variables.",
                );
                ui.add_space(10.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Frame::group(ui.style())
                        .inner_margin(egui::Margin::same(12.0))
                        .show(ui, |ui| {
                            ui.label(RichText::new("1. Name and language").strong());
                            ui.add_space(8.0);
                            ui.scope(|ui| {
                                let mut style = ui.style().as_ref().clone();
                                style.visuals.extreme_bg_color = egui::Color32::from_rgb(58, 58, 62);
                                style.visuals.widgets.inactive.bg_fill =
                                    egui::Color32::from_rgb(58, 58, 62);
                                style.visuals.widgets.hovered.bg_fill =
                                    egui::Color32::from_rgb(66, 66, 72);
                                style.visuals.widgets.active.bg_fill =
                                    egui::Color32::from_rgb(72, 72, 78);
                                ui.set_style(style);
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 28.0],
                                        egui::TextEdit::singleline(&mut self.new_plugin_draft.name)
                                            .hint_text("Plugin name (required)"),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                            });
                            ui.add_space(8.0);
                            egui::ComboBox::from_id_source("new_plugin_language")
                                .selected_text(self.new_plugin_draft.language.clone())
                                .show_ui(ui, |ui| {
                                    for lang in ["rust", "c", "cpp"] {
                                        if ui
                                            .selectable_label(
                                                self.new_plugin_draft.language == lang,
                                                lang,
                                            )
                                            .clicked()
                                        {
                                            self.new_plugin_draft.language = lang.to_string();
                                            changed = true;
                                        }
                                    }
                                });
                        });

                    ui.add_space(10.0);
                    egui::Frame::group(ui.style())
                        .inner_margin(egui::Margin::same(12.0))
                        .show(ui, |ui| {
                            ui.label(RichText::new("2. Main characteristics").strong());
                            ui.add_space(8.0);
                            ui.scope(|ui| {
                                let mut style = ui.style().as_ref().clone();
                                style.visuals.extreme_bg_color = egui::Color32::from_rgb(58, 58, 62);
                                style.visuals.widgets.inactive.bg_fill =
                                    egui::Color32::from_rgb(58, 58, 62);
                                style.visuals.widgets.hovered.bg_fill =
                                    egui::Color32::from_rgb(66, 66, 72);
                                style.visuals.widgets.active.bg_fill =
                                    egui::Color32::from_rgb(72, 72, 78);
                                ui.set_style(style);
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 110.0],
                                        egui::TextEdit::multiline(
                                            &mut self.new_plugin_draft.main_characteristics,
                                        )
                                        .hint_text("Describe what the plugin should do"),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                            });
                        });

                    ui.add_space(10.0);
                    egui::Frame::group(ui.style())
                        .inner_margin(egui::Margin::same(12.0))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("3. Variables, Inputs, Outputs, Internal Variables")
                                    .strong(),
                            );
                            ui.small("Each section lets you add rows with a name and a type.");
                        });
                    ui.add_space(8.0);
                    changed |= Self::render_new_plugin_fields_section(
                        ui,
                        "new_plugin_variables",
                        "Variables",
                        "Add Variable",
                        &mut self.new_plugin_draft.variables,
                        true,
                    );
                    ui.add_space(8.0);
                    changed |= Self::render_new_plugin_fields_section(
                        ui,
                        "new_plugin_inputs",
                        "Inputs",
                        "Add Input",
                        &mut self.new_plugin_draft.inputs,
                        false,
                    );
                    ui.add_space(8.0);
                    changed |= Self::render_new_plugin_fields_section(
                        ui,
                        "new_plugin_outputs",
                        "Outputs",
                        "Add Output",
                        &mut self.new_plugin_draft.outputs,
                        false,
                    );
                    ui.add_space(8.0);
                    changed |= Self::render_new_plugin_fields_section(
                        ui,
                        "new_plugin_internal",
                        "Internal Variables",
                        "Add Internal Variable",
                        &mut self.new_plugin_draft.internal_variables,
                        false,
                    );

                    ui.add_space(10.0);
                    egui::Frame::group(ui.style())
                        .inner_margin(egui::Margin::same(12.0))
                        .show(ui, |ui| {
                            ui.label(RichText::new("4. Options").strong());
                            ui.add_space(6.0);
                            if ui
                                .checkbox(
                                    &mut self.new_plugin_draft.autostart,
                                    "Autostart (loads_started)",
                                )
                                .changed()
                            {
                                changed = true;
                            }
                            if ui
                                .checkbox(
                                    &mut self.new_plugin_draft.supports_start_stop,
                                    "Start/Stop controls",
                                )
                                .changed()
                            {
                                changed = true;
                            }
                            if ui
                                .checkbox(
                                    &mut self.new_plugin_draft.supports_restart,
                                    "Reset button (supports_restart)",
                                )
                                .changed()
                            {
                                changed = true;
                            }
                            if ui
                                .checkbox(
                                    &mut self.new_plugin_draft.supports_apply,
                                    "Modify button (supports_apply)",
                                )
                                .changed()
                            {
                                changed = true;
                            }
                            if ui
                                .checkbox(
                                    &mut self.new_plugin_draft.external_window,
                                    "Open as external window",
                                )
                                .changed()
                            {
                                changed = true;
                            }
                            if ui
                                .checkbox(
                                    &mut self.new_plugin_draft.starts_expanded,
                                    "Starts expanded",
                                )
                                .changed()
                            {
                                changed = true;
                            }
                            ui.add_space(6.0);
                            ui.label("Required connected input ports to start (comma-separated)");
                            if ui
                                .add(
                                    egui::TextEdit::singleline(
                                        &mut self.new_plugin_draft.required_input_ports_csv,
                                    )
                                    .hint_text("e.g. in_0,in_1"),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                            ui.label("Required connected output ports to start (comma-separated)");
                            if ui
                                .add(
                                    egui::TextEdit::singleline(
                                        &mut self.new_plugin_draft.required_output_ports_csv,
                                    )
                                    .hint_text("e.g. out_0"),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                        });

                    ui.add_space(10.0);
                    let can_create = !self.new_plugin_draft.name.trim().is_empty();
                    ui.label(RichText::new("5. Create").strong());
                    let create_response = ui.add_enabled_ui(can_create, |ui| {
                        styled_button(ui, "Create")
                    });
                    if create_response.inner.clicked() {
                        self.open_plugin_creator_folder_dialog();
                    }
                    if !can_create {
                        ui.label("Plugin name is required before creating.");
                    }
                    if let Some(path) = &self.plugin_creator_last_path {
                        ui.small(format!("Last destination: {}", path.display()));
                    }
                });

                if changed {
                    self.mark_workspace_dirty();
                }
            });
        });
    }

    fn is_app_plugins_path(path: &std::path::Path) -> bool {
        path.components().any(|c| c.as_os_str() == "app_plugins")
    }

    pub(crate) fn render_manage_plugins_window(&mut self, ctx: &egui::Context) {
        if !self.windows.manage_plugins_open {
            return;
        }

        let mut window_open = self.windows.manage_plugins_open;
        let window_size = egui::vec2(760.0, 440.0);
        let default_pos = Self::center_window(ctx, window_size);
        let mut install_selected: Option<(BuildAction, String)> = None;
        let mut reinstall_selected: Option<(BuildAction, String)> = None;
        let mut uninstall_selected: Option<usize> = None;
        let mut rescan = false;

        let response = egui::Window::new("Manage plugins")
            .open(&mut window_open)
            .resizable(false)
            .default_pos(default_pos)
            .default_size(window_size)
            .min_size(window_size)
            .max_size(window_size)
            .fixed_size(window_size)
            .show(ctx, |ui| {
                let total_w = ui.available_width();
                let left_w = (total_w * 0.52).max(260.0);
                let right_w = (total_w - left_w - 10.0).max(220.0);
                let full_h = ui.available_height();
                let footer_h = 72.0;
                let search_h = 34.0;
                let list_h = (full_h - search_h - footer_h - 16.0).max(120.0);

                let installed_kinds: HashSet<String> = self
                    .plugin_manager
                    .installed_plugins
                    .iter()
                    .map(|plugin| plugin.manifest.kind.clone())
                    .collect();

                let mut selected: Option<usize> = None;

                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(left_w, full_h),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.scope(|ui| {
                                let mut style = ui.style().as_ref().clone();
                                style.visuals.extreme_bg_color = egui::Color32::from_gray(50);
                                style.visuals.widgets.inactive.bg_fill =
                                    egui::Color32::from_gray(50);
                                style.visuals.widgets.hovered.bg_fill =
                                    egui::Color32::from_gray(55);
                                style.visuals.widgets.active.bg_fill = egui::Color32::from_gray(60);
                                ui.set_style(style);
                                ui.add_sized(
                                    [200.0, 24.0],
                                    egui::TextEdit::singleline(
                                        &mut self.windows.manage_plugin_search,
                                    )
                                    .hint_text("Search plugins"),
                                );
                            });
                            ui.add_space(6.0);
                            ui.separator();

                            ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), list_h),
                                egui::Layout::top_down(egui::Align::LEFT),
                                |ui| {
                                    egui::ScrollArea::vertical()
                                        .auto_shrink([false, false])
                                        .max_height(list_h)
                                        .min_scrolled_height(list_h)
                                        .show(ui, |ui| {
                                            for (idx, detected) in self
                                                .plugin_manager
                                                .detected_plugins
                                                .iter()
                                                .enumerate()
                                            {
                                                let label = detected.manifest.name.clone();
                                                if !self
                                                    .windows
                                                    .manage_plugin_search
                                                    .trim()
                                                    .is_empty()
                                                    && !label.to_lowercase().contains(
                                                        &self
                                                            .windows
                                                            .manage_plugin_search
                                                            .to_lowercase(),
                                                    )
                                                {
                                                    continue;
                                                }
                                                let response = ui
                                                    .allocate_ui_with_layout(
                                                        egui::vec2(ui.available_width(), 22.0),
                                                        egui::Layout::left_to_right(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            ui.add(egui::SelectableLabel::new(
                                                                self.windows
                                                                    .manage_plugin_selected_index
                                                                    == Some(idx),
                                                                egui::RichText::new(label)
                                                                    .size(14.0),
                                                            ))
                                                        },
                                                    )
                                                    .inner;
                                                if response.clicked() {
                                                    selected = Some(idx);
                                                }
                                            }
                                        });
                                },
                            );

                            ui.separator();
                            ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), footer_h),
                                egui::Layout::top_down(egui::Align::LEFT),
                                |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label("Browse plugin folder");
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if styled_button(ui, "Browse...").clicked() {
                                                    self.open_install_dialog();
                                                }
                                            },
                                        );
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("Rescan default plugins folder");
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if styled_button(ui, "Rescan").clicked() {
                                                    rescan = true;
                                                }
                                            },
                                        );
                                    });
                                },
                            );
                        },
                    );

                    ui.add(egui::Separator::default().vertical());

                    ui.allocate_ui_with_layout(
                        egui::vec2(right_w, full_h),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            Self::render_preview_action_panel(ui, full_h, right_w, |ui| {
                                if let Some(idx) = self.windows.manage_plugin_selected_index {
                                    if let Some(detected) =
                                        self.plugin_manager.detected_plugins.get(idx)
                                    {
                                        let inputs_override = self.live_plotter_inputs_override();
                                        Self::render_plugin_preview(
                                            ui,
                                            &detected.manifest,
                                            inputs_override,
                                            &detected.manifest.kind,
                                            &serde_json::Value::Object(serde_json::Map::new()),
                                            false,
                                            &self.plugin_manager.installed_plugins,
                                        );

                                        let is_installed =
                                            installed_kinds.contains(&detected.manifest.kind);
                                        ui.add_space(12.0);
                                        if !is_installed {
                                            ui.horizontal_centered(|ui| {
                                                if ui
                                                    .add_enabled(
                                                        self.build_dialog.rx.is_none(),
                                                        egui::Button::new("Install")
                                                            .min_size(BUTTON_SIZE),
                                                    )
                                                    .clicked()
                                                {
                                                    install_selected = Some((
                                                        BuildAction::Install {
                                                            path: detected.path.clone(),
                                                            removable: true,
                                                            persist: true,
                                                        },
                                                        detected.manifest.name.clone(),
                                                    ));
                                                }
                                            });
                                        } else if let Some(installed_idx) =
                                            self.plugin_manager.installed_plugins.iter().position(
                                                |p| p.manifest.kind == detected.manifest.kind,
                                            )
                                        {
                                            let removable = self
                                                .plugin_manager
                                                .installed_plugins
                                                .get(installed_idx)
                                                .map(|p| p.removable)
                                                .unwrap_or(false);

                                            ui.horizontal_centered(|ui| {
                                                if ui
                                                    .add_enabled(
                                                        removable && self.build_dialog.rx.is_none(),
                                                        egui::Button::new("Reinstall")
                                                            .min_size(BUTTON_SIZE),
                                                    )
                                                    .clicked()
                                                {
                                                    if let Some(installed) = self
                                                        .plugin_manager
                                                        .installed_plugins
                                                        .get(installed_idx)
                                                    {
                                                        reinstall_selected = Some((
                                                            BuildAction::Reinstall {
                                                                kind: installed
                                                                    .manifest
                                                                    .kind
                                                                    .clone(),
                                                                path: installed.path.clone(),
                                                            },
                                                            installed.manifest.name.clone(),
                                                        ));
                                                    }
                                                }

                                                if ui
                                                    .add_enabled(
                                                        removable,
                                                        egui::Button::new("Uninstall")
                                                            .min_size(BUTTON_SIZE),
                                                    )
                                                    .clicked()
                                                {
                                                    uninstall_selected = Some(installed_idx);
                                                }
                                            });
                                        }
                                    }
                                } else {
                                    ui.label("Select a plugin to preview.");
                                }
                            });
                        },
                    );
                });

                if let Some(idx) = selected {
                    self.windows.manage_plugin_selected_index = Some(idx);
                }
            });

        if let Some(response) = response {
            self.window_rects.push(response.response.rect);
            if !self.confirm_dialog.open
                && (response.response.clicked() || response.response.dragged())
            {
                ctx.move_to_top(response.response.layer_id);
            }
            if self.pending_window_focus == Some(WindowFocus::ManagePlugins) {
                ctx.move_to_top(response.response.layer_id);
                self.pending_window_focus = None;
            }
        }

        if rescan {
            self.load_installed_plugins();
            self.scan_detected_plugins();
        }
        if let Some((action, label)) = install_selected {
            self.start_plugin_build(action, label);
        }
        if let Some((action, label)) = reinstall_selected {
            self.start_plugin_build(action, label);
        }
        if let Some(idx) = uninstall_selected {
            self.show_confirm(
                "Uninstall plugin",
                "Uninstall this plugin?",
                "Uninstall",
                ConfirmAction::UninstallPlugin(idx),
            );
        }

        self.windows.manage_plugins_open = window_open;
    }

    pub(crate) fn render_install_plugins_window(&mut self, ctx: &egui::Context) {
        if !self.windows.install_plugins_open {
            return;
        }

        let mut window_open = self.windows.install_plugins_open;
        let window_size = egui::vec2(760.0, 440.0);
        let default_pos = Self::center_window(ctx, window_size);
        let mut install_selected: Option<(BuildAction, String)> = None;
        let mut reinstall_selected: Option<(BuildAction, String)> = None;
        let mut rescan = false;

        let response = egui::Window::new("Install plugin")
            .open(&mut window_open)
            .resizable(false)
            .default_pos(default_pos)
            .default_size(window_size)
            .min_size(window_size)
            .max_size(window_size)
            .fixed_size(window_size)
            .show(ctx, |ui| {
                let total_w = ui.available_width();
                let left_w = (total_w * 0.52).max(260.0);
                let right_w = (total_w - left_w - 10.0).max(220.0);
                let full_h = ui.available_height();
                let footer_h = 72.0;
                let search_h = 34.0;
                let list_h = (full_h - search_h - footer_h - 16.0).max(120.0);
                let installed_kinds: HashSet<String> = self
                    .plugin_manager
                    .installed_plugins
                    .iter()
                    .map(|plugin| plugin.manifest.kind.clone())
                    .collect();
                let mut selected: Option<usize> = None;

                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(left_w, full_h),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.scope(|ui| {
                                let mut style = ui.style().as_ref().clone();
                                style.visuals.extreme_bg_color = egui::Color32::from_gray(50);
                                style.visuals.widgets.inactive.bg_fill =
                                    egui::Color32::from_gray(50);
                                style.visuals.widgets.hovered.bg_fill =
                                    egui::Color32::from_gray(55);
                                style.visuals.widgets.active.bg_fill = egui::Color32::from_gray(60);
                                ui.set_style(style);
                                ui.add_sized(
                                    [200.0, 24.0],
                                    egui::TextEdit::singleline(
                                        &mut self.windows.install_plugin_search,
                                    )
                                    .hint_text("Search plugins"),
                                );
                            });
                            ui.add_space(6.0);
                            ui.separator();

                            ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), list_h),
                                egui::Layout::top_down(egui::Align::LEFT),
                                |ui| {
                                    egui::ScrollArea::vertical()
                                        .auto_shrink([false, false])
                                        .max_height(list_h)
                                        .min_scrolled_height(list_h)
                                        .show(ui, |ui| {
                                            for (idx, detected) in self
                                                .plugin_manager
                                                .detected_plugins
                                                .iter()
                                                .enumerate()
                                            {
                                                if Self::is_app_plugins_path(&detected.path) {
                                                    continue;
                                                }
                                                let label = detected.manifest.name.clone();
                                                if !self
                                                    .windows
                                                    .install_plugin_search
                                                    .trim()
                                                    .is_empty()
                                                    && !label.to_lowercase().contains(
                                                        &self
                                                            .windows
                                                            .install_plugin_search
                                                            .to_lowercase(),
                                                    )
                                                {
                                                    continue;
                                                }
                                                let response = ui
                                                    .allocate_ui_with_layout(
                                                        egui::vec2(ui.available_width(), 22.0),
                                                        egui::Layout::left_to_right(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            ui.add(egui::SelectableLabel::new(
                                                                self.windows.install_selected_index
                                                                    == Some(idx),
                                                                egui::RichText::new(label)
                                                                    .size(14.0),
                                                            ))
                                                        },
                                                    )
                                                    .inner;
                                                if response.clicked() {
                                                    selected = Some(idx);
                                                }
                                            }
                                        });
                                },
                            );

                            ui.separator();
                            ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), footer_h),
                                egui::Layout::top_down(egui::Align::LEFT),
                                |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label("Browse plugin folder");
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if styled_button(ui, "Browse...").clicked() {
                                                    self.open_install_dialog();
                                                }
                                            },
                                        );
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("Rescan default plugins folder");
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if styled_button(ui, "Rescan").clicked() {
                                                    rescan = true;
                                                }
                                            },
                                        );
                                    });
                                },
                            );
                        },
                    );

                    ui.add(egui::Separator::default().vertical());

                    ui.allocate_ui_with_layout(
                        egui::vec2(right_w, full_h),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            Self::render_preview_action_panel(ui, full_h, right_w, |ui| {
                                if let Some(idx) = self.windows.install_selected_index {
                                    if let Some(detected) =
                                        self.plugin_manager.detected_plugins.get(idx)
                                    {
                                        if Self::is_app_plugins_path(&detected.path) {
                                            ui.label("Select a plugin to preview.");
                                            return;
                                        }
                                        let inputs_override = self.live_plotter_inputs_override();
                                        Self::render_plugin_preview(
                                            ui,
                                            &detected.manifest,
                                            inputs_override,
                                            &detected.manifest.kind,
                                            &serde_json::Value::Object(serde_json::Map::new()),
                                            false,
                                            &self.plugin_manager.installed_plugins,
                                        );

                                        let is_installed =
                                            installed_kinds.contains(&detected.manifest.kind);
                                        ui.add_space(12.0);
                                        if !is_installed {
                                            ui.horizontal_centered(|ui| {
                                                if ui
                                                    .add_enabled(
                                                        self.build_dialog.rx.is_none(),
                                                        egui::Button::new("Install")
                                                            .min_size(BUTTON_SIZE),
                                                    )
                                                    .clicked()
                                                {
                                                    install_selected = Some((
                                                        BuildAction::Install {
                                                            path: detected.path.clone(),
                                                            removable: true,
                                                            persist: true,
                                                        },
                                                        detected.manifest.name.clone(),
                                                    ));
                                                }
                                            });
                                        } else if let Some(installed_idx) =
                                            self.plugin_manager.installed_plugins.iter().position(
                                                |p| p.manifest.kind == detected.manifest.kind,
                                            )
                                        {
                                            let removable = self
                                                .plugin_manager
                                                .installed_plugins
                                                .get(installed_idx)
                                                .map(|p| p.removable)
                                                .unwrap_or(false);
                                            ui.horizontal_centered(|ui| {
                                                if ui
                                                    .add_enabled(
                                                        removable && self.build_dialog.rx.is_none(),
                                                        egui::Button::new("Reinstall")
                                                            .min_size(BUTTON_SIZE),
                                                    )
                                                    .clicked()
                                                {
                                                    if let Some(installed) = self
                                                        .plugin_manager
                                                        .installed_plugins
                                                        .get(installed_idx)
                                                    {
                                                        reinstall_selected = Some((
                                                            BuildAction::Reinstall {
                                                                kind: installed
                                                                    .manifest
                                                                    .kind
                                                                    .clone(),
                                                                path: installed.path.clone(),
                                                            },
                                                            installed.manifest.name.clone(),
                                                        ));
                                                    }
                                                }
                                            });
                                        }
                                    }
                                } else {
                                    ui.label("Select a plugin to preview.");
                                }
                            });
                        },
                    );
                });

                if let Some(idx) = selected {
                    self.windows.install_selected_index = Some(idx);
                }
            });

        if let Some(response) = response {
            self.window_rects.push(response.response.rect);
            if !self.confirm_dialog.open
                && (response.response.clicked() || response.response.dragged())
            {
                ctx.move_to_top(response.response.layer_id);
            }
            if self.pending_window_focus == Some(WindowFocus::InstallPlugins) {
                ctx.move_to_top(response.response.layer_id);
                self.pending_window_focus = None;
            }
        }

        if rescan {
            self.load_installed_plugins();
            self.scan_detected_plugins();
        }
        if let Some((action, label)) = install_selected {
            self.start_plugin_build(action, label);
        }
        if let Some((action, label)) = reinstall_selected {
            self.start_plugin_build(action, label);
        }

        self.windows.install_plugins_open = window_open;
    }

    pub(crate) fn render_uninstall_plugins_window(&mut self, ctx: &egui::Context) {
        if !self.windows.uninstall_plugins_open {
            return;
        }

        let mut window_open = self.windows.uninstall_plugins_open;
        let window_size = egui::vec2(760.0, 440.0);
        let default_pos = Self::center_window(ctx, window_size);
        let mut uninstall_selected: Option<usize> = None;

        let response = egui::Window::new("Uninstall plugin")
            .open(&mut window_open)
            .resizable(false)
            .default_pos(default_pos)
            .default_size(window_size)
            .min_size(window_size)
            .max_size(window_size)
            .fixed_size(window_size)
            .show(ctx, |ui| {
                let total_w = ui.available_width();
                let left_w = (total_w * 0.52).max(260.0);
                let right_w = (total_w - left_w - 10.0).max(220.0);
                let full_h = ui.available_height();
                let search_h = 34.0;
                let list_h = (full_h - search_h - 10.0).max(120.0);
                let mut selected: Option<usize> = None;

                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(left_w, full_h),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.scope(|ui| {
                                let mut style = ui.style().as_ref().clone();
                                style.visuals.extreme_bg_color = egui::Color32::from_gray(50);
                                style.visuals.widgets.inactive.bg_fill =
                                    egui::Color32::from_gray(50);
                                style.visuals.widgets.hovered.bg_fill =
                                    egui::Color32::from_gray(55);
                                style.visuals.widgets.active.bg_fill = egui::Color32::from_gray(60);
                                ui.set_style(style);
                                ui.add_sized(
                                    [200.0, 24.0],
                                    egui::TextEdit::singleline(
                                        &mut self.windows.uninstall_plugin_search,
                                    )
                                    .hint_text("Search plugins"),
                                );
                            });
                            ui.add_space(6.0);
                            ui.separator();

                            ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), list_h),
                                egui::Layout::top_down(egui::Align::LEFT),
                                |ui| {
                                    egui::ScrollArea::vertical()
                                        .auto_shrink([false, false])
                                        .max_height(list_h)
                                        .min_scrolled_height(list_h)
                                        .show(ui, |ui| {
                                            for (idx, installed) in self
                                                .plugin_manager
                                                .installed_plugins
                                                .iter()
                                                .enumerate()
                                            {
                                                if !installed.removable
                                                    || Self::is_app_plugins_path(&installed.path)
                                                {
                                                    continue;
                                                }
                                                let label = installed.manifest.name.clone();
                                                if !self
                                                    .windows
                                                    .uninstall_plugin_search
                                                    .trim()
                                                    .is_empty()
                                                    && !label.to_lowercase().contains(
                                                        &self
                                                            .windows
                                                            .uninstall_plugin_search
                                                            .to_lowercase(),
                                                    )
                                                {
                                                    continue;
                                                }
                                                let response = ui
                                                    .allocate_ui_with_layout(
                                                        egui::vec2(ui.available_width(), 22.0),
                                                        egui::Layout::left_to_right(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            ui.add(egui::SelectableLabel::new(
                                                                self.windows
                                                                    .uninstall_selected_index
                                                                    == Some(idx),
                                                                egui::RichText::new(label)
                                                                    .size(14.0),
                                                            ))
                                                        },
                                                    )
                                                    .inner;
                                                if response.clicked() {
                                                    selected = Some(idx);
                                                }
                                            }
                                        });
                                },
                            );
                        },
                    );

                    ui.add(egui::Separator::default().vertical());

                    ui.allocate_ui_with_layout(
                        egui::vec2(right_w, full_h),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            Self::render_preview_action_panel(ui, full_h, right_w, |ui| {
                                if let Some(idx) = self.windows.uninstall_selected_index {
                                    if let Some(installed) =
                                        self.plugin_manager.installed_plugins.get(idx)
                                    {
                                        if !installed.removable
                                            || Self::is_app_plugins_path(&installed.path)
                                        {
                                            ui.label("Select a plugin to preview.");
                                            return;
                                        }
                                        let inputs_override = self.live_plotter_inputs_override();
                                        Self::render_plugin_preview(
                                            ui,
                                            &installed.manifest,
                                            inputs_override,
                                            &installed.manifest.kind,
                                            &serde_json::Value::Object(serde_json::Map::new()),
                                            false,
                                            &self.plugin_manager.installed_plugins,
                                        );
                                        ui.add_space(12.0);
                                        ui.horizontal_centered(|ui| {
                                            if ui
                                                .add_enabled(
                                                    installed.removable,
                                                    egui::Button::new("Uninstall")
                                                        .min_size(BUTTON_SIZE),
                                                )
                                                .clicked()
                                            {
                                                uninstall_selected = Some(idx);
                                            }
                                        });
                                    }
                                } else {
                                    ui.label("Select a plugin to preview.");
                                }
                            });
                        },
                    );
                });

                if let Some(idx) = selected {
                    self.windows.uninstall_selected_index = Some(idx);
                }
            });

        if let Some(response) = response {
            self.window_rects.push(response.response.rect);
            if !self.confirm_dialog.open
                && (response.response.clicked() || response.response.dragged())
            {
                ctx.move_to_top(response.response.layer_id);
            }
            if self.pending_window_focus == Some(WindowFocus::UninstallPlugins) {
                ctx.move_to_top(response.response.layer_id);
                self.pending_window_focus = None;
            }
        }

        if let Some(idx) = uninstall_selected {
            self.show_confirm(
                "Uninstall plugin",
                "Uninstall this plugin?",
                "Uninstall",
                ConfirmAction::UninstallPlugin(idx),
            );
        }

        self.windows.uninstall_plugins_open = window_open;
    }

    pub(crate) fn render_plugin_context_menu(&mut self, ctx: &egui::Context) {
        let Some((plugin_id, pos, opened_frame)) = self.plugin_context_menu else {
            return;
        };

        let mut close_menu = false;
        let menu_response = egui::Area::new(egui::Id::new("plugin_context_menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    let row_height = ui.text_style_height(&egui::TextStyle::Button) + 6.0;
                    let menu_width = 160.0;
                    let add_clicked = ui
                        .allocate_ui_with_layout(
                            egui::vec2(menu_width, row_height),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.add(egui::SelectableLabel::new(false, "Add connections"))
                                    .clicked()
                            },
                        )
                        .inner;
                    if add_clicked {
                        self.open_connection_editor(plugin_id, ConnectionEditMode::Add);
                        close_menu = true;
                    }
                    let remove_clicked = ui
                        .allocate_ui_with_layout(
                            egui::vec2(menu_width, row_height),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.add(egui::SelectableLabel::new(false, "Remove connections"))
                                    .clicked()
                            },
                        )
                        .inner;
                    if remove_clicked {
                        self.open_connection_editor(plugin_id, ConnectionEditMode::Remove);
                        close_menu = true;
                    }
                    let config_clicked = ui
                        .allocate_ui_with_layout(
                            egui::vec2(menu_width, row_height),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.add(egui::SelectableLabel::new(false, "Plugin config"))
                                    .clicked()
                            },
                        )
                        .inner;
                    if config_clicked {
                        self.windows.plugin_config_open = true;
                        self.windows.plugin_config_id = Some(plugin_id);
                        close_menu = true;
                        self.pending_window_focus = Some(WindowFocus::PluginConfig);
                    }
                    let duplicate_clicked = ui
                        .allocate_ui_with_layout(
                            egui::vec2(menu_width, row_height),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.add(egui::SelectableLabel::new(false, "Duplicate plugin"))
                                    .clicked()
                            },
                        )
                        .inner;
                    if duplicate_clicked {
                        self.duplicate_plugin(plugin_id);
                        close_menu = true;
                    }
                });
            });

        let pointer_pos = ctx.input(|i| i.pointer.interact_pos());
        let hovered = pointer_pos
            .map(|pos| menu_response.response.rect.contains(pos))
            .unwrap_or(false);
        let close_click = ctx.input(|i| {
            i.pointer.primary_clicked() || i.pointer.primary_down() || i.pointer.secondary_clicked()
        });
        if close_click && !hovered && ctx.frame_nr() != opened_frame {
            close_menu = true;
        }

        if close_menu {
            self.plugin_context_menu = None;
        }
    }

    fn plugin_config_window_size(plugin_kind: &str) -> egui::Vec2 {
        match plugin_kind {
            "csv_recorder" => egui::vec2(520.0, 360.0),
            "live_plotter" => egui::vec2(420.0, 240.0),
            _ => egui::vec2(320.0, 180.0),
        }
    }

    fn render_plugin_config_contents(
        &mut self,
        ui: &mut egui::Ui,
        plugin_id: u64,
        name_by_kind: &HashMap<String, String>,
    ) {
        let Some(plugin_index) = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .position(|p| p.id == plugin_id)
        else {
            ui.label("Plugin not found.");
            return;
        };

        let plugin_kind = self.workspace_manager.workspace.plugins[plugin_index]
            .kind
            .clone();
        let display_name = name_by_kind
            .get(&plugin_kind)
            .cloned()
            .unwrap_or_else(|| Self::display_kind(&plugin_kind));
        let mut plugin_changed = false;

        ui.horizontal(|ui| {
            let (id_rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(id_rect, 8.0, egui::Color32::from_gray(60));
            ui.painter().text(
                id_rect.center(),
                egui::Align2::CENTER_CENTER,
                plugin_id.to_string(),
                egui::FontId::proportional(12.0),
                egui::Color32::from_rgb(200, 200, 210),
            );
            ui.label(RichText::new(display_name).strong().size(16.0));
        });
        ui.add_space(6.0);

        let plugin = &mut self.workspace_manager.workspace.plugins[plugin_index];
        let mut priority = plugin.priority;
        kv_row_wrapped(ui, "Priority", 140.0, |ui| {
            if ui
                .add_sized([90.0, 0.0], egui::DragValue::new(&mut priority).speed(1))
                .changed()
            {
                plugin_changed = true;
            }
        });
        priority = priority.clamp(0, 99);
        if plugin.priority != priority {
            plugin.priority = priority;
            plugin_changed = true;
        }

        if plugin_changed {
            self.mark_workspace_dirty();
        }
    }

    pub(crate) fn render_plugin_config_window(&mut self, ctx: &egui::Context) {
        let name_by_kind: HashMap<String, String> = self
            .plugin_manager
            .installed_plugins
            .iter()
            .map(|plugin| (plugin.manifest.kind.clone(), plugin.manifest.name.clone()))
            .collect();

        let external_plugin_ids: Vec<u64> = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .filter(|plugin| {
                self.plugin_manager
                    .plugin_behaviors
                    .get(&plugin.kind)
                    .map(|b| b.external_window)
                    .unwrap_or(false)
            })
            .map(|plugin| plugin.id)
            .collect();
        for plugin_id in external_plugin_ids {
            let plugin_kind = self
                .workspace_manager
                .workspace
                .plugins
                .iter()
                .find(|p| p.id == plugin_id)
                .map(|p| p.kind.as_str())
                .unwrap_or("");
            let window_size = Self::plugin_config_window_size(plugin_kind);
            let display_name = name_by_kind
                .get(plugin_kind)
                .cloned()
                .unwrap_or_else(|| Self::display_kind(plugin_kind));
            let viewport_id = egui::ViewportId::from_hash_of(("plugin_config", plugin_id));
            let builder = egui::ViewportBuilder::default()
                .with_title(format!("{display_name} #{plugin_id}"))
                .with_inner_size([window_size.x, window_size.y])
                .with_close_button(false);
            ctx.show_viewport_immediate(viewport_id, builder, |ctx, class| {
                if class == egui::ViewportClass::Embedded {
                    return;
                }
                egui::CentralPanel::default().show(ctx, |ui| {
                    self.render_plugin_config_contents(ui, plugin_id, &name_by_kind);
                });
            });
        }

        if !self.windows.plugin_config_open {
            self.windows.plugin_config_id = None;
            return;
        }

        let plugin_id = match self.windows.plugin_config_id {
            Some(id) => id,
            None => {
                self.windows.plugin_config_open = false;
                return;
            }
        };
        let plugin_kind = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .find(|p| p.id == plugin_id)
            .map(|p| p.kind.as_str())
            .unwrap_or("");
        let external_window = self
            .plugin_manager
            .plugin_behaviors
            .get(plugin_kind)
            .map(|b| b.external_window)
            .unwrap_or(false);
        if external_window {
            self.windows.plugin_config_open = false;
            self.windows.plugin_config_id = None;
            return;
        }

        let window_size = Self::plugin_config_window_size(plugin_kind);
        let mut open = self.windows.plugin_config_open;
        let default_pos = Self::center_window(ctx, window_size);
        let response = egui::Window::new("Plugin config")
            .open(&mut open)
            .resizable(false)
            .default_pos(default_pos)
            .default_size(window_size)
            .fixed_size(window_size)
            .show(ctx, |ui| {
                self.render_plugin_config_contents(ui, plugin_id, &name_by_kind);
            });
        if let Some(response) = response {
            self.window_rects.push(response.response.rect);
            if !self.confirm_dialog.open
                && (response.response.clicked() || response.response.dragged())
            {
                ctx.move_to_top(response.response.layer_id);
            }
            if self.pending_window_focus == Some(WindowFocus::PluginConfig) {
                ctx.move_to_top(response.response.layer_id);
                self.pending_window_focus = None;
            }
        }
        self.windows.plugin_config_open = open;
        if !self.windows.plugin_config_open {
            self.windows.plugin_config_id = None;
        }
    }
}
