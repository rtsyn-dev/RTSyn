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
use crate::HighlightMode;
use crate::WindowFocus;

impl GuiApp {
    /// Opens the connection editor window for a specific plugin.
    ///
    /// This function initializes the connection editor state and prepares it for display.
    /// The editor can be opened in either "Add" or "Remove" mode to manage connections
    /// for the specified plugin.
    ///
    /// # Parameters
    /// - `plugin_id`: The unique identifier of the plugin to manage connections for
    /// - `mode`: The editing mode (`ConnectionEditMode::Add` or `ConnectionEditMode::Remove`)
    ///
    /// # Side Effects
    /// - Sets the connection editor host to `Main`
    /// - Opens the connection editor window
    /// - Resets editor state (selected indices, ports, tabs)
    /// - Clears any existing connection highlights
    /// - Sets pending window focus based on the mode
    ///
    /// # Implementation Details
    /// The function resets all editor state to ensure a clean starting point:
    /// - Defaults to the "Outputs" tab
    /// - Clears any previous selections
    /// - Resets port indices to 0
    /// - Clears last selected plugin and tab tracking
    pub(crate) fn open_connection_editor(&mut self, plugin_id: u64, mode: ConnectionEditMode) {
        self.connection_editor_host = ConnectionEditorHost::Main;
        self.connection_editor.open = true;
        self.connection_editor.mode = mode;
        self.connection_editor.tab = ConnectionEditTab::Outputs;
        self.connection_editor.plugin_id = Some(plugin_id);
        self.connection_editor.selected_idx = None;
        self.connection_editor.from_port_idx = 0;
        self.connection_editor.to_port_idx = 0;
        self.connection_editor.last_selected = None;
        self.connection_editor.last_tab = None;
        self.connection_highlight_plugin_id = None;
        // Clear connection highlights when entering connection mode
        self.highlight_mode = HighlightMode::None;
        self.pending_window_focus = Some(match mode {
            ConnectionEditMode::Add => WindowFocus::ConnectionEditorAdd,
            ConnectionEditMode::Remove => WindowFocus::ConnectionEditorRemove,
        });
    }

    /// Opens the connection editor within a plugin window context.
    ///
    /// This is a specialized version of `open_connection_editor` that opens the editor
    /// within the context of a specific plugin window rather than as a standalone window.
    /// This allows for contextual connection editing when working within plugin interfaces.
    ///
    /// # Parameters
    /// - `host_plugin_id`: The ID of the plugin window that will host the connection editor
    /// - `plugin_id`: The ID of the plugin to manage connections for
    /// - `mode`: The editing mode (`ConnectionEditMode::Add` or `ConnectionEditMode::Remove`)
    ///
    /// # Side Effects
    /// - Calls `open_connection_editor` with the specified parameters
    /// - Sets the connection editor host to `PluginWindow(host_plugin_id)`
    ///
    /// # Implementation Details
    /// This function delegates the main editor setup to `open_connection_editor` and then
    /// overrides the host setting to embed the editor within the specified plugin window.
    pub(crate) fn open_connection_editor_in_plugin_window(
        &mut self,
        host_plugin_id: u64,
        plugin_id: u64,
        mode: ConnectionEditMode,
    ) {
        self.open_connection_editor(plugin_id, mode);
        self.connection_editor_host = ConnectionEditorHost::PluginWindow(host_plugin_id);
    }

