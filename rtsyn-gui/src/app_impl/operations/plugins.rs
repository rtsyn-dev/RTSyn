use crate::GuiApp;
use crate::HighlightMode;
use crate::state::PluginOrderMode;
use crate::ViewMode;
use rtsyn_core::plugin::PluginMetadataSource;
use rtsyn_runtime::LogicMessage;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

/// GUI implementation of plugin metadata source that communicates with the runtime logic thread.
///
/// This struct provides a bridge between the plugin manager and the runtime system,
/// allowing the GUI to query plugin metadata and behavior information through
/// message passing to the logic thread.
struct GuiMetadataSource<'a> {
    /// Channel sender for communicating with the runtime logic thread
    logic_tx: &'a mpsc::Sender<LogicMessage>,
}

impl PluginMetadataSource for GuiMetadataSource<'_> {
    /// Queries plugin metadata from the runtime logic thread.
    ///
    /// # Parameters
    /// - `library_path`: Path to the plugin library file
    /// - `timeout`: Maximum time to wait for response
    ///
    /// # Returns
    /// Optional tuple containing:
    /// - Input port names
    /// - Output port names  
    /// - Variable definitions (name, default value)
    /// - Display schema for UI rendering
    /// - UI schema for configuration
    ///
    /// # Side Effects
    /// Sends message to logic thread and blocks waiting for response
    fn query_plugin_metadata(
        &self,
        library_path: &str,
        timeout: Duration,
    ) -> Option<(
        Vec<String>,
        Vec<String>,
        Vec<(String, f64)>,
        Option<rtsyn_plugin::ui::DisplaySchema>,
        Option<rtsyn_plugin::ui::UISchema>,
    )> {
        let (tx, rx) = mpsc::channel();
        let _ = self.logic_tx.send(LogicMessage::QueryPluginMetadata(
            library_path.to_string(),
            tx,
        ));
        rx.recv_timeout(timeout).ok().flatten()
    }

    /// Queries plugin behavior information from the runtime.
    ///
    /// # Parameters
    /// - `kind`: Plugin type identifier
    /// - `library_path`: Optional path to plugin library
    /// - `timeout`: Maximum time to wait for response
    ///
    /// # Returns
    /// Optional plugin behavior configuration
    ///
    /// # Side Effects
    /// Sends message to logic thread and blocks waiting for response
    fn query_plugin_behavior(
        &self,
        kind: &str,
        library_path: Option<&str>,
        timeout: Duration,
    ) -> Option<rtsyn_plugin::ui::PluginBehavior> {
        let (tx, rx) = mpsc::channel();
        let _ = self.logic_tx.send(LogicMessage::QueryPluginBehavior(
            kind.to_string(),
            library_path.map(|s| s.to_string()),
            tx,
        ));
        rx.recv_timeout(timeout).ok().flatten()
    }
}

impl GuiApp {
    pub(crate) const PLUGIN_CARD_WIDTH: f32 = 280.0;
    pub(crate) const PLUGIN_CARD_FIXED_HEIGHT: f32 = 132.0;
    pub(crate) const PLUGIN_CARD_GRID_X_GAP: f32 = 20.0;
    pub(crate) const PLUGIN_CARD_GRID_Y_GAP: f32 = 48.0;
    pub(crate) const PLUGIN_LAYOUT_OFFSET_X: f32 = 12.0;
    pub(crate) const PLUGIN_LAYOUT_OFFSET_Y: f32 = 12.0;

    pub(crate) fn plugin_layout_grid_for_view(view_mode: ViewMode) -> (f32, f32, f32, f32) {
        match view_mode {
            // Keep spacing aligned with card dimensions and avoid overlap.
            ViewMode::Cards => (
                Self::PLUGIN_CARD_WIDTH + Self::PLUGIN_CARD_GRID_X_GAP,
                Self::PLUGIN_CARD_FIXED_HEIGHT + Self::PLUGIN_CARD_GRID_Y_GAP,
                Self::PLUGIN_LAYOUT_OFFSET_X,
                Self::PLUGIN_LAYOUT_OFFSET_Y,
            ),
            ViewMode::State => (100.0, 100.0, 50.0, 50.0),
        }
    }

