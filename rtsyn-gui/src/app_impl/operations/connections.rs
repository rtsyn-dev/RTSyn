use crate::GuiApp;
use rtsyn_core::connection as core_connections;
use rtsyn_runtime::LogicMessage;
use serde_json::Value;
use std::collections::HashSet;
use workspace::{remove_extendable_input, ConnectionDefinition, ConnectionRuleError};

impl GuiApp {
    /// Adds a connection between plugins using data from the connection editor.
    ///
    /// # Side Effects
    /// - Validates plugins are different (no self-connections)
    /// - Validates at least two plugins exist in workspace
    /// - Retrieves plugin IDs from workspace using editor indices
    /// - Validates connection fields are not empty
    /// - Adds connection using core connection logic
    /// - Shows error notifications for validation failures
    /// - Updates status message on success
    /// - Enforces connection dependencies
    /// - Marks workspace as dirty
    pub(crate) fn add_connection(&mut self) {
        if self.connection_editor.from_idx == self.connection_editor.to_idx {
            self.show_info("Connections", "Cannot connect a plugin to itself");
            return;
        }
        if self.workspace_manager.workspace.plugins.len() < 2 {
            self.status = "Add at least two plugins before connecting".to_string();
            return;
        }

        let from_plugin = match self
            .workspace_manager
            .workspace
            .plugins
            .get(self.connection_editor.from_idx)
        {
            Some(plugin) => plugin.id,
            None => {
                self.status = "Invalid source plugin".to_string();
                return;
            }
        };

        let to_plugin = match self
            .workspace_manager
            .workspace
            .plugins
            .get(self.connection_editor.to_idx)
        {
            Some(plugin) => plugin.id,
            None => {
                self.status = "Invalid target plugin".to_string();
                return;
            }
        };

        let from_port = self.connection_editor.from_port.trim();
        let to_port = self.connection_editor.to_port.trim();
        let kind = self.connection_editor.kind.trim();

        if from_port.is_empty() || to_port.is_empty() || kind.is_empty() {
            self.status = "Connection fields cannot be empty".to_string();
            return;
        }
        if let Err(err) = core_connections::add_connection(
            &mut self.workspace_manager.workspace,
            &self.plugin_manager.installed_plugins,
            from_plugin,
            from_port,
            to_plugin,
            to_port,
            kind,
        ) {
            let message = match err {
                ConnectionRuleError::SelfConnection => "Cannot connect a plugin to itself.",
                ConnectionRuleError::InputLimitExceeded => "Input already has a connection.",
                ConnectionRuleError::DuplicateConnection => {
                    "Connection between these plugins already exists."
                }
            };
            self.show_info("Connections", message);
            return;
        }
        self.status = "Connection added".to_string();
        self.enforce_connection_dependent();
        self.mark_workspace_dirty();
    }

    /// Adds a connection directly between specified plugins.
    ///
    /// # Parameters
    /// - `from_plugin`: Source plugin ID
    /// - `from_port`: Source port name
    /// - `to_plugin`: Target plugin ID  
    /// - `to_port`: Target port name
    /// - `kind`: Connection type/kind
    ///
    /// # Side Effects
    /// - Validates plugins are different (no self-connections)
    /// - Validates connection fields are not empty
    /// - Adds connection using core connection logic
    /// - Shows error notifications for validation failures
    /// - Marks workspace as dirty
    /// - Enforces connection dependencies
    pub(crate) fn add_connection_direct(
        &mut self,
        from_plugin: u64,
        from_port: String,
        to_plugin: u64,
        to_port: String,
        kind: String,
    ) {
        if from_plugin == to_plugin {
            self.show_info("Connections", "Cannot connect a plugin to itself");
            return;
        }
        if from_port.trim().is_empty() || to_port.trim().is_empty() || kind.trim().is_empty() {
            self.show_info("Connections", "Connection fields cannot be empty");
            return;
        }
        if let Err(err) = core_connections::add_connection(
            &mut self.workspace_manager.workspace,
            &self.plugin_manager.installed_plugins,
            from_plugin,
            &from_port,
            to_plugin,
            &to_port,
            &kind,
        ) {
            let message = match err {
                ConnectionRuleError::SelfConnection => "Cannot connect a plugin to itself.",
                ConnectionRuleError::InputLimitExceeded => "Input already has a connection.",
                ConnectionRuleError::DuplicateConnection => {
                    "Connection between these plugins already exists."
                }
            };
            self.show_info("Connections", message);
            return;
        }
        self.mark_workspace_dirty();
        self.enforce_connection_dependent();
    }

