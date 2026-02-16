use crate::GuiApp;
use crate::HighlightMode;
use crate::NewPluginDraft;
use crate::state;
use crate::state::{
    ConnectionEditorHost, FrequencyUnit, PeriodUnit,
    StateSync,
};
use crate::managers::{
    FileDialogManager, NotificationHandler, PlotterManager, PluginBehaviorManager,
};
use eframe::egui::{self};
use rtsyn_runtime::{LogicMessage, LogicState};
use rtsyn_core::plugin::PluginManager;
use rtsyn_core::workspace::WorkspaceManager;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};


impl GuiApp {
    /// - May trigger plugin behavior caching for installed plugins
    pub(crate) fn new_with_runtime(
            logic_tx: Sender<LogicMessage>,
            logic_state_rx: Receiver<LogicState>,
        ) -> Self {
            let install_db_path = PathBuf::from("app_plugins").join("installed_plugins.json");
            let workspace_dir = PathBuf::from("app_workspaces");

            let available_cores = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1);

            let mut plugin_manager = PluginManager::new(install_db_path);
            let mut workspace_manager = WorkspaceManager::new(workspace_dir);
            let file_dialogs = FileDialogManager::new();
            let plotter_manager = PlotterManager::new();
            let state_sync = StateSync::new(logic_tx, logic_state_rx);
            let notification_handler = NotificationHandler::new();
            let behavior_manager = PluginBehaviorManager::new();

            plugin_manager.refresh_library_paths();
            workspace_manager
                .workspace
                .plugins
                .iter_mut()
                .for_each(|p| {
                    if let Some(installed) = plugin_manager
                        .installed_plugins
                        .iter()
                        .find(|i| i.manifest.kind == p.kind)
                    {
                        if let Some(lib_path) = &installed.library_path {
                            if let Some(config) = p.config.as_object_mut() {
                                config.insert(
                                    "library_path".to_string(),
                                    serde_json::Value::String(lib_path.to_string_lossy().to_string()),
                                );
                            }
                        }
                    }
                });

