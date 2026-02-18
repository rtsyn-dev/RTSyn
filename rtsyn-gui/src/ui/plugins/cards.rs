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
use crate::utils::{format_f64_with_input, normalize_numeric_input, parse_f64_input};
use crate::HighlightMode;
use crate::{has_rt_capabilities, spawn_file_dialog_thread, zenity_file_dialog, LivePlotter};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

pub(super) struct PluginRenderContext {
    pub(super) highlighted_plugins: std::collections::HashSet<u64>,
    pub(super) has_connection_highlight: bool,
    pub(super) current_id: Option<u64>,
    pub(super) selected_id: Option<u64>,
    pub(super) tab_primary: egui::Color32,
    pub(super) tab_secondary: egui::Color32,
    pub(super) name_by_kind: std::collections::HashMap<String, String>,
    pub(super) panel_rect: egui::Rect,
    pub(super) circle_radius: f32,
    pub(super) spacing: f32,
}

impl GuiApp {
    /// Renders interactive plugin cards in the main workspace panel.
    ///
    /// This is the core function for rendering plugin instances as interactive cards
    /// in the workspace. Each card displays plugin information, controls, and real-time
    /// data, allowing users to configure, start/stop, and monitor plugins.
    ///
    /// # Parameters
    /// - `ctx`: The egui context for rendering and input handling
    /// - `panel_rect`: The rectangular area available for rendering plugin cards
    ///
    /// # Card Layout
    /// Each plugin card contains:
    /// - Header with plugin ID badge, name, and close button
    /// - Collapsible sections for variables, inputs, outputs, and internal variables
    /// - Control buttons (Start/Stop, Restart, Modify) based on plugin capabilities
    /// - Real-time value displays and configuration controls
    ///
    /// # Interaction Features
    /// - Drag and drop positioning with boundary constraints
    /// - Double-click to select/deselect plugins
    /// - Right-click context menus for advanced operations
    /// - Connection highlighting for visual feedback
    /// - Real-time data updates and control synchronization
    ///
    /// # Plugin Types Handled
    /// - Standard plugins with UI schemas
    /// - App plugins (csv_recorder, live_plotter, etc.) with special handling
    /// - External window plugins (excluded from card rendering)
    /// - Value viewer plugins with special display logic
    ///
    /// # Side Effects
    /// - Updates plugin positions and rectangles for interaction tracking
    /// - Modifies plugin configurations and states
    /// - Triggers workspace updates and plugin operations
    /// - Manages connection states and validation
    /// - Updates plotter configurations and external windows
    ///
    /// # Performance Considerations
    /// - Uses efficient scrolling for large plugin configurations
    /// - Implements card height capping to prevent UI overflow
    /// - Optimizes rendering for plugins with many variables
    pub(crate) fn render_plugin_cards(&mut self, ctx: &egui::Context, panel_rect: egui::Rect) {
        self.render_plugin_cards_filtered(ctx, panel_rect, None);
    }