    /// Removes a connection and handles extendable input reindexing.
    ///
    /// # Parameters
    /// - `connection`: Connection definition to remove
    ///
    /// # Side Effects
    /// - Checks if target port is an extendable input
    /// - For extendable inputs on supported plugins:
    ///   - Removes matching connections from workspace
    ///   - Reindexes remaining extendable inputs
    ///   - Recomputes plotter UI refresh rate if needed
    /// - For regular connections:
    ///   - Removes matching connections from workspace
    /// - Marks workspace as dirty
    /// - Enforces connection dependencies
    pub(crate) fn remove_connection_with_input(&mut self, connection: ConnectionDefinition) {
        if Self::extendable_input_index(&connection.to_port).is_some() {
            let target_kind = self
                .workspace_manager
                .workspace
                .plugins
                .iter()
                .find(|p| p.id == connection.to_plugin)
                .map(|p| p.kind.clone());
            if let Some(kind) = target_kind {
                if self.is_extendable_inputs(&kind) {
                    let matches = |left: &ConnectionDefinition, right: &ConnectionDefinition| {
                        left.from_plugin == right.from_plugin
                            && left.to_plugin == right.to_plugin
                            && left.from_port == right.from_port
                            && left.to_port == right.to_port
                            && left.kind == right.kind
                    };
                    self.workspace_manager
                        .workspace
                        .connections
                        .retain(|conn| !matches(conn, &connection));
                    self.reindex_extendable_inputs(connection.to_plugin);
                    self.mark_workspace_dirty();
                    self.enforce_connection_dependent();
                    if self.plugin_uses_plotter_viewport(&kind) {
                        self.recompute_plotter_ui_hz();
                    }
                    return;
                }
            }
        }
        let matches = |left: &ConnectionDefinition, right: &ConnectionDefinition| {
            left.from_plugin == right.from_plugin
                && left.to_plugin == right.to_plugin
                && left.from_port == right.from_port
                && left.to_port == right.to_port
                && left.kind == right.kind
        };
        self.workspace_manager
            .workspace
            .connections
            .retain(|conn| !matches(conn, &connection));
        self.mark_workspace_dirty();
        self.enforce_connection_dependent();
    }

    /// Enforces connection dependencies by stopping plugins without inputs.
    ///
    /// Stops specific plugin types (csv_recorder, live_plotter, comedi_daq) that
    /// are running but have no incoming connections, as they require input data.
    ///
    /// # Side Effects
    /// - Identifies plugins with incoming connections
    /// - Stops running plugins of specific types without incoming connections
    /// - Sends stop messages to runtime logic thread
    /// - Updates plugin running state in workspace
    pub(crate) fn enforce_connection_dependent(&mut self) {
        let mut stopped = Vec::new();

        let incoming: HashSet<u64> = self
            .workspace_manager
            .workspace
            .connections
            .iter()
            .map(|conn| conn.to_plugin)
            .collect();
        for plugin in &mut self.workspace_manager.workspace.plugins {
            if !matches!(
                plugin.kind.as_str(),
                "csv_recorder" | "live_plotter" | "comedi_daq"
            ) {
                continue;
            }
            if incoming.contains(&plugin.id) {
                continue;
            }
            if plugin.running {
                plugin.running = false;
                stopped.push(plugin.id);
            }
        }
        for id in stopped {
            let _ = self
                .state_sync
                .logic_tx
                .send(LogicMessage::SetPluginRunning(id, false));
        }
    }

    /// Extracts the numeric index from an extendable input port name.
    ///
    /// # Parameters
    /// - `port`: Port name (e.g., "in_0", "in_1")
    ///
    /// # Returns
    /// Optional index if port follows extendable input naming pattern
    pub(crate) fn extendable_input_index(port: &str) -> Option<usize> {
        core_connections::extendable_input_index(port)
    }

