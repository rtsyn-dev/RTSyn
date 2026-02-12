use super::*;
use crate::WindowFocus;

impl GuiApp {
    pub(crate) fn open_connection_editor(&mut self, plugin_id: u64, mode: ConnectionEditMode) {
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
        self.pending_window_focus = Some(match mode {
            ConnectionEditMode::Add => WindowFocus::ConnectionEditorAdd,
            ConnectionEditMode::Remove => WindowFocus::ConnectionEditorRemove,
        });
    }

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
                    if extendable && !auto_extend {
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
        }
    }

    pub(crate) fn render_connection_view(&mut self, ctx: &egui::Context, panel_rect: egui::Rect) {
        if !self.connections_view_enabled {
            self.connection_context_menu = None;
            return;
        }

        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("connection_lines"),
        ));

        let mut groups: HashMap<(u64, u64), (Vec<String>, Vec<String>, Vec<usize>)> =
            HashMap::new();
        for (idx, connection) in self
            .workspace_manager
            .workspace
            .connections
            .iter()
            .enumerate()
        {
            let entry = groups
                .entry((connection.from_plugin, connection.to_plugin))
                .or_insert_with(|| (Vec::new(), Vec::new(), Vec::new()));
            entry.0.push(connection.from_port.clone());
            entry.1.push(connection.to_port.clone());
            entry.2.push(idx);
        }

        let mut keys: Vec<(u64, u64)> = groups.keys().copied().collect();
        keys.sort();

        let unique_ports = |ports: &[String]| -> Vec<String> {
            let set: HashSet<String> = ports.iter().cloned().collect();
            let mut list: Vec<String> = set.into_iter().collect();
            list.sort();
            list
        };

        let out_color = egui::Color32::from_rgb(80, 200, 120);
        let in_color = egui::Color32::from_rgb(255, 170, 80);
        let selected_plugin = if self.confirm_dialog.open {
            None
        } else {
            self.selected_plugin_id
        };
        let with_alpha = |color: egui::Color32, alpha: u8| {
            egui::Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), alpha)
        };

        let pointer_pos = ctx.input(|i| i.pointer.hover_pos());
        let pointer_over_plugin = pointer_pos
            .map(|pos| self.plugin_rects.values().any(|rect| rect.contains(pos)))
            .unwrap_or(false);
        let pointer_over_window = pointer_pos
            .map(|pos| self.window_rects.iter().any(|rect| rect.contains(pos)))
            .unwrap_or(false);

        let mut best_hover: Option<(f32, egui::Pos2, Vec<String>, Vec<String>, u64, u64, usize)> =
            None;
        for (from_id, to_id) in keys {
            let Some((from_ports, to_ports, conn_indices)) = groups.get(&(from_id, to_id)) else {
                continue;
            };
            let reverse_ports = groups.get(&(to_id, from_id));
            if reverse_ports.is_some() && from_id > to_id {
                continue;
            }
            let unique_outputs = unique_ports(from_ports);
            let unique_inputs = unique_ports(to_ports);
            let conn_index = conn_indices.iter().min().copied().unwrap_or(0);
            let conn_display_index = conn_index + 1;
            let Some(from_rect) = self.plugin_rects.get(&from_id) else {
                continue;
            };
            let Some(to_rect) = self.plugin_rects.get(&to_id) else {
                continue;
            };
            if !panel_rect.intersects(*from_rect) && !panel_rect.intersects(*to_rect) {
                continue;
            }

            let start = from_rect.center();
            let end = to_rect.center();
            let dir = (end - start).normalized();
            let perp = egui::vec2(-dir.y, dir.x);
            let offset = if reverse_ports.is_some() {
                perp * 6.0
            } else {
                egui::Vec2::ZERO
            };

            let is_selected = selected_plugin
                .map(|selected| selected == from_id || selected == to_id)
                .unwrap_or(false);
            let (out_line, in_line, stroke) = if let Some(_) = selected_plugin {
                if is_selected {
                    (out_color, in_color, 4.0)
                } else {
                    (with_alpha(out_color, 80), with_alpha(in_color, 80), 1.5)
                }
            } else {
                (out_color, in_color, 2.0)
            };

            let draw_line = |start: egui::Pos2,
                             end: egui::Pos2,
                             painter: &egui::Painter,
                             out_line: egui::Color32,
                             in_line: egui::Color32,
                             stroke: f32| {
                let mid = egui::pos2((start.x + end.x) * 0.5, (start.y + end.y) * 0.5);
                painter.line_segment([start, mid], (stroke, out_line));
                painter.line_segment([mid, end], (stroke, in_line));

                let dir = (end - start).normalized();
                let arrow_len = 8.0;
                let arrow_width = 5.0;
                let tip = mid + dir * arrow_len;
                let left = mid + egui::vec2(-dir.y, dir.x) * arrow_width;
                let right = mid + egui::vec2(dir.y, -dir.x) * arrow_width;
                painter.add(egui::Shape::convex_polygon(
                    vec![tip, left, right],
                    out_line,
                    egui::Stroke::NONE,
                ));
                mid
            };

            let mid_primary = draw_line(
                start + offset,
                end + offset,
                &painter,
                out_line,
                in_line,
                stroke,
            );
            let (mid_reverse, reverse_outputs, reverse_inputs, reverse_index) =
                if let Some((rev_out, rev_in, rev_indices)) = reverse_ports {
                    let mid = draw_line(
                        end - offset,
                        start - offset,
                        &painter,
                        out_line,
                        in_line,
                        stroke,
                    );
                    let rev_index = rev_indices.iter().min().copied().unwrap_or(0);
                    (
                        Some(mid),
                        unique_ports(rev_out),
                        unique_ports(rev_in),
                        rev_index,
                    )
                } else {
                    (None, Vec::new(), Vec::new(), 0)
                };

            if let Some(pointer) = pointer_pos {
                if pointer_over_plugin || pointer_over_window {
                    continue;
                }
                if self.confirm_dialog.open {
                    continue;
                }
                let hover_pad = 10.0;
                let dist_primary = distance_to_segment(pointer, start + offset, end + offset);
                if dist_primary <= hover_pad {
                    let replace = best_hover
                        .as_ref()
                        .map(|(dist, _, _, _, _, _, _)| dist_primary < *dist)
                        .unwrap_or(true);
                    if replace {
                        best_hover = Some((
                            dist_primary,
                            mid_primary,
                            unique_outputs.clone(),
                            unique_inputs.clone(),
                            from_id,
                            to_id,
                            conn_display_index,
                        ));
                    }
                }
                if let Some(mid) = mid_reverse {
                    let dist_reverse = distance_to_segment(pointer, end - offset, start - offset);
                    if dist_reverse <= hover_pad {
                        let replace = best_hover
                            .as_ref()
                            .map(|(dist, _, _, _, _, _, _)| dist_reverse < *dist)
                            .unwrap_or(true);
                        if replace {
                            best_hover = Some((
                                dist_reverse,
                                mid,
                                reverse_outputs.clone(),
                                reverse_inputs.clone(),
                                to_id,
                                from_id,
                                reverse_index + 1,
                            ));
                        }
                    }
                }
            }
        }
        if self.confirm_dialog.open {
            best_hover = None;
        }
        if let (Some(pointer), Some((_dist, _mid, outputs, inputs, from_id, to_id, conn_index))) =
            (pointer_pos, best_hover)
        {
            // Only show tooltip if pointer is not over any UI element
            if pointer_over_plugin
                || pointer_over_window
                || self.confirm_dialog.open
                || ctx.is_pointer_over_area()
            {
                // Still allow right-click menu
                if ctx.input(|i| i.pointer.secondary_clicked()) && !self.confirm_dialog.open {
                    let matched: Vec<ConnectionDefinition> = self
                        .workspace_manager
                        .workspace
                        .connections
                        .iter()
                        .filter(|conn| conn.from_plugin == from_id && conn.to_plugin == to_id)
                        .cloned()
                        .collect();
                    if !matched.is_empty() {
                        self.connection_context_menu = Some((matched, pointer, ctx.frame_nr()));
                    }
                }
                return;
            }
            if ctx.input(|i| i.pointer.secondary_clicked()) && !self.confirm_dialog.open {
                let matched: Vec<ConnectionDefinition> = self
                    .workspace_manager
                    .workspace
                    .connections
                    .iter()
                    .filter(|conn| conn.from_plugin == from_id && conn.to_plugin == to_id)
                    .cloned()
                    .collect();
                if !matched.is_empty() {
                    self.connection_context_menu = Some((matched, pointer, ctx.frame_nr()));
                }
            }
            let outputs_len = outputs.len();
            let inputs_len = inputs.len();
            let outputs = if outputs.is_empty() {
                "none".to_string()
            } else {
                outputs.join(", ")
            };
            let inputs = if inputs.is_empty() {
                "none".to_string()
            } else {
                inputs.join(", ")
            };
            let mut tooltip_pos = pointer + egui::vec2(12.0, 12.0);
            if tooltip_pos.y < panel_rect.min.y + 6.0 {
                tooltip_pos.y = panel_rect.min.y + 6.0;
            }
            egui::Area::new(egui::Id::new(("conn_hover", from_id, to_id)))
                .order(egui::Order::Middle)
                .fixed_pos(tooltip_pos)
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_min_width(0.0);
                        ui.set_max_width(180.0);
                        ui.horizontal(|ui| {
                            let (id_rect, _) = ui
                                .allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                            ui.painter().circle_filled(
                                id_rect.center(),
                                8.0,
                                egui::Color32::from_gray(60),
                            );
                            ui.painter().text(
                                id_rect.center(),
                                egui::Align2::CENTER_CENTER,
                                conn_index.to_string(),
                                egui::FontId::proportional(11.0),
                                ui.visuals().text_color(),
                            );
                            ui.label(RichText::new("Connection").strong());
                        });
                        ui.separator();
                        let input_label = if inputs_len == 1 { "Input:" } else { "Inputs:" };
                        let output_label = if outputs_len == 1 {
                            "Output:"
                        } else {
                            "Outputs:"
                        };
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(output_label).color(out_color));
                            ui.label(outputs);
                        });
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(input_label).color(in_color));
                            ui.label(inputs);
                        });
                    });
                });
        }
    }

    pub(crate) fn render_connection_context_menu(&mut self, ctx: &egui::Context) {
        if !self.connections_view_enabled {
            self.connection_context_menu = None;
            return;
        }
        let Some((connections, pos, opened_frame)) = self.connection_context_menu.clone() else {
            return;
        };

        let mut close_menu = false;
        let menu_response = egui::Area::new(egui::Id::new("connection_context_menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    let row_height = ui.text_style_height(&egui::TextStyle::Button) + 6.0;
                    let menu_width = 160.0;
                    let remove_clicked = ui
                        .allocate_ui_with_layout(
                            egui::vec2(menu_width, row_height),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.add(egui::SelectableLabel::new(false, "Remove connection"))
                                    .clicked()
                            },
                        )
                        .inner;
                    if remove_clicked {
                        let mut remove_direct: Vec<ConnectionDefinition> = Vec::new();
                        let mut remove_inputs: HashMap<u64, Vec<usize>> = HashMap::new();
                        for conn in &connections {
                            let extendable = self
                                .workspace_manager
                                .workspace
                                .plugins
                                .iter()
                                .find(|p| p.id == conn.to_plugin)
                                .map(|p| self.is_extendable_inputs(&p.kind))
                                .unwrap_or(false);
                            if extendable {
                                if let Some(idx) = Self::extendable_input_index(&conn.to_port) {
                                    remove_inputs.entry(conn.to_plugin).or_default().push(idx);
                                    continue;
                                }
                            }
                            remove_direct.push(conn.clone());
                        }

                        for (plugin_id, mut inputs) in remove_inputs {
                            inputs.sort_unstable_by(|a, b| b.cmp(a));
                            inputs.dedup();
                            for idx in inputs {
                                self.remove_extendable_input_at(plugin_id, idx);
                            }
                        }

                        if !remove_direct.is_empty() {
                            let matches =
                                |left: &ConnectionDefinition, right: &ConnectionDefinition| {
                                    left.from_plugin == right.from_plugin
                                        && left.to_plugin == right.to_plugin
                                        && left.from_port == right.from_port
                                        && left.to_port == right.to_port
                                        && left.kind == right.kind
                                };
                            self.workspace_manager.workspace.connections.retain(|conn| {
                                !remove_direct.iter().any(|remove| matches(conn, remove))
                            });
                            self.workspace_manager.workspace_dirty = true;
                            self.enforce_connection_dependent();
                        }
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

        if close_menu || self.confirm_dialog.open {
            self.connection_context_menu = None;
        }
    }
}
