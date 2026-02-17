//! Plugin management and UI rendering functionality for RTSyn GUI.
//!
//! This module provides comprehensive plugin management capabilities including:
//! - Plugin card rendering and interaction
//! - Plugin installation, uninstallation, and management windows
//! - Plugin creation wizard with field configuration
//! - Plugin configuration dialogs
//! - Context menus and plugin operations
//!
//! The module handles both built-in app plugins (csv_recorder, live_plotter, etc.)
//! and user-installed plugins with dynamic UI schema support.

use super::*;
use crate::utils::truncate_string;
use crate::WindowFocus;
use crate::{
    BuildAction,
    PluginFieldDraft,
};
use rtsyn_cli::plugin_creator::PluginKindType;

impl GuiApp {
/// Truncates plugin names for display in list views with ellipsis.
    ///
    /// This function ensures plugin names fit within constrained list layouts
    /// by truncating them to a specified character limit and adding ellipsis
    /// when truncation occurs.
    ///
    /// # Parameters
    /// - `name`: The plugin name to potentially truncate
    /// - `max_chars`: Maximum number of characters before truncation
    ///
    /// Renders the plugin addition window for adding installed plugins to workspace.
    ///
    /// This function creates a modal window that allows users to browse through
    /// installed plugins and add them to the current workspace. It provides a
    /// two-panel interface with a searchable plugin list and detailed preview.
    ///
    /// # Window Layout
    /// - Left panel: Searchable list of installed plugins
    /// - Right panel: Plugin preview with add button
    /// - Fixed window size for consistent user experience
    /// - Proper focus management and window layering
    ///
    /// # Features
    /// - Real-time search filtering of plugin names
    /// - Plugin selection with visual feedback
    /// - Detailed plugin preview including ports and variables
    /// - One-click addition to workspace
    /// - Special handling for live plotter input overrides
    ///
    /// # Interaction
    /// - Click to select plugins from the list
    /// - Search box filters plugins by name (case-insensitive)
    /// - "Add to runtime" button adds selected plugin to workspace
    /// - Window can be closed via title bar or ESC key
    ///
    /// # Side Effects
    /// - Updates plugin selection state
    /// - Adds plugins to workspace when requested
    /// - Manages window focus and layering
    /// - Updates window rectangle tracking for interaction
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
                                                    truncate_string(&label, 44);
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