    /// Finds the next available index for extendable inputs on a plugin.
    ///
    /// # Parameters
    /// - `plugin_id`: Target plugin ID
    ///
    /// # Returns
    /// Next available index for creating new extendable input
    pub(crate) fn next_available_extendable_input_index(&self, plugin_id: u64) -> usize {
        core_connections::next_available_extendable_input_index(
            &self.workspace_manager.workspace,
            plugin_id,
        )
    }

    /// Gets display names for extendable input ports on a plugin.
    ///
    /// # Parameters
    /// - `plugin_id`: Target plugin ID
    /// - `include_placeholder`: Whether to include a placeholder for the next available input
    ///
    /// # Returns
    /// Vector of port names sorted by index, optionally with placeholder
    ///
    /// # Side Effects
    /// - Collects existing extendable input connections
    /// - Sorts and deduplicates by index
    /// - Adds placeholder port name if requested and appropriate
    pub(crate) fn extendable_input_display_ports(
        &self,
        plugin_id: u64,
        include_placeholder: bool,
    ) -> Vec<String> {
        let mut entries: Vec<(usize, String)> = self
            .workspace_manager
            .workspace
            .connections
            .iter()
            .filter(|conn| conn.to_plugin == plugin_id)
            .filter_map(|conn| {
                Self::extendable_input_index(&conn.to_port).map(|idx| (idx, conn.to_port.clone()))
            })
            .collect();
        entries.sort_by_key(|(idx, _)| *idx);
        entries.dedup_by(|a, b| a.0 == b.0);
        let mut list: Vec<String> = entries.into_iter().map(|(_, port)| port).collect();
        if include_placeholder {
            if list.is_empty() {
                list.push("in_0".to_string());
            } else {
                let next_idx = self.next_available_extendable_input_index(plugin_id);
                let next_name = format!("in_{next_idx}");
                if !list.contains(&next_name) {
                    list.push(next_name);
                }
            }
        }
        list
    }

    /// Removes an extendable input at a specific index and reindexes remaining inputs.
    ///
    /// # Parameters
    /// - `plugin_id`: Target plugin ID
    /// - `remove_idx`: Index of the extendable input to remove
    ///
    /// # Side Effects
    /// - Validates plugin exists and supports extendable inputs
    /// - Calculates current input count from connections and config
    /// - Removes connections to the specified input index
    /// - Updates plugin configuration with new input count
    /// - For CSV recorder plugins: updates column configuration
    /// - Marks workspace as dirty
    /// - Enforces connection dependencies
    /// - Recomputes plotter UI refresh rate if needed
    pub(crate) fn remove_extendable_input_at(&mut self, plugin_id: u64, remove_idx: usize) {
        let plugin_index = match self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .position(|p| p.id == plugin_id)
        {
            Some(idx) => idx,
            None => return,
        };
        let kind = self.workspace_manager.workspace.plugins[plugin_index]
            .kind
            .clone();
        if !self.is_extendable_inputs(&kind) {
            return;
        }

        let (current_count, mut columns, is_csv) = {
            let plugin = &self.workspace_manager.workspace.plugins[plugin_index];
            let mut input_count = plugin
                .config
                .get("input_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let mut columns = Vec::new();
            let is_csv = plugin.kind == "csv_recorder";
            if is_csv {
                columns = plugin
                    .config
                    .get("columns")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .map(|v| v.as_str().unwrap_or("").to_string())
                            .collect()
                    })
                    .unwrap_or_default();
                if columns.len() > input_count {
                    input_count = columns.len();
                }
            }
            let mut max_idx: Option<usize> = None;
            for conn in &self.workspace_manager.workspace.connections {
                if conn.to_plugin != plugin_id {
                    continue;
                }
                if let Some(idx) = Self::extendable_input_index(&conn.to_port) {
                    max_idx = Some(max_idx.map(|v| v.max(idx)).unwrap_or(idx));
                }
            }
            if let Some(idx) = max_idx {
                input_count = input_count.max(idx + 1);
            }
            (input_count, columns, is_csv)
        };

        if remove_idx >= current_count {
            return;
        }

        remove_extendable_input(
            &mut self.workspace_manager.workspace.connections,
            plugin_id,
            remove_idx,
        );
        let new_count = current_count.saturating_sub(1);

