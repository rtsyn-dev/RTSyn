use crate::GuiApp;
use egui::{RichText, Ui};
use rtsyn_plugin::ui::{FieldType, FileMode, UISchema};
use serde_json::Value;

impl GuiApp {
    /// Render a generic config window for any plugin using its UI schema
    #[allow(dead_code)]
    pub(crate) fn render_generic_plugin_config(
        &mut self,
        ui: &mut Ui,
        plugin_id: u64,
        schema: &UISchema,
    ) {
        let Some(plugin_index) = self.workspace.plugins.iter().position(|p| p.id == plugin_id)
        else {
            ui.label("Plugin not found");
            return;
        };

        let mut config = self.workspace.plugins[plugin_index].config.clone();
        let mut config_changed = false;

        for field in &schema.fields {
            match &field.field_type {
                FieldType::Text { multiline, max_length } => {
                    let mut value = config
                        .get(&field.key)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    ui.horizontal(|ui| {
                        ui.label(&field.label);
                        let response = if *multiline {
                            ui.text_edit_multiline(&mut value)
                        } else {
                            ui.text_edit_singleline(&mut value)
                        };

                        if response.changed() {
                            if let Some(max) = max_length {
                                value.truncate(*max);
                            }
                            config.as_object_mut().unwrap().insert(
                                field.key.clone(),
                                Value::String(value),
                            );
                            config_changed = true;
                        }
                    });

                    if let Some(hint) = &field.hint {
                        ui.label(RichText::new(hint).color(egui::Color32::GRAY).small());
                    }
                }

                FieldType::Integer { min, max, step } => {
                    let mut value = config
                        .get(&field.key)
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);

                    ui.horizontal(|ui| {
                        ui.label(&field.label);
                        if ui
                            .add(egui::DragValue::new(&mut value).speed(*step as f64))
                            .changed()
                        {
                            if let Some(min_val) = min {
                                value = value.max(*min_val);
                            }
                            if let Some(max_val) = max {
                                value = value.min(*max_val);
                            }
                            config
                                .as_object_mut()
                                .unwrap()
                                .insert(field.key.clone(), Value::from(value));
                            config_changed = true;
                        }
                    });

                    if let Some(hint) = &field.hint {
                        ui.label(RichText::new(hint).color(egui::Color32::GRAY).small());
                    }
                }

                FieldType::Float { min, max, step } => {
                    let mut value = config
                        .get(&field.key)
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);

                    ui.horizontal(|ui| {
                        ui.label(&field.label);
                        if ui
                            .add(egui::DragValue::new(&mut value).speed(*step))
                            .changed()
                        {
                            if let Some(min_val) = min {
                                value = value.max(*min_val);
                            }
                            if let Some(max_val) = max {
                                value = value.min(*max_val);
                            }
                            config
                                .as_object_mut()
                                .unwrap()
                                .insert(field.key.clone(), Value::from(value));
                            config_changed = true;
                        }
                    });

                    if let Some(hint) = &field.hint {
                        ui.label(RichText::new(hint).color(egui::Color32::GRAY).small());
                    }
                }

                FieldType::Boolean => {
                    let mut value = config
                        .get(&field.key)
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    if ui.checkbox(&mut value, &field.label).changed() {
                        config
                            .as_object_mut()
                            .unwrap()
                            .insert(field.key.clone(), Value::Bool(value));
                        config_changed = true;
                    }

                    if let Some(hint) = &field.hint {
                        ui.label(RichText::new(hint).color(egui::Color32::GRAY).small());
                    }
                }

                FieldType::FilePath { mode, .. } => {
                    let mut value = config
                        .get(&field.key)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    ui.horizontal(|ui| {
                        ui.label(&field.label);
                        if ui
                            .add(egui::TextEdit::singleline(&mut value).desired_width(200.0))
                            .changed()
                        {
                            config
                                .as_object_mut()
                                .unwrap()
                                .insert(field.key.clone(), Value::String(value.clone()));
                            config_changed = true;
                        }
                        if ui.button("Browse...").clicked() {
                            // TODO: Integrate with existing file dialog system
                            self.open_file_dialog_for_field(plugin_id, field.key.clone(), *mode);
                        }
                    });

                    if let Some(hint) = &field.hint {
                        ui.label(RichText::new(hint).color(egui::Color32::GRAY).small());
                    }
                }

                FieldType::DynamicList { item_type, add_label } => {
                    ui.horizontal(|ui| {
                        ui.label(&field.label);
                        if ui.button(add_label).clicked() {
                            let list = config
                                .get_mut(&field.key)
                                .and_then(|v| v.as_array_mut());
                            if let Some(list) = list {
                                list.push(Value::String(String::new()));
                                config_changed = true;
                            } else {
                                config.as_object_mut().unwrap().insert(
                                    field.key.clone(),
                                    Value::Array(vec![Value::String(String::new())]),
                                );
                                config_changed = true;
                            }
                        }
                    });

                    let list = config
                        .get(&field.key)
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();

                    let mut remove_idx = None;
                    for (idx, item) in list.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let label = format!("  [{}]", idx);
                            ui.label(label);

                            match item_type.as_ref() {
                                FieldType::Text { .. } => {
                                    let mut value = item.as_str().unwrap_or("").to_string();
                                    if ui
                                        .text_edit_singleline(&mut value)
                                        .changed()
                                    {
                                        if let Some(list) = config
                                            .get_mut(&field.key)
                                            .and_then(|v| v.as_array_mut())
                                        {
                                            list[idx] = Value::String(value);
                                            config_changed = true;
                                        }
                                    }
                                }
                                _ => {
                                    ui.label("Unsupported item type");
                                }
                            }

                            if ui.button("X").clicked() {
                                remove_idx = Some(idx);
                            }
                        });
                    }

                    if let Some(idx) = remove_idx {
                        if let Some(list) = config
                            .get_mut(&field.key)
                            .and_then(|v| v.as_array_mut())
                        {
                            list.remove(idx);
                            config_changed = true;
                        }
                    }
                }

                FieldType::Choice { options } => {
                    let current = config
                        .get(&field.key)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    ui.horizontal(|ui| {
                        ui.label(&field.label);
                        let mut selected = current.clone();
                        egui::ComboBox::from_id_source(&field.key)
                            .selected_text(&selected)
                            .show_ui(ui, |ui| {
                                for option in options {
                                    if ui.selectable_label(&selected == option, option).clicked() {
                                        selected = option.clone();
                                    }
                                }
                            });
                        
                        if selected != current {
                            config.as_object_mut().unwrap().insert(
                                field.key.clone(),
                                Value::String(selected),
                            );
                            config_changed = true;
                        }
                    });
                }
            }
        }

        if config_changed {
            self.workspace.plugins[plugin_index].config = config;
            self.mark_workspace_dirty();
        }
    }

    #[allow(dead_code)]
    fn open_file_dialog_for_field(&mut self, _plugin_id: u64, _key: String, _mode: FileMode) {
        // Placeholder for file dialog integration
        // Will be connected to existing file dialog system in full implementation
    }
}
