use crate::GuiApp;
use rtsyn_core::plugin::PluginMetadataSource;
use rtsyn_runtime::runtime::LogicMessage;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

struct GuiMetadataSource<'a> {
    logic_tx: &'a mpsc::Sender<LogicMessage>,
}

impl PluginMetadataSource for GuiMetadataSource<'_> {
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
    pub(crate) fn scan_detected_plugins(&mut self) {
        self.plugin_manager
            .scan_detected_plugins_in(&["plugins", "app_plugins"]);
    }

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
        self.status = "Installed plugin added".to_string();
        self.mark_workspace_dirty();
    }

    pub(crate) fn duplicate_plugin(&mut self, plugin_id: u64) {
        let new_id = match self
            .plugin_manager
            .duplicate_plugin_in_workspace(&mut self.workspace_manager.workspace, plugin_id)
        {
            Ok(id) => id,
            Err(_) => {
                self.show_info("Plugin", "Invalid plugin");
                return;
            }
        };
        if let Some(kind) = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .find(|p| p.id == new_id)
            .map(|p| p.kind.clone())
        {
            self.ensure_plugin_behavior_cached(&kind);
        }
        self.status = "Plugin duplicated".to_string();
        self.mark_workspace_dirty();
    }

    pub(crate) fn remove_plugin(&mut self, plugin_index: usize) {
        if plugin_index >= self.workspace_manager.workspace.plugins.len() {
            self.status = "Invalid plugin selection".to_string();
            return;
        }

        let removed_id = self.workspace_manager.workspace.plugins[plugin_index].id;

        if self.selected_plugin_id == Some(removed_id) {
            self.selected_plugin_id = None;
        }
        if self.windows.plugin_config_id == Some(removed_id) {
            self.windows.plugin_config_id = None;
            self.windows.plugin_config_open = false;
        }
        self.plotter_manager.plotters.remove(&removed_id);

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

        let removed_ids = self.plugin_manager.remove_plugins_by_kind_from_workspace(
            &mut self.workspace_manager.workspace,
            &plugin.manifest.kind,
        );

        for id in &removed_ids {
            if self.selected_plugin_id == Some(*id) {
                self.selected_plugin_id = None;
            }
            if self.windows.plugin_config_id == Some(*id) {
                self.windows.plugin_config_id = None;
                self.windows.plugin_config_open = false;
            }
            self.plotter_manager.plotters.remove(id);
        }

        self.scan_detected_plugins();
        self.show_info("Plugin", "Plugin uninstalled");
    }

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
            return;
        }
        self.status = "Plugin installed".to_string();
    }

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
            if self.selected_plugin_id == Some(*id) {
                self.selected_plugin_id = None;
            }
            if self.windows.plugin_config_id == Some(*id) {
                self.windows.plugin_config_id = None;
                self.windows.plugin_config_open = false;
            }
            self.plotter_manager.plotters.remove(id);
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
            return;
        }
        self.status = "Plugin refreshed".to_string();
    }

    pub(crate) fn refresh_installed_library_paths(&mut self) {
        self.plugin_manager.refresh_installed_library_paths();
    }

    pub(crate) fn inject_library_paths_into_workspace(&mut self) {
        self.plugin_manager
            .inject_library_paths_into_workspace(&mut self.workspace_manager.workspace);
    }

    pub(crate) fn load_installed_plugins(&mut self) {
        self.plugin_manager.load_installed_plugins();
    }

    pub(crate) fn refresh_installed_plugin_metadata_cache(&mut self) {
        let targets: Vec<(String, PathBuf)> = self
            .plugin_manager
            .installed_plugins
            .iter()
            .filter(|plugin| plugin.removable && !plugin.path.as_os_str().is_empty())
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