    pub(crate) fn order_plugins_layout(
        &mut self,
        panel_rect: eframe::egui::Rect,
        mode: PluginOrderMode,
    ) {
        let mut plugin_ids: Vec<u64> = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .filter(|plugin| {
                !self
                    .behavior_manager
                    .cached_behaviors
                    .get(&plugin.kind)
                    .map(|b| b.external_window)
                    .unwrap_or(false)
            })
            .map(|plugin| plugin.id)
            .collect();

        let name_by_kind = self.get_name_by_kind();
        let by_id: HashMap<u64, (String, i32)> = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .map(|plugin| {
                let name = name_by_kind
                    .get(&plugin.kind)
                    .cloned()
                    .unwrap_or_else(|| Self::display_kind(&plugin.kind));
                (plugin.id, (name, plugin.priority))
            })
            .collect();
        let mut connection_counts: HashMap<u64, usize> = HashMap::new();
        for conn in &self.workspace_manager.workspace.connections {
            *connection_counts.entry(conn.from_plugin).or_insert(0) += 1;
            *connection_counts.entry(conn.to_plugin).or_insert(0) += 1;
        }

        plugin_ids.sort_by(|left, right| match mode {
            PluginOrderMode::Name => {
                let left_name = by_id
                    .get(left)
                    .map(|v| v.0.as_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                let right_name = by_id
                    .get(right)
                    .map(|v| v.0.as_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                left_name.cmp(&right_name).then_with(|| left.cmp(right))
            }
            PluginOrderMode::Id => left.cmp(right),
            PluginOrderMode::Priority => {
                let left_priority = by_id.get(left).map(|v| v.1).unwrap_or(0);
                let right_priority = by_id.get(right).map(|v| v.1).unwrap_or(0);
                left_priority.cmp(&right_priority).then_with(|| left.cmp(right))
            }
            PluginOrderMode::Connections => {
                let left_connections = connection_counts.get(left).copied().unwrap_or(0);
                let right_connections = connection_counts.get(right).copied().unwrap_or(0);
                right_connections
                    .cmp(&left_connections)
                    .then_with(|| left.cmp(right))
            }
        });

        let (x_step, y_step, x_offset, y_offset) = Self::plugin_layout_grid_for_view(self.view_mode);
        let usable_width = (panel_rect.width() - x_offset * 2.0).max(x_step);
        let cols = ((usable_width / x_step).floor() as usize).max(1);

        for (idx, plugin_id) in plugin_ids.iter().enumerate() {
            let col = idx % cols;
            let row = idx / cols;
            let pos = panel_rect.min
                + eframe::egui::vec2(
                    x_offset + (col as f32 * x_step),
                    y_offset + (row as f32 * y_step),
                );
            match self.view_mode {
                ViewMode::Cards => {
                    self.plugin_positions.insert(*plugin_id, pos);
                }
                ViewMode::State => {
                    self.state_plugin_positions.insert(*plugin_id, pos);
                }
            }
        }
    }

    /// Drains plugin compatibility warnings and shows them as notifications.
    ///
    /// # Side Effects
    /// - Retrieves warnings from plugin manager
    /// - Adds new warnings to seen warnings set
    /// - Shows info notifications for unseen warnings
    fn drain_plugin_compatibility_warnings_to_notifications(&mut self) {
        for warning in self.plugin_manager.take_compatibility_warnings() {
            if self.seen_compatibility_warnings.insert(warning.clone()) {
                self.show_info("Plugin Compatibility", &warning);
            }
        }
    }

    /// Scans for detected plugins in standard directories.
    ///
    /// # Side Effects
    /// - Scans "plugins" and "app_plugins" directories
    /// - Updates plugin manager's detected plugins list
    /// - Shows compatibility warnings as notifications
    pub(crate) fn scan_detected_plugins(&mut self) {
        self.plugin_manager
            .scan_detected_plugins_in(&["plugins", "app_plugins"]);
        self.drain_plugin_compatibility_warnings_to_notifications();
    }

    /// Adds an installed plugin to the current workspace.
    ///
    /// # Parameters
    /// - `installed_index`: Index of the plugin in the installed plugins list
    ///
    /// # Side Effects
    /// - Validates plugin exists at given index
    /// - Caches plugin behavior information
    /// - Adds plugin to workspace using metadata source
    /// - Opens plotter viewport if plugin uses plotting
    /// - Updates status message
    /// - Marks workspace as dirty
    pub(crate) fn add_installed_plugin(&mut self, installed_index: usize) {
        let installed = match self.plugin_manager.installed_plugins.get(installed_index) {
            Some(plugin) => plugin.clone(),
            None => {
                self.status = "Invalid installed plugin".to_string();
                return;
            }
        };
        self.ensure_plugin_behavior_cached_with_path(
            &installed.manifest.kind,
            installed.library_path.as_ref(),
        );
        let metadata = GuiMetadataSource {
            logic_tx: &self.state_sync.logic_tx,
        };
        if let Err(err) = self.plugin_manager.add_installed_plugin_to_workspace(
            installed_index,
            &mut self.workspace_manager.workspace,
            &metadata,
        ) {
            self.status = err;
            return;
        }
        if let Some(added) = self.workspace_manager.workspace.plugins.last() {
            if self.plugin_uses_plotter_viewport(&added.kind) {
                let plotter = self
                    .plotter_manager
                    .plotters
                    .entry(added.id)
                    .or_insert_with(|| {
                        std::sync::Arc::new(std::sync::Mutex::new(
                            crate::plotter::LivePlotter::new(added.id),
                        ))
                    });
                if let Ok(mut plotter) = plotter.lock() {
                    plotter.open = true;
                }
                self.recompute_plotter_ui_hz();
            }
        }
        self.status = "Installed plugin added".to_string();
        self.mark_workspace_dirty();
    }

    /// Creates a duplicate of an existing plugin in the workspace.
    ///
    /// # Parameters
    /// - `plugin_id`: Unique identifier of the plugin to duplicate
    ///
    /// # Side Effects
    /// - Creates new plugin instance with unique ID
    /// - Caches behavior information for duplicated plugin
    /// - Updates status message
    /// - Marks workspace as dirty
    /// - Shows error notification if plugin ID is invalid
    pub(crate) fn duplicate_plugin(&mut self, plugin_id: u64) {
        if let Some(source) = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .find(|p| p.id == plugin_id)
        {
            let kind = source.kind.clone();
            let library_path = source
                .config
                .get("library_path")
                .and_then(|v| v.as_str())
                .map(std::path::PathBuf::from);
            self.ensure_plugin_behavior_cached_with_path(&kind, library_path.as_ref());
        }
        match self
            .plugin_manager
            .duplicate_plugin_in_workspace(&mut self.workspace_manager.workspace, plugin_id)
        {
            Ok(_) => {}
            Err(_) => {
                self.show_info("Plugin", "Invalid plugin");
                return;
            }
        };
        self.status = "Plugin duplicated".to_string();
        self.mark_workspace_dirty();
    }

    /// Starts every plugin in the current workspace.
    pub(crate) fn start_all_plugins(&mut self) {
        let mut changed = false;
        for plugin in &mut self.workspace_manager.workspace.plugins {
            if !plugin.running {
                plugin.running = true;
                changed = true;
            }
        }
        if changed {
            let _ = self
                .state_sync
                .logic_tx
                .send(LogicMessage::SetAllPluginsRunning(true));
            self.open_running_plotters();
            self.mark_workspace_dirty();
        }
    }

    /// Stops every running plugin in the current workspace.
    pub(crate) fn stop_all_plugins(&mut self) {
        let mut changed = false;
        for plugin in &mut self.workspace_manager.workspace.plugins {
            if plugin.running {
                plugin.running = false;
                changed = true;
            }
        }
        if changed {
            let _ = self
                .state_sync
                .logic_tx
                .send(LogicMessage::SetAllPluginsRunning(false));
            for plotter in self.plotter_manager.plotters.values() {
                if let Ok(mut plotter) = plotter.lock() {
                    plotter.open = false;
                }
            }
            self.recompute_plotter_ui_hz();
            self.mark_workspace_dirty();
        }
    }

    /// Removes a plugin from the workspace by index.
    ///
    /// # Parameters
    /// - `plugin_index`: Index of the plugin in the workspace plugins list
    ///
    /// # Side Effects
    /// - Validates plugin index bounds
    /// - Clears selection if removed plugin was selected
    /// - Closes configuration window if open for removed plugin
    /// - Removes associated plotter data
    /// - Updates extendable input counts for remaining plugins
    /// - Recomputes plotter UI refresh rate
    /// - Enforces connection dependencies
    /// - Updates status message
    /// - Marks workspace as dirty
    pub(crate) fn remove_plugin(&mut self, plugin_index: usize) {
        if plugin_index >= self.workspace_manager.workspace.plugins.len() {
            self.status = "Invalid plugin selection".to_string();
            return;
        }

        let removed_id = self.workspace_manager.workspace.plugins[plugin_index].id;

        // Clear highlight if removed plugin was highlighted
        if matches!(self.highlight_mode, HighlightMode::AllConnections(id) if id == removed_id) {
            self.highlight_mode = HighlightMode::None;
        }
        if let HighlightMode::SingleConnection(from, to) = self.highlight_mode {
            if from == removed_id || to == removed_id {
                self.highlight_mode = HighlightMode::None;
            }
        }
        if self.windows.plugin_config_id == Some(removed_id) {
            self.windows.plugin_config_id = None;
            self.windows.plugin_config_open = false;
        }
        self.plotter_manager.plotters.remove(&removed_id);
        self.plotter_manager
            .plotter_preview_settings
            .remove(&removed_id);
        self.plugin_positions.remove(&removed_id);
        self.state_plugin_positions.remove(&removed_id);
        self.plugin_rects.remove(&removed_id);

        if let Err(err) = self
            .plugin_manager
            .remove_plugin_from_workspace(&mut self.workspace_manager.workspace, removed_id)
        {
            self.status = err;
            return;
        }
        let ids: Vec<u64> = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .map(|p| p.id)
            .collect();
        for id in ids {
            self.sync_extendable_input_count(id);
        }
        self.recompute_plotter_ui_hz();
        self.enforce_connection_dependent();
        self.status = "Plugin removed".to_string();
        self.mark_workspace_dirty();
    }

    pub(crate) fn remove_plugin_by_id(&mut self, plugin_id: u64) {
        if let Some(index) = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .position(|p| p.id == plugin_id)
        {
            self.remove_plugin(index);
        }
    }

    /// Uninstalls a plugin and removes all instances from workspace.
    ///
    /// # Parameters
    /// - `installed_index`: Index of the plugin in the installed plugins list
    ///
    /// # Side Effects
    /// - Uninstalls plugin from system
    /// - Removes all workspace instances of the plugin type
    /// - Clears UI state for removed plugins (selection, config windows, plotters)
    /// - Rescans for detected plugins
    /// - Shows success/error notifications
    pub(crate) fn uninstall_plugin(&mut self, installed_index: usize) {
        let plugin = match self
            .plugin_manager
            .uninstall_plugin_by_index(installed_index)
        {
            Ok(plugin) => plugin,
            Err(err) => {
                self.show_info("Plugin", &err);
                return;
            }
        };
        self.invalidate_display_schema_cache();

        let removed_ids = self.plugin_manager.remove_plugins_by_kind_from_workspace(
            &mut self.workspace_manager.workspace,
            &plugin.manifest.kind,
        );

        for id in &removed_ids {
            // Clear highlight if removed plugin was highlighted
            if matches!(self.highlight_mode, HighlightMode::AllConnections(hid) if hid == *id) {
                self.highlight_mode = HighlightMode::None;
            }
            if let HighlightMode::SingleConnection(from, to) = self.highlight_mode {
                if from == *id || to == *id {
                    self.highlight_mode = HighlightMode::None;
                }
            }
            if self.windows.plugin_config_id == Some(*id) {
                self.windows.plugin_config_id = None;
                self.windows.plugin_config_open = false;
            }
            self.plotter_manager.plotters.remove(id);
            self.plotter_manager.plotter_preview_settings.remove(id);
            self.plugin_positions.remove(id);
            self.state_plugin_positions.remove(id);
            self.plugin_rects.remove(id);
        }

        self.scan_detected_plugins();
        self.invalidate_name_cache();
        self.show_info("Plugin", "Plugin uninstalled");
    }

    /// Installs a plugin from a folder path.
    ///
    /// # Parameters
    /// - `folder`: Path to the plugin folder
    /// - `removable`: Whether the plugin can be uninstalled
    /// - `persist`: Whether to persist the installation
    ///
    /// # Side Effects
    /// - Installs plugin using metadata source for validation
    /// - Updates status message
    /// - Shows error notifications on failure
    /// - Drains compatibility warnings to notifications
    pub(crate) fn install_plugin_from_folder<P: AsRef<Path>>(
        &mut self,
        folder: P,
        removable: bool,
        persist: bool,
    ) {
        let metadata = GuiMetadataSource {
            logic_tx: &self.state_sync.logic_tx,
        };
        if let Err(err) = self.plugin_manager.install_plugin_from_folder(
            folder.as_ref(),
            removable,
            persist,
            &metadata,
        ) {
            self.status = err;
            self.show_info("Plugin Install Error", &self.status.clone());
            return;
        }
        self.invalidate_display_schema_cache();
        self.status = "Plugin installed".to_string();
        self.drain_plugin_compatibility_warnings_to_notifications();
    }

    /// Refreshes an installed plugin with updated code from path.
    ///
    /// # Parameters
    /// - `kind`: Plugin type identifier
    /// - `path`: Path to the updated plugin files
    ///
    /// # Side Effects
    /// - Removes UI state for existing plugin instances
    /// - Refreshes plugin installation if path is not empty
    /// - Updates status message
    /// - Shows error notifications on failure
    /// - Drains compatibility warnings to notifications
    pub(crate) fn refresh_installed_plugin(&mut self, kind: String, path: &Path) {
        let plugin_ids: Vec<u64> = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .filter(|p| p.kind == kind)
            .map(|p| p.id)
            .collect();

        for id in &plugin_ids {
            // Clear highlight if refreshed plugin was highlighted
            if matches!(self.highlight_mode, HighlightMode::AllConnections(hid) if hid == *id) {
                self.highlight_mode = HighlightMode::None;
            }
            if let HighlightMode::SingleConnection(from, to) = self.highlight_mode {
                if from == *id || to == *id {
                    self.highlight_mode = HighlightMode::None;
                }
            }
            if self.windows.plugin_config_id == Some(*id) {
                self.windows.plugin_config_id = None;
                self.windows.plugin_config_open = false;
            }
            self.plotter_manager.plotters.remove(id);
            self.plotter_manager.plotter_preview_settings.remove(id);
            self.plugin_positions.remove(id);
            self.state_plugin_positions.remove(id);
            self.plugin_rects.remove(id);
        }

        if path.as_os_str().is_empty() {
            self.status = "Plugin refreshed".to_string();
            return;
        }
        let metadata = GuiMetadataSource {
            logic_tx: &self.state_sync.logic_tx,
        };
        if let Err(err) = self
            .plugin_manager
            .refresh_installed_plugin(&kind, path, &metadata)
        {
            self.status = err;
            self.show_info("Plugin Refresh Error", &self.status.clone());
            return;
        }
        self.invalidate_display_schema_cache();
        self.status = "Plugin refreshed".to_string();
        self.invalidate_name_cache();
        self.drain_plugin_compatibility_warnings_to_notifications();
    }

    /// Refreshes library paths for all installed plugins.
    ///
    /// # Side Effects
    /// Updates the library paths in the plugin manager's installed plugins list
    pub(crate) fn refresh_installed_library_paths(&mut self) {
        self.plugin_manager.refresh_installed_library_paths();
    }

    /// Injects current library paths into workspace plugin definitions.
    ///
    /// # Side Effects
    /// Updates library_path field for all plugins in the current workspace
    pub(crate) fn inject_library_paths_into_workspace(&mut self) {
        self.plugin_manager
            .inject_library_paths_into_workspace(&mut self.workspace_manager.workspace);
    }

    /// Loads all installed plugins from the plugin directory.
    ///
    /// # Side Effects
    /// - Scans and loads plugin manifests and metadata
    /// - Drains compatibility warnings to notifications
    pub(crate) fn load_installed_plugins(&mut self) {
        self.plugin_manager.load_installed_plugins();
        self.invalidate_display_schema_cache();
        self.drain_plugin_compatibility_warnings_to_notifications();
    }

    /// Refreshes metadata cache for installed plugins with incomplete metadata.
    ///
    /// # Side Effects
    /// - Identifies plugins with missing metadata (inputs, outputs, variables, schemas)
    /// - Queries runtime for updated metadata using metadata source
    /// - Updates plugin manager's cached metadata
    pub(crate) fn refresh_installed_plugin_metadata_cache(&mut self) {
        let targets: Vec<(String, PathBuf)> = self
            .plugin_manager
            .installed_plugins
            .iter()
            .filter(|plugin| {
                if plugin.path.as_os_str().is_empty() {
                    return false;
                }
                plugin.metadata_inputs.is_empty()
                    || plugin.metadata_outputs.is_empty()
                    || plugin.metadata_variables.is_empty()
                    || plugin.display_schema.is_none()
            })
            .map(|plugin| (plugin.manifest.kind.clone(), plugin.path.clone()))
            .collect();
        if targets.is_empty() {
            return;
        }

        let metadata = GuiMetadataSource {
            logic_tx: &self.state_sync.logic_tx,
        };
        for (kind, path) in targets {
            let _ = self
                .plugin_manager
                .refresh_installed_plugin(&kind, &path, &metadata);
        }
    }
}