    /// Renders a configurable section for plugin field definitions in the creation wizard.
    ///
    /// This function creates an interactive section where users can define plugin fields
    /// (variables, inputs, outputs, etc.) with name, type, and optionally default values.
    /// It provides add/remove functionality and type selection for each field.
    ///
    /// # Parameters
    /// - `ui`: The egui UI context for rendering
    /// - `id_key`: Unique identifier for UI element disambiguation
    /// - `title`: Section title displayed to the user
    /// - `add_label`: Text for the "add new field" button
    /// - `fields`: Mutable reference to the field collection
    /// - `show_default_value`: Whether to show default value input fields
    ///
    /// # Returns
    /// `true` if any changes were made to the field collection, `false` otherwise
    ///
    /// # Field Configuration
    /// Each field can be configured with:
    /// - Name: User-defined identifier for the field
    /// - Type: Selected from predefined types (f64, f32, i64, i32, bool, string)
    /// - Default value: Optional default value (when `show_default_value` is true)
    ///
    /// # Interaction Features
    /// - Add button to create new fields
    /// - Remove button for each field
    /// - Type dropdown with automatic default value updates
    /// - Input validation and type-appropriate defaults
    ///
    /// # UI Styling
    /// Uses custom styling for input fields with darker background colors
    /// to distinguish from the main UI theme.
    pub(super) fn render_new_plugin_fields_section(
        ui: &mut egui::Ui,
        id_key: &str,
        title: &str,
        add_label: &str,
        fields: &mut Vec<PluginFieldDraft>,
        show_default_value: bool,
    ) -> bool {
        let mut changed = false;
        let section_width = ui.available_width();
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                ui.set_min_width(section_width);
                ui.set_max_width(section_width);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(title).strong());
                        ui.add_space(8.0);
                        if ui.button(add_label).clicked() {
                            fields.push(PluginFieldDraft::default());
                            changed = true;
                        }
                    });
                    ui.add_space(6.0);
                    let mut remove_idx = None;
                    let mut idx = 0usize;
                    while idx < fields.len() {
                        let current_idx = idx;
                        ui.scope(|ui| {
                            ui.set_min_width(section_width);
                            ui.set_max_width(section_width);
                            let mut style = ui.style().as_ref().clone();
                            style.visuals.extreme_bg_color = egui::Color32::from_rgb(58, 58, 62);
                            style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(58, 58, 62);
                            style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(66, 66, 72);
                            style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(72, 72, 78);
                            ui.set_style(style);
                            let field_id = fields[current_idx].id;
                            ui.push_id(field_id, |ui| {
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_sized(
                                            [200.0, 26.0],
                                            egui::TextEdit::singleline(
                                                &mut fields[current_idx].name,
                                            )
                                            .hint_text("Name"),
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                    let previous_type = fields[current_idx].type_name.clone();
                                    egui::ComboBox::from_id_source((id_key, current_idx, "type"))
                                        .width(96.0)
                                        .selected_text(fields[current_idx].type_name.clone())
                                        .show_ui(ui, |ui| {
                                            for ty in Self::NEW_PLUGIN_TYPES {
                                                if ui
                                                    .selectable_label(
                                                        fields[current_idx].type_name == ty,
                                                        ty,
                                                    )
                                                    .clicked()
                                                {
                                                    fields[current_idx].type_name = ty.to_string();
                                                    changed = true;
                                                }
                                            }
                                        });
                                    if previous_type != fields[current_idx].type_name {
                                        let prev_default =
                                            Self::plugin_creator_default_by_type(&previous_type);
                                        if fields[current_idx].default_value.trim().is_empty()
                                            || fields[current_idx].default_value == prev_default
                                        {
                                            fields[current_idx].default_value =
                                                Self::plugin_creator_default_by_type(
                                                    &fields[current_idx].type_name,
                                                )
                                                .to_string();
                                        }
                                    }
                                    if show_default_value {
                                        let default_hint =
                                            Self::plugin_creator_default_by_type(
                                                &fields[current_idx].type_name,
                                            )
                                            .to_string();
                                        if ui
                                            .add_sized(
                                                [120.0, 26.0],
                                                egui::TextEdit::singleline(
                                                    &mut fields[current_idx].default_value,
                                                )
                                                .hint_text(default_hint),
                                            )
                                            .changed()
                                        {
                                            changed = true;
                                        }
                                    }
                                    if ui.small_button("Remove").clicked() {
                                        remove_idx = Some(current_idx);
                                    }
                                });
                            });
                        });
                        if idx + 1 < fields.len() {
                            ui.add_space(4.0);
                        }
                        idx += 1;
                    }
                    if let Some(idx) = remove_idx {
                        if idx < fields.len() {
                            fields.remove(idx);
                            changed = true;
                        }
                    }
                });
            });
        changed
    }

    fn render_required_ports_section(
        ui: &mut egui::Ui,
        id: &str,
        all_required: &mut bool,
        entries: &mut Vec<String>,
        selection: &mut String,
        available: &[String],
        add_button_label: &str,
    ) -> bool {
        let mut changed = false;
        let section_width = ui.available_width();
        let mut seen = std::collections::HashSet::new();
        let mut candidates = Vec::new();
        for name in available {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                continue;
            }
            if entries.iter().any(|entry| entry == trimmed) {
                continue;
            }
            if seen.insert(trimmed.to_string()) {
                candidates.push(trimmed.to_string());
            }
        }

        if *all_required {
            if !selection.is_empty() {
                selection.clear();
                changed = true;
            }
        } else if !candidates.is_empty()
            && (selection.is_empty()
                || !candidates
                    .iter()
                    .any(|name| name == selection.trim()))
        {
            selection.clear();
            selection.push_str(&candidates[0]);
            changed = true;
        }

        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(8.0))
            .show(ui, |ui| {
                ui.set_min_width(section_width);
                ui.set_max_width(section_width);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if ui.checkbox(all_required, "All required").changed() {
                            changed = true;
                        }
                        ui.add_space(6.0);
                        if !*all_required && !candidates.is_empty() {
                            egui::ComboBox::from_id_source(format!("{id}_dropdown"))
                                .selected_text(if selection.is_empty() {
                                    "Select port"
                                } else {
                                    selection.as_str()
                                })
                                .width(140.0)
                                .show_ui(ui, |ui| {
                                    for option in &candidates {
                                        if ui
                                            .selectable_label(selection.as_str() == option, option)
                                            .clicked()
                                        {
                                            *selection = option.clone();
                                            changed = true;
                                        }
                                    }
                                });
                        }
                        ui.add_space(6.0);
                        let selection_text = selection.trim();
                        let can_add = !*all_required
                            && !selection_text.is_empty()
                            && candidates.iter().any(|name| name == selection_text);
                        if ui
                            .add_enabled(can_add, egui::Button::new(add_button_label))
                            .clicked()
                        {
                            entries.push(selection_text.to_string());
                            selection.clear();
                            changed = true;
                        }
                    });
                    if !entries.is_empty() {
                        ui.add_space(6.0);
                        let mut remove_idx = None;
                        ui.vertical(|ui| {
                            for (idx, entry) in entries.iter().enumerate() {
                                ui.push_id((id, idx), |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(entry);
                                        if ui.small_button("×").clicked() {
                                            remove_idx = Some(idx);
                                        }
                                    });
                                });
                                if idx + 1 < entries.len() {
                                    ui.add_space(4.0);
                                }
                            }
                        });
                        if let Some(idx) = remove_idx {
                            if idx < entries.len() {
                                entries.remove(idx);
                                changed = true;
                            }
                        }
                    }
                });
            });

        changed
    }

    /// Renders the new plugin creation wizard window.
    ///
    /// This function creates a comprehensive plugin creation interface using an external
    /// viewport. The wizard guides users through all aspects of plugin creation including
    /// naming, language selection, field definitions, and behavioral configuration.
    ///
    /// # Window Structure
    /// The wizard is organized into 5 main sections:
    /// 1. Name and Language - Basic plugin identification and target language
    /// 2. Main Characteristics - Description and purpose of the plugin
    /// 3. Field Definitions - Variables, inputs, outputs, and internal variables
    /// 4. Options - Behavioral flags and requirements
    /// 5. Creation - Final creation with folder selection
    ///
    /// # Field Types Supported
    /// - Variables: Runtime configurable parameters with default values
    /// - Inputs: Data input ports for receiving values
    /// - Outputs: Data output ports for sending computed values
    /// - Internal Variables: Plugin-internal state variables
    ///
    /// # Configuration Options
    /// - Autostart: Plugin loads in started state
    /// - Start/Stop controls: Enable runtime control buttons
    /// - Restart support: Enable reset/restart functionality
    /// - Apply support: Enable modify/apply operations
    /// - External window: Plugin opens in separate window
    /// - Starts expanded: UI sections start in expanded state
    /// - Required ports: Ports that must be connected before starting
    ///
    /// # Side Effects
    /// - Updates plugin draft configuration
    /// - Triggers folder selection dialogs
    /// - Marks workspace as dirty when changes are made
    /// - Creates plugin files when creation is completed
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
                ui.heading(
                    RichText::new("New Plugin")
                        .size(28.0)
                        .color(egui::Color32::WHITE)
                        .strong(),
                );
                ui.label(
                    "Create a Rust/C/C++ scaffold with structured inputs, outputs and runtime variables.",
                );
                ui.add_space(10.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let section_width = ui.available_width();
                    egui::Frame::group(ui.style())
                        .inner_margin(egui::Margin::same(12.0))
                        .show(ui, |ui| {
                            ui.set_min_width(section_width);
                            ui.label(RichText::new("1. Name, language, and type").strong());
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
                            ui.horizontal(|ui| {
                                ui.label("Language");
                                ui.add_space(6.0);
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
                                ui.add_space(16.0);
                                ui.label("Type");
                                ui.add_space(6.0);
                                egui::ComboBox::from_id_source("new_plugin_type")
                                    .selected_text(self.new_plugin_draft.plugin_type.variant())
                                    .show_ui(ui, |ui| {
                                        for ty in [
                                            PluginKindType::Standard,
                                            PluginKindType::Device,
                                            PluginKindType::Computational,
                                        ] {
                                            if ui
                                                .selectable_label(
                                                    self.new_plugin_draft.plugin_type == ty,
                                                    ty.variant(),
                                                )
                                                .clicked()
                                            {
                                                self.new_plugin_draft.plugin_type = ty;
                                                changed = true;
                                            }
                                        }
                                    });
                                ui.add_space(12.0);
                                let description = match self.new_plugin_draft.plugin_type {
                                    PluginKindType::Standard => "Standard RTSyn plugin",
                                    PluginKindType::Computational => {
                                        "Plugin with computational capabilities, uses numeric integration"
                                    }
                                    PluginKindType::Device => {
                                        "Device plugin for communication with hardware"
                                    }
                                };
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(description)
                                            .color(egui::Color32::from_rgb(150, 150, 150))
                                            .size(15.0),
                                    )
                                    .wrap(true),
                                );
                            });
                        });

                    ui.add_space(10.0);
                    let section_width = ui.available_width();
                    egui::Frame::group(ui.style())
                        .inner_margin(egui::Margin::same(12.0))
                        .show(ui, |ui| {
                            ui.set_min_width(section_width);
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
                    let section_width = ui.available_width();
                    egui::Frame::group(ui.style())
                        .inner_margin(egui::Margin::same(12.0))
                        .show(ui, |ui| {
                            ui.set_min_width(section_width);
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
                    let section_width = ui.available_width();
                    egui::Frame::group(ui.style())
                        .inner_margin(egui::Margin::same(12.0))
                        .show(ui, |ui| {
                            ui.set_min_width(section_width);
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
                            ui.label("Required connected input ports to start");
                            changed |= Self::render_required_ports_section(
                                ui,
                                "new_plugin_required_inputs",
                                &mut self.new_plugin_draft.required_inputs_all,
                                &mut self.new_plugin_draft.required_inputs,
                                &mut self.new_plugin_draft.required_input_selection,
                                &Self::plugin_creator_field_names(&self.new_plugin_draft.inputs),
                                "Add Input",
                            );
                            ui.label("Required connected output ports to start");
                            changed |= Self::render_required_ports_section(
                                ui,
                                "new_plugin_required_outputs",
                                &mut self.new_plugin_draft.required_outputs_all,
                                &mut self.new_plugin_draft.required_outputs,
                                &mut self.new_plugin_draft.required_output_selection,
                                &Self::plugin_creator_field_names(&self.new_plugin_draft.outputs),
                                "Add Output",
                            );
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

    /// Checks if a path belongs to the app_plugins directory.
    ///
    /// This function determines whether a given path is part of the built-in
    /// app_plugins directory structure. This is used to distinguish between
    /// built-in plugins and user-installed plugins for management purposes.
    ///
    /// # Parameters
    /// - `path`: The filesystem path to check
    ///
    /// # Returns
    /// `true` if the path contains an "app_plugins" component, `false` otherwise
    ///
    /// # Usage
    /// This is primarily used in plugin management windows to:
    /// - Filter out built-in plugins from installation lists
    /// - Prevent uninstallation of core app plugins
    /// - Apply different management rules to built-in vs user plugins
    pub(super) fn is_app_plugins_path(path: &std::path::Path) -> bool {
        path.components().any(|c| c.as_os_str() == "app_plugins")
    }

    /// Renders the comprehensive plugin management window.
    ///
    /// This function creates the main plugin management interface where users can
    /// install, reinstall, and uninstall plugins. It provides a two-panel layout
    /// with plugin browsing, preview, and management actions.
    ///
    /// # Window Layout
    /// - Left panel: Searchable list of detected plugins with management controls
    /// - Right panel: Plugin preview with install/reinstall/uninstall buttons
    /// - Footer: Browse and rescan functionality
    /// - Fixed window size for consistent experience
    ///
    /// # Management Actions
    /// - Install: Add new plugins to the system
    /// - Reinstall: Rebuild and reinstall existing plugins
    /// - Uninstall: Remove plugins from the system (with confirmation)
    /// - Browse: Open file dialog to scan additional plugin directories
    /// - Rescan: Refresh the list of available plugins
    ///
    /// # Plugin States
    /// - Not installed: Shows install button
    /// - Installed (removable): Shows reinstall and uninstall buttons
    /// - Installed (non-removable): Limited actions available
    ///
    /// # Features
    /// - Real-time search filtering
    /// - Plugin installation status tracking
    /// - Build process integration with progress feedback
    /// - Confirmation dialogs for destructive actions
    /// - Automatic plugin detection refresh
    ///
    /// # Side Effects
    /// - Triggers plugin builds and installations
    /// - Updates plugin detection state
    /// - Shows confirmation dialogs for uninstallation
    /// - Manages build dialog state and progress
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

    /// Renders the plugin installation window for adding new plugins.
    ///
    /// This function creates a focused interface for plugin installation, allowing
    /// users to browse available plugins and install them. It filters out built-in
    /// app plugins and focuses on user-installable plugins.
    ///
    /// # Window Layout
    /// - Left panel: Searchable list of installable plugins with browse/rescan controls
    /// - Right panel: Plugin preview with install/reinstall actions
    /// - Excludes built-in app_plugins from the installation list
    /// - Fixed window size for consistent user experience
    ///
    /// # Installation Features
    /// - Install new plugins that aren't currently installed
    /// - Reinstall existing plugins (if removable)
    /// - Browse additional directories for plugins
    /// - Rescan default plugin directories
    /// - Real-time search filtering
    ///
    /// # Plugin Filtering
    /// - Excludes plugins from app_plugins directory (built-ins)
    /// - Shows installation status for each plugin
    /// - Filters by search terms in plugin names
    ///
    /// # Actions Available
    /// - Install: For plugins not currently installed
    /// - Reinstall: For installed removable plugins
    /// - Browse: Open file dialog for additional plugin directories
    /// - Rescan: Refresh available plugin list
    ///
    /// # Side Effects
    /// - Initiates plugin build and installation processes
    /// - Updates plugin detection and installation state
    /// - Manages build dialog progress and feedback
    /// - Triggers file dialogs for directory browsing
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

    /// Renders the plugin uninstallation window for removing installed plugins.
    ///
    /// This function creates a focused interface for plugin removal, showing only
    /// plugins that can be safely uninstalled. It excludes built-in app plugins
    /// and non-removable plugins to prevent system instability.
    ///
    /// # Window Layout
    /// - Left panel: Searchable list of removable installed plugins
    /// - Right panel: Plugin preview with uninstall action
    /// - Simplified interface focused on removal operations
    /// - Fixed window size for consistent experience
    ///
    /// # Plugin Filtering
    /// Only shows plugins that meet all criteria:
    /// - Currently installed in the system
    /// - Marked as removable (not system-critical)
    /// - Not located in app_plugins directory (not built-in)
    /// - Match current search filter
    ///
    /// # Safety Features
    /// - Excludes built-in plugins to prevent system damage
    /// - Only shows removable plugins to avoid dependency issues
    /// - Requires confirmation before actual uninstallation
    /// - Provides plugin preview to confirm selection
    ///
    /// # User Interaction
    /// - Search filtering by plugin name
    /// - Click to select plugins for preview
    /// - Uninstall button triggers confirmation dialog
    /// - Preview shows plugin details before removal
    ///
    /// # Side Effects
    /// - Shows confirmation dialogs for uninstallation
    /// - Updates plugin selection state
    /// - Manages window focus and layering
    /// - Triggers actual plugin removal after confirmation
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

    /// Renders the right-click context menu for plugin cards.
    ///
    /// This function displays a context menu when users right-click on plugin cards,
    /// providing quick access to common plugin operations. The menu appears at the
    /// click position and handles proper focus and dismissal behavior.
    ///
    /// # Menu Options
    /// - Add connections: Opens connection editor in add mode
    /// - Remove connections: Opens connection editor in remove mode  
    /// - Plugin config: Opens plugin configuration window
    /// - Duplicate plugin: Creates a copy of the plugin
    ///
    /// # Interaction Behavior
    /// - Appears at the right-click position
    /// - Dismisses when clicking outside the menu
    /// - Dismisses when selecting a menu option
    /// - Prevents multiple menus from being open simultaneously
    /// - Uses foreground layer for proper z-ordering
    ///
    /// # Menu State Management
    /// - Tracks the plugin ID, position, and frame when opened
    /// - Prevents immediate dismissal on the opening frame
    /// - Handles both primary and secondary click dismissal
    /// - Clears menu state when closed
    ///
    /// # Side Effects
    /// - Opens connection editor with appropriate mode
    /// - Opens plugin configuration window
    /// - Triggers plugin duplication
    /// - Updates window focus state
    /// - Manages menu visibility state
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
}
