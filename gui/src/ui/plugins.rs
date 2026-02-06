use super::*;
use crate::utils::{format_f64_with_input, normalize_numeric_input, parse_f64_input};
use crate::WindowFocus;
use crate::{BuildAction, LivePlotter, has_rt_capabilities, spawn_file_dialog_thread, zenity_file_dialog};
use std::sync::{Arc, Mutex};
use std::sync::mpsc;

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

    fn open_csv_path_dialog(&mut self, plugin_id: u64) {
        if self.file_dialogs.csv_path_dialog_rx.is_some() {
            self.show_info("CSV", "Dialog already open.");
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.file_dialogs.csv_path_dialog_rx = Some(rx);
        self.csv_path_target_plugin_id = Some(plugin_id);
        crate::spawn_file_dialog_thread(move || {
            let file = if crate::has_rt_capabilities() {
                crate::zenity_file_dialog("save", None)
            } else {
                rfd::FileDialog::new().save_file()
            };
            let _ = tx.send(file);
        });
    }

    pub(crate) fn open_manage_plugins(&mut self) {
        self.windows.manage_plugins_open = true;
        self.scan_detected_plugins();
        self.pending_window_focus = Some(WindowFocus::ManagePlugins);
    }

    pub(crate) fn open_plugins(&mut self) {
        self.windows.plugins_open = true;
        self.windows.plugin_selected_index = None;
        self.windows.organize_selected_index = None;
        self.pending_window_focus = Some(WindowFocus::Plugins);
    }

    pub(crate) fn render_plugin_cards(&mut self, ctx: &egui::Context, panel_rect: egui::Rect) {
        let mut pending_info: Option<String> = None;
        let incoming_connections: HashSet<u64> = self
            .workspace_manager.workspace
            .connections
            .iter()
            .map(|conn| conn.to_plugin)
            .collect();
        let name_by_kind: HashMap<String, String> = self
            .plugin_manager.installed_plugins
            .iter()
            .map(|plugin| (plugin.manifest.kind.clone(), plugin.manifest.name.clone()))
            .collect();
        let __manifest_by_kind: HashMap<String, PluginManifest> = self
            .plugin_manager.installed_plugins
            .iter()
            .map(|plugin| (plugin.manifest.kind.clone(), plugin.manifest.clone()))
            .collect();
        let metadata_by_kind: HashMap<String, Vec<(String, f64)>> = self
            .plugin_manager.installed_plugins
            .iter()
            .map(|plugin| (plugin.manifest.kind.clone(), plugin.metadata_variables.clone()))
            .collect();
        let computed_outputs = self.state_sync.computed_outputs.clone();
        let input_values = self.state_sync.input_values.clone();
        let _internal_variable_values = self.state_sync.internal_variable_values.clone();
        let viewer_values = self.state_sync.viewer_values.clone();
        let mut remove_id: Option<u64> = None;
        let mut pending_running: Vec<(u64, bool)> = Vec::new();
        let mut pending_restart: Vec<u64> = Vec::new();
        let mut pending_workspace_update = false;

        let mut index = 0usize;
        let max_per_row = ((panel_rect.width() / 240.0).floor() as usize).max(1);
        let mut workspace_changed = false;
        let mut recompute_plotter_needed = false;
        let right_down = ctx.input(|i| i.pointer.secondary_down());
        for plugin in &mut self.workspace_manager.workspace.plugins {
            let col = index % max_per_row;
            let row = index / max_per_row;
            let default_pos = panel_rect.min
                + egui::vec2(12.0 + (col as f32 * 240.0), 12.0 + (row as f32 * 140.0));
            let pos = self
                .plugin_positions
                .get(&plugin.id)
                .cloned()
                .unwrap_or(default_pos);
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
                    let card_width = 280.0;
                    ui.set_width(card_width);
                    
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
                                ui.label(RichText::new(display_name).size(15.0).strong());
                                
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
                                    .max_height(400.0)
                                    .drag_to_scroll(false)
                                    .show(ui, |ui| {
                                        ui.push_id(("plugin_content", plugin.id), |ui| {
                                        ui.style_mut().spacing.item_spacing = egui::vec2(0.0, 6.0);
                                    
                                    if plugin.kind != "csv_recorder" {
                                        match plugin.config {
                                            Value::Object(ref mut map) => {
                                                if plugin.kind == "comedi_daq" {
                                                    ui.label(RichText::new("Comedi Configuration").strong().size(13.0));
                                                    ui.add_space(4.0);

                                                    egui::Grid::new(("comedi_config_grid", plugin.id))
                                                    .num_columns(2)
                                                    .min_col_width(110.0)
                                                    .spacing([10.0, 6.0])
                                                    .show(ui, |ui| {
                                                        ui.label("Scan:");
                                                        let mut rescan_devices = false;
                                                        if ui.button("Scan Channels").clicked() {
                                                            let next = map
                                                                .get("scan_nonce")
                                                                .and_then(|v| v.as_u64())
                                                                .unwrap_or(0)
                                                                .saturating_add(1);
                                                            map.insert(
                                                                "scan_nonce".to_string(),
                                                                Value::from(next),
                                                            );
                                                            plugin_changed = true;
                                                            rescan_devices = true;
                                                        }
                                                        ui.end_row();

                                                        ui.label("Device:");
                                                        let current_path = map
                                                            .get("device_path")
                                                            .and_then(|v| v.as_str())
                                                            .unwrap_or("/dev/comedi0")
                                                            .to_string();
                                                        let mut devices: Vec<String> = Vec::new();
                                                        if let Ok(entries) = std::fs::read_dir("/dev") {
                                                            for entry in entries.flatten() {
                                                                if let Ok(name) = entry.file_name().into_string() {
                                                                    if name.starts_with("comedi") && !name.contains("_subd") {
                                                                        devices.push(format!("/dev/{name}"));
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        devices.sort();
                                                        devices.dedup();
                                                        if devices.is_empty() {
                                                            devices.push("(no devices found)".to_string());
                                                        } else if !devices.contains(&current_path) {
                                                            devices.insert(0, current_path.clone());
                                                        }
                                                        if rescan_devices
                                                            && !devices.is_empty()
                                                            && devices[0].starts_with("/dev/")
                                                            && !devices.contains(&current_path)
                                                        {
                                                            map.insert(
                                                                "device_path".to_string(),
                                                                Value::from(devices[0].clone()),
                                                            );
                                                            plugin_changed = true;
                                                        }
                                                        let selected_text = if devices.len() == 1
                                                            && devices[0] == "(no devices found)"
                                                        {
                                                            "(no devices found)".to_string()
                                                        } else {
                                                            current_path.clone()
                                                        };
                                                        let mut selected = selected_text.clone();
                                                        let resp = egui::ComboBox::from_id_source(("comedi_device_combo", plugin.id))
                                                            .selected_text(&selected_text)
                                                            .show_ui(ui, |ui| {
                                                                let mut changed = false;
                                                                for dev in &devices {
                                                                    if ui.selectable_value(&mut selected, dev.clone(), dev).changed() {
                                                                        changed = true;
                                                                    }
                                                                }
                                                                changed
                                                            });
                                                        if resp.inner.unwrap_or(false)
                                                            && selected != current_path
                                                            && selected.starts_with("/dev/")
                                                        {
                                                            map.insert(
                                                                "device_path".to_string(),
                                                                Value::from(selected),
                                                            );
                                                            plugin_changed = true;
                                                        }
                                                        ui.end_row();
                                                    });
                                            } else {
                                                let vars = metadata_by_kind
                                                    .get(&plugin.kind)
                                                    .cloned()
                                                    .unwrap_or_default();
                                                if !vars.is_empty() {
                                                    egui::CollapsingHeader::new(
                                                        RichText::new("\u{f013}  Variables").size(13.0).strong()  // gear icon
                                                    )
                                                    .default_open(true)
                                                    .show(ui, |ui| {
                                                        ui.add_space(4.0);
                                                        for (name, __default_value) in vars {
                                                            let key = &name;
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
                                                                            format_f64_6(
                                                                                value.as_f64().unwrap_or(0.0),
                                                                            )
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
                                        }
                                        _ => {
                                            ui.label("Config is not an object.");
                                        }
                                    }
                                }

                                        if let Some(installed) = self.plugin_manager.installed_plugins.iter().find(|p| p.manifest.kind == plugin.kind) {
                                            if let Some(schema) = &installed.display_schema {
                                                // Variables section for app plugins
                                                let is_app_plugin = matches!(plugin.kind.as_str(), "csv_recorder" | "live_plotter" | "performance_monitor");
                                                if !schema.variables.is_empty() && is_app_plugin {
                                                    egui::CollapsingHeader::new(
                                                        RichText::new("\u{f0ae}  Variables").size(13.0).strong()
                                                    )
                                                    .default_open(true)
                                                    .show(ui, |ui| {
                                                        ui.add_space(4.0);
                                                        
                                                        for var_name in &schema.variables {
                                                            let (tx, rx) = mpsc::channel();
                                                            let _ = self.state_sync.logic_tx.send(LogicMessage::GetPluginVariable(plugin.id, var_name.clone(), tx));
                                                            
                                                            if let Ok(Some(value)) = rx.recv() {
                                                                // Check if this is a FilePath field from ui_schema
                                                                let is_filepath = installed.ui_schema.as_ref()
                                                                    .and_then(|schema| schema.fields.iter().find(|f| f.key == *var_name))
                                                                    .map(|field| matches!(field.field_type, rtsyn_plugin::ui::FieldType::FilePath { .. }))
                                                                    .unwrap_or(false);
                                                                
                                                                kv_row_wrapped(ui, var_name, 140.0, |ui| {
                                                                    match &value {
                                                                        Value::String(s) => {
                                                                            let mut text = s.clone();
                                                                            let width = if is_filepath { 80.0 } else { 80.0 };
                                                                            if ui.add(egui::TextEdit::singleline(&mut text).desired_width(width)).changed() {
                                                                                let _ = self.state_sync.logic_tx.send(LogicMessage::SetPluginVariable(plugin.id, var_name.clone(), Value::String(text)));
                                                                            }
                                                                            if is_filepath {
                                                                                ui.add_space(2.0);
                                                                                if ui.small_button("...").clicked() {
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
                                                                            }
                                                                        }
                                                                        Value::Bool(b) => {
                                                                            let mut checked = *b;
                                                                            if ui.checkbox(&mut checked, "").changed() {
                                                                                let _ = self.state_sync.logic_tx.send(LogicMessage::SetPluginVariable(plugin.id, var_name.clone(), Value::Bool(checked)));
                                                                            }
                                                                        }
                                                                        Value::Number(n) => {
                                                                            // Get field type info from ui_schema
                                                                            let field_info = installed.ui_schema.as_ref()
                                                                                .and_then(|schema| schema.fields.iter().find(|f| f.key == *var_name));
                                                                            
                                                                            if let Some(f) = n.as_f64() {
                                                                                let mut val = f;
                                                                                let (speed, min, max) = if let Some(field) = field_info {
                                                                                    if let rtsyn_plugin::ui::FieldType::Float { min, max, step } = &field.field_type {
                                                                                        (*step, *min, *max)
                                                                                    } else {
                                                                                        (1.0, None, None)
                                                                                    }
                                                                                } else {
                                                                                    (1.0, None, None)
                                                                                };
                                                                                let range = match (min, max) {
                                                                                    (Some(mn), Some(mx)) => mn..=mx,
                                                                                    (Some(mn), None) => mn..=f64::INFINITY,
                                                                                    (None, Some(mx)) => f64::NEG_INFINITY..=mx,
                                                                                    (None, None) => f64::NEG_INFINITY..=f64::INFINITY,
                                                                                };
                                                                                if ui.add(egui::DragValue::new(&mut val).speed(speed).clamp_range(range)).changed() {
                                                                                    let _ = self.state_sync.logic_tx.send(LogicMessage::SetPluginVariable(plugin.id, var_name.clone(), Value::from(val)));
                                                                                    if var_name == "refresh_hz" {
                                                                                        recompute_plotter_needed = true;
                                                                                    }
                                                                                }
                                                                            } else if let Some(i) = n.as_i64() {
                                                                                let mut val = i;
                                                                                let (speed, min, max) = if let Some(field) = field_info {
                                                                                    if let rtsyn_plugin::ui::FieldType::Integer { min, max, step } = &field.field_type {
                                                                                        (*step as f64, *min, *max)
                                                                                    } else {
                                                                                        (1.0, None, None)
                                                                                    }
                                                                                } else {
                                                                                    (1.0, None, None)
                                                                                };
                                                                                let range = match (min, max) {
                                                                                    (Some(mn), Some(mx)) => mn..=mx,
                                                                                    (Some(mn), None) => mn..=i64::MAX,
                                                                                    (None, Some(mx)) => i64::MIN..=mx,
                                                                                    (None, None) => i64::MIN..=i64::MAX,
                                                                                };
                                                                                if ui.add(egui::DragValue::new(&mut val).speed(speed).clamp_range(range)).changed() {
                                                                                    let _ = self.state_sync.logic_tx.send(LogicMessage::SetPluginVariable(plugin.id, var_name.clone(), Value::from(val)));
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
                                                    .default_open(true)
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
                                                    .default_open(true)
                                                    .show(ui, |ui| {
                                                        ui.add_space(4.0);
                                                        for output_name in &schema.outputs {
                                                            let value = computed_outputs
                                                                .get(&(plugin.id, output_name.clone()))
                                                                .copied()
                                                                .unwrap_or(0.0);
                                                            let mut value_text = if (value.fract() - 0.0).abs() < f64::EPSILON {
                                                                format!("{value:.0}")
                                                            } else {
                                                                format!("{value:.4}")
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
                                        let supports_start_stop = self.plugin_manager.plugin_behaviors.get(&plugin.kind)
                                            .map(|b| b.supports_start_stop)
                                            .unwrap_or(true);  // Default to true
                                        if supports_start_stop {
                                            let label = if plugin.running { "Stop" } else { "Start" };
                                            if ui.button(label).clicked() {
                                                let is_connection_dependent = matches!(plugin.kind.as_str(), "csv_recorder" | "live_plotter" | "comedi_daq");
                                                if is_connection_dependent
                                                    && !plugin.running
                                                    && !incoming_connections.contains(&plugin.id)
                                                {
                                                    pending_info = Some(
                                                        "Add connections before starting this plugin."
                                                            .to_string(),
                                                    );
                                                    blocked_start = true;
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
                                        let supports_restart = self.plugin_manager.plugin_behaviors.get(&plugin.kind)
                                            .map(|b| b.supports_restart)
                                            .unwrap_or(false);  // Default to false
                                        if supports_restart {
                                            if ui.button("Restart").clicked() {
                                                pending_restart.push(plugin.id);
                                            }
                                        }
                                    });
                                    
                                    if controls_changed {
                                        workspace_changed = true;
                                    }
                        });
                    });
                });

            self.plugin_positions
                .insert(plugin.id, response.response.rect.min);
            self.plugin_rects.insert(plugin.id, response.response.rect);
            if ctx.input(|i| i.pointer.button_double_clicked(egui::PointerButton::Primary)) {
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
            let _ = self
            .state_sync.logic_tx
                .send(LogicMessage::UpdateWorkspace(self.workspace_manager.workspace.clone()));
        }
        for (plugin_id, running) in pending_running {
            // Mark plugin as stopped BEFORE sending message to prevent one more update
            if !running {
                if let Some(plugin) = self.workspace_manager.workspace.plugins.iter_mut().find(|p| p.id == plugin_id) {
                    plugin.running = false;
                }
            }
            
            let _ = self
            .state_sync.logic_tx
                .send(LogicMessage::SetPluginRunning(plugin_id, running));
        }
        if recompute_plotter_needed {
            self.recompute_plotter_ui_hz();
        }
        for plugin_id in pending_restart {
            self.restart_plugin(plugin_id);
        }
        if workspace_changed {
            self.mark_workspace_dirty();
        }

        if let Some(id) = remove_id {
            let name_by_kind: HashMap<String, String> = self
            .plugin_manager.installed_plugins
                .iter()
                .map(|plugin| (plugin.manifest.kind.clone(), plugin.manifest.name.clone()))
                .collect();
            let label = self
            .workspace_manager.workspace
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
                    ui.label(RichText::new(&manifest.name).strong().size(18.0));
                    if let Some(version) = &manifest.version {
                        ui.label(RichText::new(format!("v{version}")).color(egui::Color32::GRAY));
                    }
                });
                if let Some(description) = &manifest.description {
                    let description = Self::normalize_preview_description(description);
                    ui.label(RichText::new(description));
                }

                ui.add_space(6.0);
                ui.label(RichText::new("Ports").strong());
                let inputs = inputs_override.unwrap_or_else(|| {
                    installed_plugins
                        .iter()
                        .find(|p| p.manifest.kind == manifest.kind)
                        .map(|p| p.metadata_inputs.clone())
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
                    .map(|p| p.metadata_outputs.join(", "))
                    .unwrap_or_default();
                egui::Grid::new(("plugin_preview_ports", manifest.kind.as_str()))
                    .num_columns(2)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("Inputs:");
                        ui.label(if inputs_label.is_empty() {
                            "none"
                        } else {
                            &inputs_label
                        });
                        ui.end_row();
                        ui.label("Outputs:");
                        ui.label(if outputs.is_empty() { "none" } else { &outputs });
                        ui.end_row();
                    });

                if let Some(plugin) = installed_plugins.iter().find(|p| p.manifest.kind == manifest.kind) {
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

    fn live_plotter_inputs_override(&self) -> Option<Vec<String>> {
        let plugin = self
            .workspace_manager.workspace
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

    pub(crate) fn render_plugins_window(&mut self, ctx: &egui::Context) {
        if !self.windows.plugins_open {
            return;
        }

        let name_by_kind: HashMap<String, String> = self
            .plugin_manager.installed_plugins
            .iter()
            .map(|plugin| (plugin.manifest.kind.clone(), plugin.manifest.name.clone()))
            .collect();
        let __manifest_by_kind: HashMap<String, PluginManifest> = self
            .plugin_manager.installed_plugins
            .iter()
            .map(|plugin| (plugin.manifest.kind.clone(), plugin.manifest.clone()))
            .collect();
        let metadata_by_kind: HashMap<String, Vec<(String, f64)>> = self
            .plugin_manager.installed_plugins
            .iter()
            .map(|plugin| (plugin.manifest.kind.clone(), plugin.metadata_variables.clone()))
            .collect();

        let mut window_open = self.windows.plugins_open;
        let window_size = egui::vec2(700.0, 420.0);
        let default_pos = Self::center_window(ctx, window_size);
        let response = egui::Window::new("Add plugins")
            .open(&mut window_open)
            .resizable(false)
            .default_pos(default_pos)
            .default_size(window_size)
            .default_pos(default_pos)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(matches!(self.windows.plugin_tab, PluginTab::Add), "Add plugin")
                        .clicked()
                    {
                        self.windows.plugin_tab = PluginTab::Add;
                    }
                    if ui
                        .selectable_label(matches!(self.windows.plugin_tab, PluginTab::Organize), "Organize plugins")
                        .clicked()
                    {
                        self.windows.plugin_tab = PluginTab::Organize;
                    }
                });
                ui.separator();

                match self.windows.plugin_tab {
                    PluginTab::Add => {
                        ui.columns(2, |columns| {
                            columns[0].label("Search");
                            columns[0].text_edit_singleline(&mut self.windows.plugin_search);
                            columns[0].add_space(6.0);
                            let mut selected: Option<usize> = None;
                            egui::ScrollArea::vertical()
                                .id_source("organize_plugin_list")
                                .max_height(220.0)
                                .show(&mut columns[0], |ui| {
                                    for (idx, installed) in self.plugin_manager.installed_plugins.iter().enumerate() {
                                        let label = installed.manifest.name.clone();
                                        if !self.windows.plugin_search.trim().is_empty()
                                            && !label
                                                .to_lowercase()
                                                .contains(&self.windows.plugin_search.to_lowercase())
                                        {
                                            continue;
                                        }
                                        if ui
                                            .selectable_label(
                                                self.windows.plugin_selected_index == Some(idx),
                                                label,
                                            )
                                            .clicked()
                                        {
                                            selected = Some(idx);
                                        }
                                    }
                                });
                            if let Some(idx) = selected {
                                self.windows.plugin_selected_index = Some(idx);
                            }

                            if let Some(idx) = self.windows.plugin_selected_index {
                                if let Some(installed) = self.plugin_manager.installed_plugins.get(idx) {
                                    let inputs_override = self.live_plotter_inputs_override();
                                    Self::render_plugin_preview(
                                        &mut columns[1],
                                        &installed.manifest,
                                        inputs_override,
                                        &installed.manifest.kind,
                                        &serde_json::Value::Object(serde_json::Map::new()),
                                        false,
                                        &self.plugin_manager.installed_plugins,
                                    );
                                    if columns[1].button("Add to workspace").clicked() {
                                        self.add_installed_plugin(idx);
                                    }
                                }
                            } else {
                                columns[1].label("Select a plugin to preview.");
                            }
                        });
                    }
                    PluginTab::Organize => {
                        let mut open_path_dialog: Option<u64> = None;
                        let mut pending_csv_prune: Option<(u64, usize)> = None;
                        let id_to_display: HashMap<u64, String> = self
            .workspace_manager.workspace
                            .plugins
                            .iter()
                            .map(|plugin| {
                                let display_name = name_by_kind
                                    .get(&plugin.kind)
                                    .cloned()
                                    .unwrap_or_else(|| Self::display_kind(&plugin.kind));
                                (plugin.id, display_name)
                            })
                            .collect();
                        let connections_snapshot = self.workspace_manager.workspace.connections.clone();
                        ui.columns(2, |columns| {
                            columns[0].label("Search");
                            columns[0].text_edit_singleline(&mut self.windows.organize_search);
                            columns[0].add_space(6.0);
                            let mut selected: Option<usize> = None;
                            egui::ScrollArea::vertical()
                                .max_height(220.0)
                                .show(&mut columns[0], |ui| {
                                    for (idx, plugin) in self.workspace_manager.workspace.plugins.iter().enumerate() {
                                        let display_name = name_by_kind
                                            .get(&plugin.kind)
                                            .cloned()
                                            .unwrap_or_else(|| Self::display_kind(&plugin.kind));
                                        let label = format!("#{} {}", plugin.id, display_name);
                                        if !self.windows.organize_search.trim().is_empty()
                                            && !label
                                                .to_lowercase()
                                                .contains(&self.windows.organize_search.to_lowercase())
                                        {
                                            continue;
                                        }
                                        if ui
                                            .selectable_label(
                                                self.windows.organize_selected_index == Some(idx),
                                                label,
                                            )
                                            .clicked()
                                        {
                                            selected = Some(idx);
                                        }
                                    }
                                });
                            if let Some(idx) = selected {
                                self.windows.organize_selected_index = Some(idx);
                            }

                            columns[1].label("Edit");
                            if let Some(idx) = self.windows.organize_selected_index {
                                let mut plugin_changed = false;
                                if let Some(plugin) = self.workspace_manager.workspace.plugins.get_mut(idx) {
                                    let display_name = name_by_kind
                                        .get(&plugin.kind)
                                        .cloned()
                                        .unwrap_or_else(|| Self::display_kind(&plugin.kind));
                                    columns[1].label(format!("#{} {}", plugin.id, display_name));
                                    if plugin.kind == "csv_recorder" {
                                    if let Value::Object(ref mut map) = plugin.config {
                                        let mut separator = map
                                            .get("separator")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or(",")
                                            .to_string();
                                            let mut include_time = map
                                                .get("include_time")
                                                .and_then(|v| v.as_bool())
                                                .unwrap_or(true);
                                            let mut path = map
                                                .get("path")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("")
                                                .to_string();
                                            let mut path_autogen = map
                                                .get("path_autogen")
                                                .and_then(|v| v.as_bool())
                                                .unwrap_or(true);
                                            let mut csv_columns: Vec<String> = map
                                                .get("columns")
                                                .and_then(|v| v.as_array())
                                                .map(|arr| {
                                                    arr.iter()
                                                        .map(|value| value.as_str().unwrap_or("").to_string())
                                                        .collect()
                                                })
                                                .unwrap_or_default();
                                            let mut input_count = csv_columns.len();

                                            let col1 = &mut columns[1];
                                            col1.horizontal(|ui| {
                                                ui.label("Separator");
                                                if ui
                                                    .add(
                                                        egui::TextEdit::singleline(&mut separator)
                                                            .desired_width(40.0),
                                                    )
                                                    .changed()
                                                {
                                                    plugin_changed = true;
                                                }
                                            });
                                            let (_unit, _scale, time_label) = Self::time_settings_from_selection(
                                                self.workspace_settings.tab,
                                                self.frequency_unit,
                                                self.period_unit,
                                            );
                                            col1.horizontal(|ui| {
                                                if ui
                                                    .checkbox(&mut include_time, "Include time column")
                                                    .changed()
                                                {
                                                    plugin_changed = true;
                                                }
                                                ui.label(RichText::new(time_label).color(egui::Color32::GRAY));
                                            });

                                            col1.horizontal(|ui| {
                                                ui.label("Path");
                                                if ui
                                                    .add(
                                                        egui::TextEdit::singleline(&mut path)
                                                            .desired_width(220.0),
                                                    )
                                                    .changed()
                                                {
                                                    plugin_changed = true;
                                                    path_autogen = false;
                                                }
                                                if ui.button("Browse...").clicked() {
                                                    open_path_dialog = Some(plugin.id);
                                                    path_autogen = false;
                                                }
                                            });

                                            col1.horizontal(|ui| {
                                                ui.label("Inputs");
                                                if ui.button("Add input").clicked() {
                                                    csv_columns.push(String::new());
                                                    input_count = csv_columns.len();
                                                    plugin_changed = true;
                                                }
                                            });

                                            col1.label("Columns");
                                            egui::ScrollArea::vertical()
                                                .id_source(("csv_columns", plugin.id))
                                                .max_height(120.0)
                                                .show(col1, |ui| {
                                                    let mut idx = 0usize;
                                                    while idx < input_count {
                                                        let label = format!("in_{idx}");
                                                        let mut value =
                                                            csv_columns.get(idx).cloned().unwrap_or_default();
                                                        if value.is_empty()
                                                            && self
            .workspace_manager.workspace
                                                                .connections
                                                                .iter()
                                                                .any(|conn| {
                                                                    conn.to_plugin == plugin.id
                                                                        && conn.to_port == label
                                                                })
                                                        {
                                                            let default_name = {
                                                                let port = format!("in_{idx}");
                                                                if let Some(conn) = connections_snapshot
                                                                    .iter()
                                                                    .find(|conn| {
                                                                        conn.to_plugin == plugin.id
                                                                            && conn.to_port == port
                                                                    })
                                                                {
                                                                    let source_name = id_to_display
                                                                        .get(&conn.from_plugin)
                                                                        .cloned()
                                                                        .unwrap_or_else(|| "plugin".to_string())
                                                                        .replace(' ', "_")
                                                                        .to_lowercase();
                                                                    let port = conn.from_port.to_lowercase();
                                                                    format!(
                                                                        "{source_name}_{}_{}",
                                                                        conn.from_plugin, port
                                                                    )
                                                                } else {
                                                                    let recorder_name = id_to_display
                                                                        .get(&plugin.id)
                                                                        .cloned()
                                                                        .unwrap_or_else(|| "plugin".to_string())
                                                                        .replace(' ', "_")
                                                                        .to_lowercase();
                                                                    format!("{recorder_name}_{}_{}", plugin.id, port.to_lowercase())
                                                                }
                                                            };
                                                            value = default_name.clone();
                                                            if idx < csv_columns.len() {
                                                                csv_columns[idx] = default_name;
                                                            } else {
                                                                csv_columns.push(default_name);
                                                            }
                                                            plugin_changed = true;
                                                        }
                                                        let display = if value.is_empty() {
                                                            "empty".to_string()
                                                        } else {
                                                            value.clone()
                                                        };
                                                        let mut remove_row = false;
                                                        ui.horizontal(|ui| {
                                                            ui.label(label);
                                                            if ui
                                                                .add(
                                                                    egui::TextEdit::singleline(&mut value)
                                                                        .hint_text(display)
                                                                        .desired_width(140.0),
                                                                )
                                                                .changed()
                                                            {
                                                                if idx < csv_columns.len() {
                                                                    csv_columns[idx] = value.clone();
                                                                }
                                                                plugin_changed = true;
                                                            }
                                                            if ui.button("X").clicked() {
                                                                remove_row = true;
                                                            }
                                                        });
                                                        if remove_row {
                                                            if idx < csv_columns.len() {
                                                                csv_columns.remove(idx);
                                                                input_count = csv_columns.len();
                                                                plugin_changed = true;
                                                                continue;
                                                            }
                                                        }
                                                        idx += 1;
                                                    }
                                                });

                                            map.insert("separator".to_string(), Value::String(separator));
                                            map.insert("include_time".to_string(), Value::from(include_time));
                                            map.insert("input_count".to_string(), Value::from(input_count as u64));
                                            map.insert(
                                                "columns".to_string(),
                                                Value::Array(csv_columns.into_iter().map(Value::from).collect()),
                                            );
                                            map.insert("path".to_string(), Value::String(path));
                                            map.insert("path_autogen".to_string(), Value::from(path_autogen));
                                            pending_csv_prune = Some((plugin.id, input_count));
                                        } else {
                                            columns[1].label("Config is not an object.");
                                        }
                                    } else {
                                        match plugin.config {
                                            Value::Object(ref mut map) => {
                                                let col1 = &mut columns[1];
                                                let vars = metadata_by_kind
                                                    .get(&plugin.kind)
                                                    .cloned()
                                                    .unwrap_or_default();
                                                col1.push_id(("organize_config_grid", plugin.id), |ui| {
                                                    egui::Grid::new(("organize_config_grid_inner", plugin.id))
                                                        .num_columns(2)
                                                        .min_col_width(110.0)
                                                        .spacing([10.0, 6.0])
                                                        .show(ui, |ui| {
                                                            for (name, _default_value) in &vars {
                                                                let key = name;
                                                                if let Some(value) = map.get_mut(key) {
                                                                    ui.label(key);
                                                                    let buffer_key =
                                                                        (plugin.id, key.clone());
                                                                    let buffer = self
                                                                        .number_edit_buffers
                                                                        .entry(buffer_key)
                                                                        .or_insert_with(|| {
                                                                            format_f64_6(
                                                                                value.as_f64().unwrap_or(0.0),
                                                                            )
                                                                        });
                                                                    let resp = ui.add(
                                                                        egui::TextEdit::singleline(buffer)
                                                                            .desired_width(80.0),
                                                                    );
                                                                    if resp.changed() {
                                                                        let _ =
                                                                            normalize_numeric_input(buffer);
                                                                        if let Some(parsed) =
                                                                            parse_f64_input(buffer)
                                                                        {
                                                                            let truncated =
                                                                                truncate_f64(parsed);
                                                                            *value = Value::from(truncated);
                                                                            *buffer = format_f64_with_input(
                                                                                buffer,
                                                                                truncated,
                                                                            );
                                                                            plugin_changed = true;
                                                                        }
                                                                    }
                                                                    ui.end_row();
                                                                }
                                                            }
                                                        });
                                                });
                                            }
                                            _ => {
                                                columns[1].label("Config is not an object.");
                                            }
                                        }
                                    }
                                    if columns[1].button("Remove from workspace").clicked() {
                                        let display_name = name_by_kind
                                            .get(&plugin.kind)
                                            .cloned()
                                            .unwrap_or_else(|| Self::display_kind(&plugin.kind));
                                        let label = format!("#{} {}", plugin.id, display_name);
                                        let plugin_id = plugin.id;
                                        self.show_confirm(
                                            "Confirm removal",
                                            &format!("Remove plugin {label} from the workspace?"),
                                            "Remove",
                                            ConfirmAction::RemovePlugin(plugin_id),
                                        );
                                    }
                                }
                                if plugin_changed {
                                    self.mark_workspace_dirty();
                                }
                            } else {
                                columns[1].label("Select a plugin to edit.");
                            }
                        });
                        if let Some(id) = open_path_dialog {
                            self.open_csv_path_dialog(id);
                        }
                        if let Some((id, count)) = pending_csv_prune {
                            prune_extendable_inputs_plugin_connections(
                                &mut self.workspace_manager.workspace.connections,
                                id,
                                count,
                            );
                            self.enforce_connection_dependent();
                        }
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
            if self.pending_window_focus == Some(WindowFocus::Plugins) {
                ctx.move_to_top(response.response.layer_id);
                self.pending_window_focus = None;
            }
        }
        self.windows.plugins_open = window_open;
    }

    pub(crate) fn render_manage_plugins_window(&mut self, ctx: &egui::Context) {
        if !self.windows.manage_plugins_open {
            return;
        }

        let mut window_open = self.windows.manage_plugins_open;
        let window_size = egui::vec2(700.0, 400.0);
        let default_pos = Self::center_window(ctx, window_size);
        let response = egui::Window::new("Manage plugins")
            .open(&mut window_open)
            .resizable(false)
            .default_pos(default_pos)
            .default_size(window_size)
            .fixed_size(window_size)
            .show(ctx, |ui| match self.windows.manage_plugins_tab {
                ManageTab::Install => {
                    let mut rescan = false;
                    let installed_kinds: HashSet<String> = self
            .plugin_manager.installed_plugins
                        .iter()
                        .map(|plugin| plugin.manifest.kind.clone())
                        .collect();
                    ui.columns(2, |columns| {
                        columns[0].horizontal(|ui| {
                            ui.label("Search");
                            ui.text_edit_singleline(&mut self.windows.install_search);
                        });
                        columns[0].add_space(6.0);
                        let mut selected: Option<usize> = None;
                        let list_height = (columns[0].available_height() - 2.0).max(40.0);
                        let width = columns[0].available_width();
                        columns[0].allocate_ui_with_layout(
                            egui::vec2(width, list_height),
                            egui::Layout::top_down(egui::Align::LEFT),
                            |ui| {
                                egui::ScrollArea::vertical()
                                    .max_height(list_height)
                                    .min_scrolled_height(list_height)
                                    .show(ui, |ui| {
                                        for (idx, detected) in
                                            self.plugin_manager.detected_plugins.iter().enumerate()
                                        {
                                            let label = detected.manifest.name.clone();
                                            if !self.windows.install_search.trim().is_empty()
                                                && !label
                                                    .to_lowercase()
                                                    .contains(&self.windows.install_search.to_lowercase())
                                            {
                                                continue;
                                            }
                                            let row = ui.add_sized(
                                                [ui.available_width(), 18.0],
                                                egui::SelectableLabel::new(
                                                    self.windows.manage_selected_index == Some(idx),
                                                    label,
                                                ),
                                            );
                                            if row.clicked() {
                                                selected = Some(idx);
                                            }
                                        }
                                    });
                            },
                        );
                        if let Some(idx) = selected {
                            self.windows.manage_selected_index = Some(idx);
                        }

                        columns[0].with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Rescan default plugins folder");
                                if ui.button("Rescan").clicked() {
                                    rescan = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Browse plugin folder").strong());
                                if ui.button("Browse...").clicked() {
                                    self.open_install_dialog();
                                }
                            });
                        });

                        let mut install_selected: Option<(BuildAction, String)> = None;
                        let mut uninstall_selected: Option<usize> = None;
                        let mut reinstall_selected: Option<(BuildAction, String)> = None;
                        if let Some(idx) = self.windows.manage_selected_index {
                            if let Some(detected) = self.plugin_manager.detected_plugins.get(idx) {
                                let inputs_override = self.live_plotter_inputs_override();
                                Self::render_plugin_preview(
                                    &mut columns[1],
                                    &detected.manifest,
                                    inputs_override,
                                    &detected.manifest.kind,
                                    &serde_json::Value::Object(serde_json::Map::new()),
                                    false,
                                        &self.plugin_manager.installed_plugins,
                                );
                                let is_installed =
                                    installed_kinds.contains(&detected.manifest.kind);
                                if !is_installed {
                                    columns[1].horizontal(|ui| {
                                        let install_button = egui::Button::new("Install");
                                        if ui
                                            .add_enabled(
                                                self.build_dialog.rx.is_none(),
                                                install_button,
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
                                } else if let Some(installed_idx) = self
            .plugin_manager.installed_plugins
                                    .iter()
                                    .position(|p| p.manifest.kind == detected.manifest.kind)
                                {
                                    let removable = self
            .plugin_manager.installed_plugins
                                        .get(installed_idx)
                                        .map(|p| p.removable)
                                        .unwrap_or(false);
                                    columns[1].horizontal(|ui| {
                                        if ui
                                            .add_enabled(
                                                removable && self.build_dialog.rx.is_none(),
                                                egui::Button::new("Reinstall"),
                                            )
                                            .clicked()
                                        {
                                            if let Some(installed) =
                                                self.plugin_manager.installed_plugins.get(installed_idx)
                                            {
                                                reinstall_selected = Some((
                                                    BuildAction::Reinstall {
                                                        kind: installed.manifest.kind.clone(),
                                                        path: installed.path.clone(),
                                                    },
                                                    installed.manifest.name.clone(),
                                                ));
                                            }
                                        }
                                        if ui
                                            .add_enabled(removable, egui::Button::new("Uninstall"))
                                            .clicked()
                                        {
                                            uninstall_selected = Some(installed_idx);
                                        }
                                    });
                                }
                            }
                        } else {
                            columns[1].label("Select a plugin to preview.");
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
                    });

                    if rescan {
                        self.load_installed_plugins();
                        self.scan_detected_plugins();
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
            if self.pending_window_focus == Some(WindowFocus::ManagePlugins) {
                ctx.move_to_top(response.response.layer_id);
                self.pending_window_focus = None;
            }
        }

        self.windows.manage_plugins_open = window_open;
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

    pub(crate) fn render_plugin_config_window(&mut self, ctx: &egui::Context) {
        if !self.windows.plugin_config_open {
            return;
        }

        let mut open = self.windows.plugin_config_open;
        let plugin_id = match self.windows.plugin_config_id {
            Some(id) => id,
            None => {
                self.windows.plugin_config_open = false;
                return;
            }
        };

        let name_by_kind: HashMap<String, String> = self
            .plugin_manager.installed_plugins
            .iter()
            .map(|plugin| (plugin.manifest.kind.clone(), plugin.manifest.name.clone()))
            .collect();

        let window_size =
            if let Some(plugin) = self.workspace_manager.workspace.plugins.iter().find(|p| p.id == plugin_id) {
                if plugin.kind == "csv_recorder" {
                    egui::vec2(520.0, 360.0)
                } else if plugin.kind == "live_plotter" {
                    egui::vec2(420.0, 240.0)
                } else {
                    egui::vec2(320.0, 180.0)
                }
            } else {
                egui::vec2(320.0, 180.0)
            };
        let default_pos = Self::center_window(ctx, window_size);
        let response = egui::Window::new("Plugin config")
            .open(&mut open)
            .resizable(false)
            .default_pos(default_pos)
            .default_size(window_size)
            .fixed_size(window_size)
            .show(ctx, |ui| {
                let plugin_index = self
            .workspace_manager.workspace
                    .plugins
                    .iter()
                    .position(|p| p.id == plugin_id);
                if let Some(plugin_index) = plugin_index {
                    let plugin_kind = self.workspace_manager.workspace.plugins[plugin_index].kind.clone();
                    let display_name = name_by_kind
                        .get(&plugin_kind)
                        .cloned()
                        .unwrap_or_else(|| Self::display_kind(&plugin_kind));
                    let mut priority = self.workspace_manager.workspace.plugins[plugin_index].priority;
                    let config = self.workspace_manager.workspace.plugins[plugin_index].config.clone();
                    let mut config_changed = false;
                    let pending_start: Option<bool> = None;

                    ui.horizontal(|ui| {
                        let (id_rect, _) = ui.allocate_exact_size(
                            egui::vec2(20.0, 20.0),
                            egui::Sense::hover(),
                        );
                        ui.painter().circle_filled(
                            id_rect.center(),
                            9.0,
                            egui::Color32::from_gray(60),
                        );
                        ui.painter().text(
                            id_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            plugin_id.to_string(),
                            egui::FontId::proportional(13.0),
                            ui.visuals().text_color(),
                        );
                        ui.label(RichText::new(display_name).strong().size(16.0));
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label("Priority");
                        if ui
                            .add(egui::DragValue::new(&mut priority).speed(1))
                            .changed()
                        {
                            config_changed = true;
                        }
                    });
                    if priority < 0 {
                        priority = 0;
                        config_changed = true;
                    } else if priority > 99 {
                        priority = 99;
                        config_changed = true;
                    }

                    if config_changed {
                        self.workspace_manager.workspace.plugins[plugin_index].priority = priority;
                        self.workspace_manager.workspace.plugins[plugin_index].config = config;
                        self.mark_workspace_dirty();
                    }
                    if let Some(running) = pending_start {
                        let _ = self
            .state_sync.logic_tx
                            .send(LogicMessage::SetPluginRunning(plugin_id, running));
                        self.mark_workspace_dirty();
                    }
                } else {
                    ui.label("Plugin not found.");
                }
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

        if !open {
            self.windows.plugin_config_open = false;
            self.windows.plugin_config_id = None;
        }
    }
}
