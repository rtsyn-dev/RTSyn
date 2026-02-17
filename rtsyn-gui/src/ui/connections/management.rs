//! Connection management UI components for the RTSyn GUI application.
//!
//! This module provides the user interface for managing audio connections between plugins
//! in the RTSyn workspace. It includes functionality for:
//!
//! - Opening and managing connection editors for adding/removing connections
//! - Rendering connection management windows with plugin selection and port configuration
//! - Visual connection display with interactive connection lines between plugins
//! - Context menus for connection operations
//! - Support for both fixed and extendable input/output ports
//!
//! The connection system supports different connection types (audio, MIDI, etc.) and
//! provides visual feedback for connection states, including highlighting and tooltips.

use super::*;
use crate::WindowFocus;

impl GuiApp {
    /// Renders the main connection management window.
    ///
    /// This function displays a comprehensive interface for managing all connections in the workspace.
    /// It provides both a list view of existing connections and controls for adding new connections
    /// between plugins.
    ///
    /// # Parameters
    /// - `ctx`: The egui context for rendering UI elements
    ///
    /// # Window Features
    /// - **Connection List**: Displays all existing connections with remove buttons
    /// - **Add Connection Interface**: Dropdown menus for selecting source/target plugins and ports
    /// - **Connection Type Selection**: Allows choosing between different connection kinds
    /// - **Validation**: Prevents duplicate connections and invalid configurations
    ///
    /// # Side Effects
    /// - Updates `self.windows.manage_connections_open` based on window state
    /// - May call `remove_connection_with_input` when connections are removed
    /// - May call `add_connection` when new connections are created
    /// - Updates connection editor state for plugin/port selection
    /// - Manages window focus and positioning
    ///
    /// # Implementation Details
    /// The function handles several complex scenarios:
    /// - **Plugin Validation**: Ensures at least 2 plugins exist before allowing connections
    /// - **Port Discovery**: Dynamically discovers available input/output ports for selected plugins
    /// - **Extendable Inputs**: Special handling for plugins with dynamically extensible input ports
    /// - **Connection Validation**: Prevents self-connections and duplicate connections
    /// - **Auto-extension**: Automatically creates new input ports for compatible plugins
    ///
    /// The window is fixed-size (420x360) and non-resizable for consistent layout.
    pub(crate) fn render_manage_connections_window(&mut self, ctx: &egui::Context) {
        if !self.windows.manage_connections_open {
            return;
        }
        let mut open = self.windows.manage_connections_open;
        let name_by_kind: HashMap<String, String> = self
            .plugin_manager
            .installed_plugins
            .iter()
            .map(|plugin| (plugin.manifest.kind.clone(), plugin.manifest.name.clone()))
            .collect();
        let window_size = egui::vec2(420.0, 360.0);
        let default_pos = Self::center_window(ctx, window_size);
        let response = egui::Window::new("Manage connections")
            .open(&mut open)
            .resizable(false)
            .default_pos(default_pos)
            .default_size(window_size)
            .fixed_size(window_size)
            .show(ctx, |ui| {
                ui.label("Connections");
                if self.workspace_manager.workspace.connections.is_empty() {
                    ui.label("No connections yet.");
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(140.0)
                        .show(ui, |ui| {
                            for (idx, connection) in self
                                .workspace_manager
                                .workspace
                                .connections
                                .iter()
                                .enumerate()
                            {
                                let display_idx = idx + 1;
                                ui.label(format!(
                                    "{}:{} -> {}:{} ({})",
                                    connection.from_plugin,
                                    connection.from_port,
                                    connection.to_plugin,
                                    connection.to_port,
                                    Self::display_connection_kind(&connection.kind)
                                ));
                                if styled_button(ui, format!("Remove #{display_idx}")).clicked() {
                                    let connection = connection.clone();
                                    self.remove_connection_with_input(connection);
                                    break;
                                }
                            }
                        });
                }

                ui.separator();
                ui.label("Add connection");
                if self.workspace_manager.workspace.plugins.len() < 2 {
                    ui.label("Add at least two plugins.");
                    return;
                }

                let plugin_len = self.workspace_manager.workspace.plugins.len();
                if self.connection_editor.from_idx >= plugin_len {
                    self.connection_editor.from_idx = 0;
                }
                if self.connection_editor.to_idx >= plugin_len {
                    self.connection_editor.to_idx = 0;
                }
                if self.connection_editor.from_idx == self.connection_editor.to_idx
                    && plugin_len > 1
                {
                    self.connection_editor.to_idx =
                        (self.connection_editor.from_idx + 1) % plugin_len;
                }

                let from_kind = self.workspace_manager.workspace.plugins
                    [self.connection_editor.from_idx]
                    .kind
                    .clone();
                let to_kind = self.workspace_manager.workspace.plugins
                    [self.connection_editor.to_idx]
                    .kind
                    .clone();

                // Cache behaviors before using them
                self.ensure_plugin_behavior_cached(&from_kind);
                self.ensure_plugin_behavior_cached(&to_kind);

                let from_id =
                    self.workspace_manager.workspace.plugins[self.connection_editor.from_idx].id;
                let to_id =
                    self.workspace_manager.workspace.plugins[self.connection_editor.to_idx].id;
                let mut from_ports = self.ports_for_plugin(from_id, false);
                let mut to_ports = self.ports_for_plugin(to_id, true);
                if from_ports.is_empty() {
                    from_ports.push("out".to_string());
                }
                let extendable = self.is_extendable_inputs(&to_kind);
                let auto_extend = self.auto_extend_inputs(&to_kind);
                if to_ports.is_empty() {
                    if extendable && auto_extend.is_empty() {
                        ui.label("Add inputs to this plugin before connecting.");
                        return;
                    }
                    if extendable {
                        let next_idx = self.next_available_extendable_input_index(to_id);
                        let next_name = format!("in_{next_idx}");
                        to_ports.push(next_name);
                    } else {
                        to_ports.push("in".to_string());
                    }
                }
                if !from_ports.contains(&self.connection_editor.from_port) {
                    self.connection_editor.from_port = from_ports[0].clone();
                }
                if !self
                    .connection_editor
                    .kind_options
                    .contains(&self.connection_editor.kind)
                {
                    self.connection_editor.kind = self.connection_editor.kind_options[0].clone();
                }
                let pair_connection = self
                    .workspace_manager
                    .workspace
                    .connections
                    .iter()
                    .find(|conn| conn.from_plugin == from_id && conn.to_plugin == to_id)
                    .cloned();
                let has_pair_connection = pair_connection.is_some();
                let exact_connection = self
                    .workspace_manager
                    .workspace
                    .connections
                    .iter()
                    .find(|conn| {
                        conn.from_plugin == from_id
                            && conn.to_plugin == to_id
                            && conn.from_port == self.connection_editor.from_port
                            && conn.to_port == self.connection_editor.to_port
                            && conn.kind == self.connection_editor.kind
                    })
                    .cloned();
                let has_duplicate = exact_connection.is_some();
                let display_to_ports = if extendable {
                    self.extendable_input_display_ports(to_id, !has_pair_connection)
                } else {
                    to_ports.clone()
                };
                if let Some(connection) = pair_connection.as_ref() {
                    if display_to_ports.contains(&connection.to_port) {
                        self.connection_editor.to_port = connection.to_port.clone();
                        if self
                            .connection_editor
                            .kind_options
                            .contains(&connection.kind)
                        {
                            self.connection_editor.kind = connection.kind.clone();
                        }
                    }
                } else if !display_to_ports.contains(&self.connection_editor.to_port) {
                    if let Some(default_port) = display_to_ports.first().cloned() {
                        self.connection_editor.to_port = default_port;
                    }
                }

                ui.horizontal(|ui| {
                    ui.label("From");
                    egui::ComboBox::from_id_source("conn_from_plugin")
                        .selected_text({
                            let name = name_by_kind
                                .get(&from_kind)
                                .cloned()
                                .unwrap_or_else(|| Self::display_kind(&from_kind));
                            format!("#{} {}", from_id, name)
                        })
                        .show_ui(ui, |ui| {
                            for (idx, plugin) in
                                self.workspace_manager.workspace.plugins.iter().enumerate()
                            {
                                let name = name_by_kind
                                    .get(&plugin.kind)
                                    .cloned()
                                    .unwrap_or_else(|| Self::display_kind(&plugin.kind));
                                let label = format!("#{} {}", plugin.id, name);
                                ui.selectable_value(
                                    &mut self.connection_editor.from_idx,
                                    idx,
                                    label,
                                );
                            }
                        });
                    ui.label("Port");
                    egui::ComboBox::from_id_source("conn_from_port")
                        .selected_text(self.connection_editor.from_port.clone())
                        .show_ui(ui, |ui| {
                            for port in &from_ports {
                                ui.selectable_value(
                                    &mut self.connection_editor.from_port,
                                    port.clone(),
                                    port,
                                );
                            }
                        });
                });

                ui.horizontal(|ui| {
                    ui.label("To");
                    egui::ComboBox::from_id_source("conn_to_plugin")
                        .selected_text({
                            let name = name_by_kind
                                .get(&to_kind)
                                .cloned()
                                .unwrap_or_else(|| Self::display_kind(&to_kind));
                            format!("#{} {}", to_id, name)
                        })
                        .show_ui(ui, |ui| {
                            for (idx, plugin) in
                                self.workspace_manager.workspace.plugins.iter().enumerate()
                            {
                                let name = name_by_kind
                                    .get(&plugin.kind)
                                    .cloned()
                                    .unwrap_or_else(|| Self::display_kind(&plugin.kind));
                                let label = format!("#{} {}", plugin.id, name);
                                if idx == self.connection_editor.from_idx {
                                    ui.add_enabled(false, egui::Label::new(label));
                                } else {
                                    ui.selectable_value(
                                        &mut self.connection_editor.to_idx,
                                        idx,
                                        label,
                                    );
                                }
                            }
                        });
                    ui.label("Port");
                    egui::ComboBox::from_id_source("conn_to_port")
                        .selected_text(self.connection_editor.to_port.clone())
                        .show_ui(ui, |ui| {
                            for port in &display_to_ports {
                                ui.selectable_value(
                                    &mut self.connection_editor.to_port,
                                    port.clone(),
                                    port,
                                );
                            }
                        });
                });

                ui.horizontal(|ui| {
                    ui.label("Kind");
                    egui::ComboBox::from_id_source("conn_kind")
                        .selected_text(Self::display_connection_kind(&self.connection_editor.kind))
                        .show_ui(ui, |ui| {
                            for kind in &self.connection_editor.kind_options {
                                ui.selectable_value(
                                    &mut self.connection_editor.kind,
                                    kind.clone(),
                                    Self::display_connection_kind(kind),
                                );
                            }
                        });
                    if let Some(connection) = exact_connection.clone() {
                        if ui
                            .add_sized([160.0, 28.0], egui::Button::new("Remove connection"))
                            .clicked()
                        {
                            self.remove_connection_with_input(connection);
                        }
                    }
                    if ui
                        .add_enabled(!has_duplicate, egui::Button::new("Add connection"))
                        .clicked()
                    {
                        self.add_connection();
                    }
                });
            });
        if let Some(response) = response {
            self.window_rects.push(response.response.rect);
            if !self.confirm_dialog.open
                && (response.response.clicked() || response.response.dragged())
            {
                ctx.move_to_top(response.response.layer_id);
            }
            if self.pending_window_focus == Some(WindowFocus::ManageConnections) {
                ctx.move_to_top(response.response.layer_id);
                self.pending_window_focus = None;
            }
        }

        self.windows.manage_connections_open = open;
    }
}