        let map = match self.workspace_manager.workspace.plugins[plugin_index].config {
            Value::Object(ref mut map) => map,
            _ => {
                self.workspace_manager.workspace.plugins[plugin_index].config =
                    Value::Object(serde_json::Map::new());
                match self.workspace_manager.workspace.plugins[plugin_index].config {
                    Value::Object(ref mut map) => map,
                    _ => return,
                }
            }
        };
        map.insert("input_count".to_string(), Value::from(new_count as u64));
        if is_csv {
            if remove_idx < columns.len() {
                columns.remove(remove_idx);
            }
            if columns.len() > new_count {
                columns.truncate(new_count);
            } else if columns.len() < new_count {
                columns.resize(new_count, String::new());
            }
            map.insert(
                "columns".to_string(),
                Value::Array(columns.into_iter().map(Value::from).collect()),
            );
        }

        self.mark_workspace_dirty();
        self.enforce_connection_dependent();
        if self.plugin_uses_plotter_viewport(&kind) {
            self.recompute_plotter_ui_hz();
        }
    }

    /// Reindexes extendable inputs to eliminate gaps in numbering.
    ///
    /// # Parameters
    /// - `plugin_id`: Target plugin ID
    ///
    /// # Side Effects
    /// - Validates plugin supports extendable inputs
    /// - Collects all extendable input connections for the plugin
    /// - Sorts connections by port index
    /// - Renumbers port names to eliminate gaps (in_0, in_1, in_2, etc.)
    /// - Updates plugin configuration with correct input count
    /// - For CSV recorder plugins: adjusts column configuration to match input count
    pub(crate) fn reindex_extendable_inputs(&mut self, plugin_id: u64) {
        let kind = match self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .find(|p| p.id == plugin_id)
            .map(|p| p.kind.clone())
        {
            Some(kind) => kind,
            None => return,
        };
        if !self.is_extendable_inputs(&kind) {
            return;
        }

        let mut entries: Vec<(usize, usize)> = self
            .workspace_manager
            .workspace
            .connections
            .iter()
            .enumerate()
            .filter(|(_, conn)| conn.to_plugin == plugin_id)
            .filter_map(|(idx, conn)| {
                Self::extendable_input_index(&conn.to_port).map(|port_idx| (idx, port_idx))
            })
            .collect();
        entries.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

        for (new_idx, (conn_idx, _)) in entries.iter().enumerate() {
            if let Some(conn) = self
                .workspace_manager
                .workspace
                .connections
                .get_mut(*conn_idx)
            {
                conn.to_port = format!("in_{new_idx}");
            }
        }

        let Some(plugin) = self
            .workspace_manager
            .workspace
            .plugins
            .iter_mut()
            .find(|p| p.id == plugin_id)
        else {
            return;
        };
        let map = match plugin.config {
            Value::Object(ref mut map) => map,
            _ => {
                plugin.config = Value::Object(serde_json::Map::new());
                match plugin.config {
                    Value::Object(ref mut map) => map,
                    _ => return,
                }
            }
        };
        let required_count = entries.len();
        map.insert(
            "input_count".to_string(),
            Value::from(required_count as u64),
        );

        if plugin.kind == "csv_recorder" {
            let mut columns: Vec<String> = map
                .get("columns")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|v| v.as_str().unwrap_or("").to_string())
                        .collect()
                })
                .unwrap_or_default();
            if columns.len() > required_count {
                columns.truncate(required_count);
            } else if columns.len() < required_count {
                columns.resize(required_count, String::new());
            }
            map.insert(
                "columns".to_string(),
                Value::Array(columns.into_iter().map(Value::from).collect()),
            );
        }
    }

    /// Synchronizes the extendable input count configuration with actual connections.
    ///
    /// # Parameters
    /// - `plugin_id`: Target plugin ID
    ///
    /// # Side Effects
    /// Delegates to core connection logic to update plugin configuration
    /// based on the number of extendable input connections
    pub(crate) fn sync_extendable_input_count(&mut self, plugin_id: u64) {
        core_connections::sync_extendable_input_count(
            &mut self.workspace_manager.workspace,
            plugin_id,
        );
    }
}