    pub(crate) fn render_plugin_cards_filtered(
        &mut self,
        ctx: &egui::Context,
        panel_rect: egui::Rect,
        only_connected: Option<bool>,
    ) {
        const CARD_WIDTH: f32 = 280.0;
        const CARD_FIXED_HEIGHT: f32 = 132.0;
        const PANEL_PAD: f32 = 8.0;
        let mut pending_info: Option<String> = None;
        let connected_input_ports: HashMap<u64, HashSet<String>> =
            self.workspace_manager.workspace.connections.iter().fold(
                HashMap::new(),
                |mut acc, conn| {
                    acc.entry(conn.to_plugin)
                        .or_insert_with(HashSet::new)
                        .insert(conn.to_port.clone());
                    acc
                },
            );
        let connected_output_ports: HashMap<u64, HashSet<String>> =
            self.workspace_manager.workspace.connections.iter().fold(
                HashMap::new(),
                |mut acc, conn| {
                    acc.entry(conn.from_plugin)
                        .or_insert_with(HashSet::new)
                        .insert(conn.from_port.clone());
                    acc
                },
            );
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
        let mut plugin_to_select: Option<u64> = None;

        // Build set of highlighted plugins if filtering
        let highlighted_plugins = self.get_highlighted_plugins();

        // Check if we should highlight plugin borders (for connection highlight mode)
        let has_connection_highlight = !matches!(self.highlight_mode, HighlightMode::None);

        let plugin_count = self.workspace_manager.workspace.plugins.len();
        for idx in 0..plugin_count {
            let kind = {
                let plugin_ref = &self.workspace_manager.workspace.plugins[idx];
                plugin_ref.kind.clone()
            };
            let parsed_schema = self.parsed_display_schema_for_kind(&kind);
            let plugin = &mut self.workspace_manager.workspace.plugins[idx];
            // Apply filter if specified
            if let Some(want_connected) = only_connected {
                let is_highlighted = highlighted_plugins.contains(&plugin.id);
                if want_connected != is_highlighted {
                    continue;
                }
            }

            let behavior = self
                .behavior_manager
                .cached_behaviors
                .get(&plugin.kind)
                .cloned()
                .unwrap_or_default();
            let opens_external_window = behavior.external_window;
            let opens_plotter_window = opens_external_window
                && crate::DEDICATED_PLOTTER_VIEW_KINDS.contains(&plugin.kind.as_str());
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
            } else if has_connection_highlight && highlighted_plugins.contains(&plugin.id) {
                // Highlight plugin borders when connections are highlighted
                // Use blue color that fits the palette
                Some(egui::Color32::from_rgb(100, 150, 255))
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
                                    let plugin_ui_schema = self
                                        .plugin_manager
                                        .installed_plugins
                                        .iter()
                                        .find(|p| p.manifest.kind == plugin.kind)
                                        .and_then(|p| p.ui_schema.clone());
                                    if !is_app_plugin {
                                        if let Some(schema) = plugin_ui_schema.as_ref() {
                                            match plugin.config {
                                                Value::Object(ref mut map) => {
                                                    for field in &schema.fields {
                                                        if map.contains_key(&field.key) {
                                                            continue;
                                                        }
                                                        let default_value = field.default.clone().unwrap_or_else(|| {
                                                            match &field.field_type {
                                                                rtsyn_plugin::ui::FieldType::Integer { .. } => {
                                                                    Value::from(0)
                                                                }
                                                                rtsyn_plugin::ui::FieldType::Float { .. } => {
                                                                    Value::from(0.0)
                                                                }
                                                                rtsyn_plugin::ui::FieldType::Text { .. }
                                                                | rtsyn_plugin::ui::FieldType::FilePath { .. } => {
                                                                    Value::from("")
                                                                }
                                                                rtsyn_plugin::ui::FieldType::Boolean => {
                                                                    Value::from(false)
                                                                }
                                                                rtsyn_plugin::ui::FieldType::Choice { options } => {
                                                                    Value::from(
                                                                        options
                                                                            .first()
                                                                            .cloned()
                                                                            .unwrap_or_default(),
                                                                    )
                                                                }
                                                                rtsyn_plugin::ui::FieldType::DynamicList { .. } => {
                                                                    Value::Array(Vec::new())
                                                                }
                                                            }
                                                        });
                                                        map.insert(field.key.clone(), default_value);
                                                        plugin_changed = true;
                                                    }

                                                    if !schema.fields.is_empty() {
                                                        egui::CollapsingHeader::new(
                                                            RichText::new("\u{f013}  Variables")
                                                                .size(13.0)
                                                                .strong(),
                                                        )
                                                        .default_open(starts_expanded)
                                                        .show(ui, |ui| {
                                                            ui.add_space(4.0);
                                                            let label_w = 140.0;
                                                            let value_w = (ui.available_width()
                                                                - label_w
                                                                - 8.0)
                                                                .max(80.0);
                                                            for field in &schema.fields {
                                                                let key = field.key.clone();
                                                                let label = field.label.clone();
                                                                if let Some(value) = map.get_mut(&key) {
                                                                    kv_row_wrapped(
                                                                        ui,
                                                                        &label,
                                                                        label_w,
                                                                        |ui| match &field.field_type {
                                                                            rtsyn_plugin::ui::FieldType::Choice {
                                                                                options,
                                                                            } => {
                                                                                let mut selected = value
                                                                                    .as_str()
                                                                                    .map(|s| s.to_string())
                                                                                    .unwrap_or_else(|| {
                                                                                        options
                                                                                            .first()
                                                                                            .cloned()
                                                                                            .unwrap_or_default()
                                                                                    });
                                                                                let old = selected.clone();
                                                                                egui::ComboBox::from_id_source((
                                                                                    plugin.id,
                                                                                    key.clone(),
                                                                                    "non_app_choice",
                                                                                ))
                                                                                .selected_text(selected.clone())
                                                                                .width(value_w)
                                                                                .show_ui(ui, |ui| {
                                                                                    for option in options {
                                                                                        let _ = ui.selectable_value(
                                                                                            &mut selected,
                                                                                            option.clone(),
                                                                                            option,
                                                                                        );
                                                                                    }
                                                                                });
                                                                                if selected != old {
                                                                                    *value = Value::String(selected);
                                                                                    plugin_changed = true;
                                                                                }
                                                                            }
                                                                            rtsyn_plugin::ui::FieldType::Boolean => {
                                                                                let mut checked =
                                                                                    value.as_bool().unwrap_or(false);
                                                                                if ui
                                                                                    .add_sized(
                                                                                        [value_w, 0.0],
                                                                                        egui::Checkbox::new(
                                                                                            &mut checked,
                                                                                            "",
                                                                                        ),
                                                                                    )
                                                                                    .changed()
                                                                                {
                                                                                    *value = Value::Bool(checked);
                                                                                    plugin_changed = true;
                                                                                }
                                                                            }
                                                                            rtsyn_plugin::ui::FieldType::Integer {
                                                                                min,
                                                                                max,
                                                                                step,
                                                                            } => {
                                                                                let mut val = value
                                                                                    .as_i64()
                                                                                    .unwrap_or_else(|| {
                                                                                        value.as_f64().unwrap_or(0.0)
                                                                                            as i64
                                                                                    });
                                                                                let range = match (min, max) {
                                                                                    (Some(mn), Some(mx)) => {
                                                                                        *mn..=*mx
                                                                                    }
                                                                                    (Some(mn), None) => {
                                                                                        *mn..=i64::MAX
                                                                                    }
                                                                                    (None, Some(mx)) => {
                                                                                        i64::MIN..=*mx
                                                                                    }
                                                                                    (None, None) => {
                                                                                        i64::MIN..=i64::MAX
                                                                                    }
                                                                                };
                                                                                if ui
                                                                                    .add_sized(
                                                                                        [value_w, 0.0],
                                                                                        egui::DragValue::new(
                                                                                            &mut val,
                                                                                        )
                                                                                        .speed(*step as f64)
                                                                                        .clamp_range(range),
                                                                                    )
                                                                                    .changed()
                                                                                {
                                                                                    *value = Value::from(val);
                                                                                    plugin_changed = true;
                                                                                }
                                                                            }
                                                                            rtsyn_plugin::ui::FieldType::Float {
                                                                                min,
                                                                                max,
                                                                                step,
                                                                            } => {
                                                                                let mut val =
                                                                                    value.as_f64().unwrap_or(0.0);
                                                                                let range = match (min, max) {
                                                                                    (Some(mn), Some(mx)) => {
                                                                                        *mn..=*mx
                                                                                    }
                                                                                    (Some(mn), None) => {
                                                                                        *mn..=f64::INFINITY
                                                                                    }
                                                                                    (None, Some(mx)) => {
                                                                                        f64::NEG_INFINITY..=*mx
                                                                                    }
                                                                                    (None, None) => {
                                                                                        f64::NEG_INFINITY
                                                                                            ..=f64::INFINITY
                                                                                    }
                                                                                };
                                                                                if ui
                                                                                    .add_sized(
                                                                                        [value_w, 0.0],
                                                                                        egui::DragValue::new(
                                                                                            &mut val,
                                                                                        )
                                                                                        .speed(*step)
                                                                                        .clamp_range(range),
                                                                                    )
                                                                                    .changed()
                                                                                {
                                                                                    *value = Value::from(val);
                                                                                    plugin_changed = true;
                                                                                }
                                                                            }
                                                                            rtsyn_plugin::ui::FieldType::Text {
                                                                                ..
                                                                            }
                                                                            | rtsyn_plugin::ui::FieldType::FilePath {
                                                                                ..
                                                                            } => {
                                                                                let mut text = value
                                                                                    .as_str()
                                                                                    .unwrap_or("")
                                                                                    .to_string();
                                                                                if ui
                                                                                    .add_sized(
                                                                                        [value_w, 0.0],
                                                                                        egui::TextEdit::singleline(
                                                                                            &mut text,
                                                                                        ),
                                                                                    )
                                                                                    .changed()
                                                                                {
                                                                                    *value = Value::String(text);
                                                                                    plugin_changed = true;
                                                                                }
                                                                            }
                                                                            rtsyn_plugin::ui::FieldType::DynamicList {
                                                                                ..
                                                                            } => {
                                                                                ui.label("List field not editable in this panel");
                                                                            }
                                                                        },
                                                                    );
                                                                }
                                                            }
                                                        });
                                                    }
                                                }
                                                _ => {
                                                    ui.label("Config is not an object.");
                                                }
                                            }
                                        } else {
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
                                                if !parsed_schema.inputs.is_empty() {
                                                    egui::CollapsingHeader::new(
                                                        RichText::new("\u{f090}  Inputs")
                                                            .size(13.0)
                                                            .strong(), // sign-in icon with space
                                                    )
                                                    .default_open(starts_expanded)
                                                    .show(ui, |ui| {
                                                        ui.add_space(4.0);
                                                        for entry in &parsed_schema.inputs {
                                                            let key_owned = entry.key.clone();
                                                            let value = input_values
                                                                .get(&(plugin.id, key_owned.clone()))
                                                                .copied()
                                                                .unwrap_or(0.0);
                                                            let mut value_text = format!("{value:.4}");
                                                            kv_row_wrapped(ui, &entry.label, 140.0, |ui| {
                                                                ui.add_enabled_ui(false, |ui| {
                                                                    ui.add_sized(
                                                                        [80.0, 0.0],
                                                                        egui::TextEdit::singleline(
                                                                            &mut value_text,
                                                                        ),
                                                                    );
                                                                });
                                                            });
                                                            ui.add_space(4.0);
                                                        }
                                                    });
                                                }

                                                // Outputs third
                                                if !parsed_schema.outputs.is_empty() {
                                                    egui::CollapsingHeader::new(
                                                        RichText::new("\u{f08b}  Outputs")
                                                            .size(13.0)
                                                            .strong(), // sign-out icon with space
                                                    )
                                                    .default_open(starts_expanded)
                                                    .show(ui, |ui| {
                                                        ui.add_space(4.0);
                                                        for entry in &parsed_schema.outputs {
                                                            let key_owned = entry.key.clone();
                                                            let value = computed_outputs
                                                                .get(&(plugin.id, key_owned))
                                                                .copied()
                                                                .unwrap_or(0.0);
                                                            let mut value_text = if value == 0.0 {
                                                                "0".to_string()
                                                            } else if (value.fract() - 0.0).abs()
                                                                < f64::EPSILON
                                                            {
                                                                format!("{value:.0}")
                                                            } else if value.abs() < 1e-3 {
                                                                format!("{value:.3e}")
                                                            } else {
                                                                format!("{value:.6}")
                                                            };
                                                            kv_row_wrapped(ui, &entry.label, 140.0, |ui| {
                                                                ui.add_enabled_ui(false, |ui| {
                                                                    ui.add_sized(
                                                                        [80.0, 0.0],
                                                                        egui::TextEdit::singleline(
                                                                            &mut value_text,
                                                                        ),
                                                                    );
                                                                });
                                                            });
                                                            ui.add_space(4.0);
                                                        }
                                                    });
                                                }

                                                if !parsed_schema.variables.is_empty() {
                                                    egui::CollapsingHeader::new(
                                                        RichText::new("\u{f085}  Internal variables")
                                                            .size(13.0)
                                                            .strong(),
                                                    )
                                                    .default_open(starts_expanded)
                                                    .show(ui, |ui| {
                                                        ui.add_space(4.0);
                                                        for entry in &parsed_schema.variables {
                                                            let key_owned = entry.key.clone();
                                                            let value = internal_variable_values
                                                                .get(&(plugin.id, key_owned.clone()))
                                                                .cloned()
                                                                .unwrap_or_else(|| {
                                                                    if matches!(
                                                                        plugin.kind.as_str(),
                                                                        "csv_recorder" | "live_plotter"
                                                                    ) {
                                                                        match entry.key.as_str() {
                                                                            "input_count" => {
                                                                                serde_json::Value::from(0)
                                                                            }
                                                                            "running" => {
                                                                                serde_json::Value::from(false)
                                                                            }
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
                                                                            .map(|v| {
                                                                                if v.is_finite()
                                                                                    && (v.fract()
                                                                                        - 0.0)
                                                                                        .abs()
                                                                                        < f64::EPSILON
                                                                                {
                                                                                    format!("{:.0}", v)
                                                                                } else {
                                                                                    format!("{:.4}", v)
                                                                                }
                                                                            })
                                                                            .unwrap_or_else(|| value.to_string())
                                                                    }
                                                                }
                                                                _ => value.to_string(),
                                                            };
                                                            kv_row_wrapped(ui, &entry.label, 140.0, |ui| {
                                                                ui.add_enabled_ui(false, |ui| {
                                                                    ui.add_sized(
                                                                        [80.0, 0.0],
                                                                        egui::TextEdit::singleline(
                                                                            &mut value_text,
                                                                        ),
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

                                                    if opens_plotter_window && plugin.running {
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

            // Move connected plugins to top so they render above connections
            if only_connected == Some(true) {
                ctx.move_to_top(response.response.layer_id);
            }

            if ctx.input(|i| {
                i.pointer
                    .button_double_clicked(egui::PointerButton::Primary)
            }) {
                if response.response.hovered() && !self.confirm_dialog.open {
                    // Toggle selection
                    if matches!(self.highlight_mode, HighlightMode::AllConnections(id) if id == plugin.id)
                    {
                        self.highlight_mode = HighlightMode::None;
                    } else {
                        plugin_to_select = Some(plugin.id);
                    }
                }
            }
            // On click, select plugin if connected, otherwise clear highlight
            // But skip if this is a double-click (to avoid blink)
            if response.response.clicked()
                && !self.confirm_dialog.open
                && !ctx.input(|i| {
                    i.pointer
                        .button_double_clicked(egui::PointerButton::Primary)
                })
            {
                let is_highlighted = highlighted_plugins.contains(&plugin.id);
                if is_highlighted {
                    // Clicking a highlighted plugin - select it
                    if !matches!(self.highlight_mode, HighlightMode::AllConnections(id) if id == plugin.id)
                    {
                        plugin_to_select = Some(plugin.id);
                    }
                } else {
                    // Clicking a non-highlighted plugin - clear connection highlight
                    self.highlight_mode = HighlightMode::None;
                }
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

        // Call shared function after loop to avoid borrow checker issues
        if let Some(plugin_id) = plugin_to_select {
            self.double_click_plugin(plugin_id);
        }
    }

    fn split_display_entry(entry: &str) -> (&str, &str) {
        if let Some((key, label)) = entry.split_once('|') {
            (key.trim(), label.trim())
        } else {
            let trimmed = entry.trim();
            (trimmed, trimmed)
        }
    }

    /// Renders a plugin preview panel showing plugin information and capabilities.
    ///
    /// This function creates a detailed preview of a plugin's characteristics, including
    /// its name, description, input/output ports, and variables. It's used in plugin
    /// management windows to help users understand plugin functionality before installation
    /// or addition to workspace.
    ///
    /// # Parameters
    /// - `ui`: The egui UI context for rendering
    /// - `manifest`: Plugin manifest containing metadata and configuration
    /// - `inputs_override`: Optional override for input port names (used for dynamic plugins)
    /// - `plugin_kind`: The plugin type identifier
    /// - `_plugin_config`: Plugin configuration (currently unused)
    /// - `_plugin_running`: Plugin running state (currently unused)
    /// - `installed_plugins`: List of all installed plugins for schema lookup
    ///
    /// # Preview Content
    /// - Plugin name and version information
    /// - Normalized description text with proper formatting
    /// - Input and output port listings
    /// - Special handling for extendable plugins (csv_recorder, live_plotter)
    /// - Variable listings with default values
    ///
    /// # Special Features
    /// - Handles incremental input ports for extendable plugins
    /// - Normalizes description text for better readability
    /// - Displays port information in a structured grid layout
    /// - Shows variable metadata when available
    pub(super) fn render_plugin_preview(
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
                let input_labels: Vec<String> = inputs
                    .iter()
                    .map(|entry| Self::split_display_entry(entry).1.to_string())
                    .collect();
                let mut inputs_label = input_labels.join(", ");
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
                            .map(|s| {
                                s.outputs
                                    .iter()
                                    .map(|entry| Self::split_display_entry(entry).1.to_string())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            })
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

    /// Renders a scrollable action panel for plugin preview windows.
    ///
    /// This function creates a standardized scrollable panel used in plugin management
    /// windows to display plugin previews and action buttons. It ensures consistent
    /// sizing and scrolling behavior across different plugin management interfaces.
    ///
    /// # Parameters
    /// - `ui`: The egui UI context for rendering
    /// - `full_h`: The full height available for the panel
    /// - `right_w`: The width of the right panel area
    /// - `body`: Closure that renders the panel content
    ///
    /// # Layout Features
    /// - Vertical scrolling with auto-shrink disabled for consistent sizing
    /// - Fixed width to prevent layout drift during content changes
    /// - Minimum and maximum height constraints for proper scrolling
    /// - Stable width baseline for consistent button centering
    pub(super) fn render_preview_action_panel(
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

    /// Generates input port names for live plotter plugins based on current configuration.
    ///
    /// This function creates dynamic input port names for live plotter plugins, which
    /// can have a variable number of input ports based on their configuration. It
    /// looks up the current input count and generates appropriately named ports.
    ///
    /// # Returns
    /// - `Some(Vec<String>)`: Vector of input port names (e.g., ["in_0", "in_1", ...])
    /// - `None`: If no live plotter plugin is found in the workspace
    ///
    /// # Port Naming
    /// Ports are named with the pattern "in_{index}" where index starts from 0.
    /// The number of ports is determined by the "input_count" configuration value,
    /// defaulting to 1 if not specified.
    ///
    /// # Usage
    /// This is primarily used in plugin preview windows to show the correct
    /// number of input ports for live plotter plugins before they are added
    /// to the workspace.
    pub(super) fn live_plotter_inputs_override(&self) -> Option<Vec<String>> {
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

    /// Normalizes plugin description text for better readability in previews.
    ///
    /// This function processes plugin description text to improve its presentation
    /// in preview panels. It handles spaced letters (like "R T S y n") by joining
    /// them when they form sequences, while preserving normal word spacing.
    ///
    /// # Parameters
    /// - `description`: The raw description text to normalize
    ///
    /// # Returns
    /// A normalized string with improved spacing and formatting
    ///
    /// # Processing Rules
    /// - Splits text into whitespace-separated tokens
    /// - Identifies sequences of single alphanumeric characters
    /// - Joins sequences of 3+ single characters into single words
    /// - Preserves sequences of 1-2 characters as separate tokens
    /// - Maintains normal word spacing for multi-character tokens
    ///
    /// # Example
    /// ```
    /// let input = "R T S y n real time synthesizer";
    /// let output = GuiApp::normalize_preview_description(input);
    /// // Returns: "RTSyn real time synthesizer"
    /// ```
    pub(super) fn normalize_preview_description(description: &str) -> String {
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
}