            let mut app = Self {
                plugin_manager,
                workspace_manager,
                file_dialogs,
                plotter_manager,
                state_sync,
                notification_handler,
                behavior_manager,
                plotter_preview: state::PlotterPreviewState::default(),
                connection_editor: state::ConnectionEditorState::default(),
                workspace_dialog: state::WorkspaceDialogState::default(),
                build_dialog: state::BuildDialogState::default(),
                confirm_dialog: state::ConfirmDialogState::default(),
                workspace_settings: state::WorkspaceSettingsState::default(),
                help_state: state::HelpState::default(),
                windows: state::WindowState::default(),
                status: String::new(),
                csv_path_target_plugin_id: None,
                plugin_creator_last_path: None,
                new_plugin_draft: NewPluginDraft::default(),
                seen_compatibility_warnings: HashSet::new(),
                plugin_positions: HashMap::new(),
                plugin_rects: HashMap::new(),
                connections_view_enabled: true,
                connection_clicked_this_frame: false,
                available_cores,
                selected_cores: (0..available_cores).map(|i| i == 0).collect(),
                frequency_value: 1000.0,
                frequency_unit: FrequencyUnit::Hz,
                period_value: 1.0,
                period_unit: PeriodUnit::Ms,
                output_refresh_hz: 1.0,
                plotter_screenshot_target: None,
                connection_highlight_plugin_id: None,
                highlight_mode: HighlightMode::None,
                pending_highlight: None,
                plugin_context_menu: None,
                connection_context_menu: None,
                connection_editor_host: ConnectionEditorHost::Main,
                number_edit_buffers: HashMap::new(),
                window_rects: Vec::new(),
                pending_window_focus: None,
                uml_preview_texture: None,
                uml_preview_hash: None,
                uml_preview_error: None,
                uml_preview_loading: false,
                uml_preview_rx: None,
                uml_text_buffer: String::new(),
                uml_export_svg: false,
                uml_export_width: 1920,
                uml_export_height: 1080,
                uml_preview_zoom: 0.0,
            };
            for warning in app.plugin_manager.take_compatibility_warnings() {
                app.show_info("Plugin Compatibility", &warning);
                app.seen_compatibility_warnings.insert(warning);
            }
            app.refresh_installed_plugin_metadata_cache();
            app.apply_workspace_settings();
            app
        }

    /// Handles double-click on a plugin - highlights all its connections.
    /// Handles double-click on a plugin - highlights all its connections.
    /// If plugin is already highlighted, clears highlights.
    pub(crate) fn double_click_plugin(&mut self, plugin_id: u64) {
        // Check if plugin has connections
        let has_connections = self.workspace_manager.workspace.connections.iter()
            .any(|c| c.from_plugin == plugin_id || c.to_plugin == plugin_id);
        
        if !has_connections {
            // Non-connected plugin - just clear any existing highlight
            self.highlight_mode = HighlightMode::None;
            self.pending_highlight = None;
            return;
        }
        
        // Toggle off only if clicking the SAME plugin again
        if matches!(self.highlight_mode, HighlightMode::AllConnections(id) if id == plugin_id) {
            self.highlight_mode = HighlightMode::None;
            self.pending_highlight = None;
            return;
        }
        
        // If currently highlighted, clear and set pending for next frame
        if !matches!(self.highlight_mode, HighlightMode::None) {
            self.pending_highlight = Some(HighlightMode::AllConnections(plugin_id));
            self.highlight_mode = HighlightMode::None;
        } else {
            // Direct switch from None
            self.highlight_mode = HighlightMode::AllConnections(plugin_id);
        }
    }
    
    /// Handles click on a connection - highlights only the two connected plugins.
    pub(crate) fn click_connection(&mut self, from_plugin: u64, to_plugin: u64) {
        // Mark that connection was clicked this frame
        self.connection_clicked_this_frame = true;
        
        // If clicking the reverse direction of current highlight, keep it (don't switch)
        if matches!(self.highlight_mode, HighlightMode::SingleConnection(f, t) 
            if (f == from_plugin && t == to_plugin) || (f == to_plugin && t == from_plugin)) {
            return;
        }
        
        // Direct switch (same as double-click behavior)
        self.highlight_mode = HighlightMode::SingleConnection(from_plugin, to_plugin);
    }
    
    /// Checks if a connection should be highlighted.
    pub(crate) fn should_highlight_connection(&self, from_plugin: u64, to_plugin: u64) -> bool {
        match self.highlight_mode {
            HighlightMode::AllConnections(plugin_id) => {
                from_plugin == plugin_id || to_plugin == plugin_id
            }
            HighlightMode::SingleConnection(from, to) => {
                // Highlight all connections between these two plugins (bidirectional)
                (from_plugin == from && to_plugin == to) || (from_plugin == to && to_plugin == from)
            }
            HighlightMode::None => false,
        }
    }
    
    /// Gets the set of plugins that should be highlighted based on current highlight mode.
    pub(crate) fn get_highlighted_plugins(&self) -> HashSet<u64> {
        match self.highlight_mode {
            HighlightMode::AllConnections(plugin_id) => {
                let mut set = HashSet::new();
                set.insert(plugin_id);
                for conn in &self.workspace_manager.workspace.connections {
                    if conn.from_plugin == plugin_id || conn.to_plugin == plugin_id {
                        set.insert(conn.from_plugin);
                        set.insert(conn.to_plugin);
                    }
                }
                set
            }
            HighlightMode::SingleConnection(from, to) => {
                let mut set = HashSet::new();
                set.insert(from);
                set.insert(to);
                set
            }
            HighlightMode::None => HashSet::new(),
        }
    }

    /// ```
    pub(crate) fn center_window(ctx: &egui::Context, size: egui::Vec2) -> egui::Pos2 {
            let rect = ctx.available_rect();
            let center = rect.center();
            center - size * 0.5
        }

    /// - Error messages and notifications
    pub(crate) fn display_kind(kind: &str) -> String {
            PluginManager::display_kind(kind)
        }

    pub(crate) fn display_connection_kind(kind: &str) -> &str {
            match kind {
                "shared_memory" => "Shared memory",
                "pipe" => "Pipe",
                "in_process" => "In process",
                other => other,
            }
        }

    /// - **Cross-platform**: Works on Unix-like systems and Windows
    pub(crate) fn default_csv_path() -> String {
            let base = std::env::var("HOME")
                .map(|home| PathBuf::from(home).join("rtsyn-recorded"))
                .unwrap_or_else(|_| PathBuf::from("rtsyn-recorded"));
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let day = now / 86_400;
            let hour = (now % 86_400) / 3_600;
            let minute = (now % 3_600) / 60;
            let second = now % 60;
            let stamp = format!("{day}-{hour:02}-{minute:02}-{second:02}");
            base.join(format!("{stamp}.csv"))
                .to_string_lossy()
                .to_string()
        }

}