    /// Renders the focused connection editor window for a specific plugin.
    ///
    /// This function displays a specialized connection editor that focuses on managing
    /// connections for a single plugin. It provides separate tabs for inputs and outputs
    /// and allows detailed connection management with other plugins in the workspace.
    ///
    /// # Parameters
    /// - `ctx`: The egui context for rendering UI elements
    ///
    /// # Window Features
    /// - **Plugin Information**: Displays the current plugin's ID, name, and description
    /// - **Tabbed Interface**: Separate tabs for managing input and output connections
    /// - **Plugin List**: Shows available plugins for connection with filtering based on existing connections
    /// - **Port Selection**: Detailed port selection with validation and conflict detection
    /// - **Connection Type**: Selection of connection kinds (audio, MIDI, etc.)
    /// - **Bidirectional Support**: Handles both adding and removing connections
    ///
    /// # Side Effects
    /// - Updates connection editor state based on user interactions
    /// - May call `add_connection_direct` or `remove_connection_with_input`
    /// - Updates plugin highlighting for visual feedback
    /// - Manages window focus and layer ordering
    /// - Resets editor state when closed
    ///
    /// # Implementation Details
    /// The editor operates in two modes:
    /// - **Add Mode**: Shows all compatible plugins and allows creating new connections
    /// - **Remove Mode**: Shows only plugins with existing connections for removal
    ///
    /// Key features include:
    /// - **Smart Port Selection**: Automatically selects appropriate ports based on existing connections
    /// - **Extendable Input Support**: Special handling for plugins with dynamic input ports
    /// - **Duplicate Prevention**: Prevents creation of identical connections
    /// - **Visual Feedback**: Highlights connected plugins and provides connection state information
    /// - **Context Preservation**: Remembers selections when switching between plugins/tabs
    ///
    /// The window is fixed-size (520x360) with a two-column layout for plugin selection and connection details.
    pub(crate) fn render_connection_editor(&mut self, ctx: &egui::Context) {
        if !self.connection_editor.open {
            return;
        }

        let mut open = self.connection_editor.open;
        let window_size = egui::vec2(520.0, 360.0);
        let default_pos = Self::center_window(ctx, window_size);
        let Some(current_id) = self.connection_editor.plugin_id else {
            self.connection_highlight_plugin_id = None;
            self.connection_editor.open = false;
            self.connection_editor_host = ConnectionEditorHost::Main;
            return;
        };
        let current_plugin = match self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .find(|plugin| plugin.id == current_id)
        {
            Some(plugin) => plugin.clone(),
            None => {
                self.connection_highlight_plugin_id = None;
                self.connection_editor.open = false;
                self.connection_editor_host = ConnectionEditorHost::Main;
                return;
            }
        };
        let name_by_kind: HashMap<String, String> = self
            .plugin_manager
            .installed_plugins
            .iter()
            .map(|plugin| (plugin.manifest.kind.clone(), plugin.manifest.name.clone()))
            .collect();
        let desc_by_kind: HashMap<String, String> = self
            .plugin_manager
            .installed_plugins
            .iter()
            .map(|plugin| {
                (
                    plugin.manifest.kind.clone(),
                    plugin.manifest.description.clone().unwrap_or_default(),
                )
            })
            .collect();

        let title = match self.connection_editor.mode {
            ConnectionEditMode::Add => "Add connections",
            ConnectionEditMode::Remove => "Remove connections",
        };
        let current_name = name_by_kind
            .get(&current_plugin.kind)
            .cloned()
            .unwrap_or_else(|| Self::display_kind(&current_plugin.kind));
        let response = egui::Window::new(title)
            .open(&mut open)
            .resizable(false)
            .default_pos(default_pos)
            .default_size(window_size)
            .fixed_size(window_size)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (id_rect, _) =
                        ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                    ui.painter()
                        .circle_filled(id_rect.center(), 9.0, egui::Color32::from_gray(60));
                    ui.painter().text(
                        id_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        current_id.to_string(),
                        egui::FontId::proportional(13.0),
                        ui.visuals().text_color(),
                    );
                    ui.add_sized(
                        [ui.available_width(), 0.0],
                        egui::Label::new(RichText::new(current_name).strong().size(16.0))
                            .wrap(true),
                    );
                });
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.connection_editor.tab,
                        ConnectionEditTab::Outputs,
                        "Outputs",
                    );
                    ui.selectable_value(
                        &mut self.connection_editor.tab,
                        ConnectionEditTab::Inputs,
                        "Inputs",
                    );
                });
                ui.separator();

                let mut candidates: Vec<usize> = Vec::new();
                for (idx, plugin) in self.workspace_manager.workspace.plugins.iter().enumerate() {
                    if plugin.id == current_id {
                        continue;
                    }
                    let has_connection = match self.connection_editor.tab {
                        ConnectionEditTab::Inputs => self
                            .workspace_manager
                            .workspace
                            .connections
                            .iter()
                            .any(|conn| {
                                conn.to_plugin == current_id && conn.from_plugin == plugin.id
                            }),
                        ConnectionEditTab::Outputs => self
                            .workspace_manager
                            .workspace
                            .connections
                            .iter()
                            .any(|conn| {
                                conn.from_plugin == current_id && conn.to_plugin == plugin.id
                            }),
                    };
                    if self.connection_editor.mode == ConnectionEditMode::Remove && !has_connection
                    {
                        continue;
                    }
                    candidates.push(idx);
                }

                if self.connection_editor.selected_idx.is_some()
                    && self
                        .connection_editor
                        .selected_idx
                        .map(|idx| !candidates.contains(&idx))
                        .unwrap_or(false)
                {
                    self.connection_editor.selected_idx = None;
                    self.connection_highlight_plugin_id = None;
                }

                ui.columns(2, |columns| {
                    columns[0].vertical(|ui| {
                        ui.label("Plugins");
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for idx in &candidates {
                                let plugin = &self.workspace_manager.workspace.plugins[*idx];
                                let name = name_by_kind
                                    .get(&plugin.kind)
                                    .cloned()
                                    .unwrap_or_else(|| Self::display_kind(&plugin.kind));
                                let label = format!("#{} {}", plugin.id, name);
                                if ui
                                    .selectable_label(
                                        self.connection_editor.selected_idx == Some(*idx),
                                        label,
                                    )
                                    .clicked()
                                {
                                    self.connection_editor.selected_idx = Some(*idx);
                                    self.connection_highlight_plugin_id = Some(plugin.id);
                                    self.connection_editor.last_selected = None;
                                }
                            }
                        });
                    });

                    columns[1].vertical(|ui| {
                        if let Some(selected_idx) = self.connection_editor.selected_idx {
                            let selected_plugin =
                                &self.workspace_manager.workspace.plugins[selected_idx];
                            let selected_name = name_by_kind
                                .get(&selected_plugin.kind)
                                .cloned()
                                .unwrap_or_else(|| Self::display_kind(&selected_plugin.kind));
                            let selected_desc = desc_by_kind
                                .get(&selected_plugin.kind)
                                .cloned()
                                .unwrap_or_default();
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
                                    selected_plugin.id.to_string(),
                                    egui::FontId::proportional(13.0),
                                    ui.visuals().text_color(),
                                );
                                ui.add_sized(
                                    [ui.available_width(), 0.0],
                                    egui::Label::new(
                                        RichText::new(selected_name).strong().size(16.0),
                                    )
                                    .wrap(true),
                                );
                            });
                            if !selected_desc.is_empty() {
                                ui.add(egui::Label::new(selected_desc).wrap(true));
                            }
                            ui.add_space(6.0);

                            match self.connection_editor.mode {
                                ConnectionEditMode::Add => {
                                    let (from_plugin, to_plugin) = match self.connection_editor.tab
                                    {
                                        ConnectionEditTab::Inputs => {
                                            (selected_plugin.id, current_id)
                                        }
                                        ConnectionEditTab::Outputs => {
                                            (current_id, selected_plugin.id)
                                        }
                                    };

                                    let from_ports = self.ports_for_plugin(from_plugin, false);
                                    let to_ports = self.ports_for_plugin(to_plugin, true);
                                    let extendable = self
                                        .workspace_manager
                                        .workspace
                                        .plugins
                                        .iter()
                                        .find(|p| p.id == to_plugin)
                                        .map(|plugin| self.is_extendable_inputs(&plugin.kind))
                                        .unwrap_or(false);
                                    let pair_connection = self
                                        .workspace_manager
                                        .workspace
                                        .connections
                                        .iter()
                                        .find(|conn| {
                                            conn.from_plugin == from_plugin
                                                && conn.to_plugin == to_plugin
                                        })
                                        .cloned();
                                    let display_to_ports = if extendable {
                                        self.extendable_input_display_ports(
                                            to_plugin,
                                            true, // Always show placeholder for extendable inputs
                                        )
                                    } else {
                                        to_ports.clone()
                                    };
                                    let missing_ports =
                                        from_ports.is_empty() || display_to_ports.is_empty();
                                    let first_available_to_port =
                                        |ports: &[String]| -> Option<usize> {
                                            ports.iter().position(|port| {
                                                !self
                                                    .workspace_manager
                                                    .workspace
                                                    .connections
                                                    .iter()
                                                    .any(|conn| {
                                                        conn.to_plugin == to_plugin
                                                            && conn.to_port == *port
                                                    })
                                            })
                                        };
                                    if self.connection_editor.last_selected
                                        != Some(selected_plugin.id)
                                        || self.connection_editor.last_tab
                                            != Some(self.connection_editor.tab)
                                    {
                                        self.connection_editor.from_port_idx = 0;
                                        if let Some(connection) = pair_connection.as_ref() {
                                            if let Some(pos) = display_to_ports
                                                .iter()
                                                .position(|port| port == &connection.to_port)
                                            {
                                                self.connection_editor.to_port_idx = pos;
                                                if self
                                                    .connection_editor
                                                    .kind_options
                                                    .contains(&connection.kind)
                                                {
                                                    self.connection_editor.kind =
                                                        connection.kind.clone();
                                                }
                                            } else {
                                                self.connection_editor.to_port_idx =
                                                    first_available_to_port(&display_to_ports)
                                                        .unwrap_or(0);
                                            }
                                        } else {
                                            self.connection_editor.to_port_idx =
                                                first_available_to_port(&display_to_ports)
                                                    .unwrap_or(0);
                                        }
                                        self.connection_editor.last_selected =
                                            Some(selected_plugin.id);
                                        self.connection_editor.last_tab =
                                            Some(self.connection_editor.tab);
                                    }
                                    if self.connection_editor.from_port_idx >= from_ports.len() {
                                        self.connection_editor.from_port_idx = 0;
                                    }
                                    if self.connection_editor.to_port_idx >= display_to_ports.len()
                                    {
                                        self.connection_editor.to_port_idx = 0;
                                    }

                                    ui.label("Ports");
                                    let direction_label = match self.connection_editor.tab {
                                        ConnectionEditTab::Inputs => {
                                            "Direction: Selected plugin -> Current plugin"
                                        }
                                        ConnectionEditTab::Outputs => {
                                            "Direction: Current plugin -> Selected plugin"
                                        }
                                    };
                                    ui.label(
                                        RichText::new(direction_label).color(egui::Color32::GRAY),
                                    );
                                    ui.horizontal(|ui| {
                                        ui.label("Source");
                                        if from_ports.is_empty() {
                                            ui.label("No outputs");
                                        } else {
                                            egui::ComboBox::from_id_source("conn_edit_from_port")
                                                .selected_text(
                                                    from_ports
                                                        [self.connection_editor.from_port_idx]
                                                        .clone(),
                                                )
                                                .show_ui(ui, |ui| {
                                                    for (idx, port) in from_ports.iter().enumerate()
                                                    {
                                                        ui.selectable_value(
                                                            &mut self
                                                                .connection_editor
                                                                .from_port_idx,
                                                            idx,
                                                            port,
                                                        );
                                                    }
                                                });
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("Target");
                                        if display_to_ports.is_empty() {
                                            ui.label("No inputs");
                                        } else {
                                            egui::ComboBox::from_id_source("conn_edit_to_port")
                                                .selected_text(
                                                    display_to_ports
                                                        [self.connection_editor.to_port_idx]
                                                        .clone(),
                                                )
                                                .show_ui(ui, |ui| {
                                                    for (idx, port) in
                                                        display_to_ports.iter().enumerate()
                                                    {
                                                        ui.selectable_value(
                                                            &mut self.connection_editor.to_port_idx,
                                                            idx,
                                                            port,
                                                        );
                                                    }
                                                });
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("Kind");
                                        egui::ComboBox::from_id_source("conn_edit_kind")
                                            .selected_text(Self::display_connection_kind(
                                                &self.connection_editor.kind,
                                            ))
                                            .show_ui(ui, |ui| {
                                                for kind in &self.connection_editor.kind_options {
                                                    ui.selectable_value(
                                                        &mut self.connection_editor.kind,
                                                        kind.clone(),
                                                        Self::display_connection_kind(kind),
                                                    );
                                                }
                                            });
                                    });
                                    if missing_ports {
                                        ui.label(
                                            RichText::new("Add inputs/outputs to connect.")
                                                .color(egui::Color32::GRAY),
                                        );
                                    } else {
                                        let from_port = from_ports
                                            [self.connection_editor.from_port_idx]
                                            .clone();
                                        let to_port = display_to_ports
                                            [self.connection_editor.to_port_idx]
                                            .clone();
                                        let exact_idx = self
                                            .workspace_manager
                                            .workspace
                                            .connections
                                            .iter()
                                            .position(|conn| {
                                                conn.from_plugin == from_plugin
                                                    && conn.to_plugin == to_plugin
                                                    && conn.from_port == from_port
                                                    && conn.to_port == to_port
                                                    && conn.kind == self.connection_editor.kind
                                            });
                                        let has_duplicate = exact_idx.is_some();
                                        ui.horizontal(|ui| {
                                            ui.add_enabled_ui(!has_duplicate, |ui| {
                                                if styled_button(ui, "Add connection").clicked() {
                                                    self.add_connection_direct(
                                                        from_plugin,
                                                        from_port.clone(),
                                                        to_plugin,
                                                        to_port.clone(),
                                                        self.connection_editor.kind.clone(),
                                                    );
                                                }
                                            });
                                            if let Some(idx) = exact_idx {
                                                if styled_button(ui, "Remove connection").clicked()
                                                {
                                                    let connection = self
                                                        .workspace_manager
                                                        .workspace
                                                        .connections[idx]
                                                        .clone();
                                                    self.remove_connection_with_input(connection);
                                                }
                                            }
                                        });
                                    }
                                }
                                ConnectionEditMode::Remove => {
                                    let connections: Vec<(usize, &ConnectionDefinition)> = self
                                        .workspace_manager
                                        .workspace
                                        .connections
                                        .iter()
                                        .enumerate()
                                        .filter(|(_, conn)| match self.connection_editor.tab {
                                            ConnectionEditTab::Inputs => {
                                                conn.to_plugin == current_id
                                                    && conn.from_plugin == selected_plugin.id
                                            }
                                            ConnectionEditTab::Outputs => {
                                                conn.from_plugin == current_id
                                                    && conn.to_plugin == selected_plugin.id
                                            }
                                        })
                                        .collect();
                                    if connections.is_empty() {
                                        ui.label("No connections to remove.");
                                    } else {
                                        let mut remove_idx: Option<usize> = None;
                                        for (idx, conn) in connections {
                                            ui.horizontal(|ui| {
                                                ui.label(format!(
                                                    "{}:{} -> {}:{} ({})",
                                                    conn.from_plugin,
                                                    conn.from_port,
                                                    conn.to_plugin,
                                                    conn.to_port,
                                                    Self::display_connection_kind(&conn.kind)
                                                ));
                                                if styled_button(ui, "Remove").clicked() {
                                                    remove_idx = Some(idx);
                                                }
                                            });
                                        }
                                        if let Some(idx) = remove_idx {
                                            let connection =
                                                self.workspace_manager.workspace.connections[idx]
                                                    .clone();
                                            self.remove_connection_with_input(connection);
                                        }
                                    }
                                }
                            }
                        } else {
                            ui.label("Select a plugin to manage connections.");
                        }
                    });
                });
            });
        if let Some(response) = response {
            self.window_rects.push(response.response.rect);
            if !self.confirm_dialog.open
                && (response.response.clicked() || response.response.dragged())
            {
                ctx.move_to_top(response.response.layer_id);
            }
            let expected_focus = match self.connection_editor.mode {
                ConnectionEditMode::Add => WindowFocus::ConnectionEditorAdd,
                ConnectionEditMode::Remove => WindowFocus::ConnectionEditorRemove,
            };
            if self.pending_window_focus == Some(expected_focus) {
                ctx.move_to_top(response.response.layer_id);
                self.pending_window_focus = None;
            }
        }

        if !open {
            self.connection_editor.open = false;
            self.connection_highlight_plugin_id = None;
            self.connection_editor.plugin_id = None;
            self.connection_editor_host = ConnectionEditorHost::Main;
        }
    }
}
