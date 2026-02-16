use super::manager::PluginManager;
use super::types::{InstalledPlugin, PluginMetadataSource};
use std::path::{Path, PathBuf};
use workspace::WorkspaceDefinition;

pub struct PluginCatalog {
    pub manager: PluginManager,
}

impl PluginCatalog {
    pub fn new(install_db_path: PathBuf) -> Self {
        Self {
            manager: PluginManager::new(install_db_path),
        }
    }

    pub fn scan_detected_plugins(&mut self) {
        self.manager
            .scan_detected_plugins_in(&["plugins", "app_plugins", "rtsyn-plugins"]);
    }

    pub fn install_plugin_from_folder<P: AsRef<Path>>(
        &mut self,
        folder: P,
        removable: bool,
        persist: bool,
        metadata: &impl PluginMetadataSource,
    ) -> Result<(), String> {
        self.manager
            .install_plugin_from_folder(folder.as_ref(), removable, persist, metadata)
    }

    pub fn uninstall_plugin_by_kind(
        &mut self,
        kind_or_name: &str,
    ) -> Result<InstalledPlugin, String> {
        let index = self
            .manager
            .installed_plugins
            .iter()
            .position(|p| {
                p.manifest.kind == kind_or_name
                    || p.manifest.name.eq_ignore_ascii_case(kind_or_name)
            })
            .ok_or_else(|| "Plugin not installed".to_string())?;
        let plugin = self.manager.installed_plugins[index].clone();
        if !plugin.removable {
            return Err("Plugin is bundled and cannot be uninstalled".to_string());
        }
        self.manager.installed_plugins.remove(index);
        self.manager.persist_installed_plugins();
        Ok(plugin)
    }

    pub fn remove_plugins_by_kind_from_workspace(
        &mut self,
        workspace: &mut WorkspaceDefinition,
        kind: &str,
    ) -> Vec<u64> {
        self.manager
            .remove_plugins_by_kind(workspace, kind)
    }

    pub fn inject_library_paths_into_workspace(&self, workspace: &mut WorkspaceDefinition) {
        self.manager.inject_library_paths_into_workspace(workspace);
    }

    pub fn list_installed(&self) -> &[InstalledPlugin] {
        &self.manager.installed_plugins
    }

    pub fn refresh_library_paths(&mut self) {
        self.manager.refresh_library_paths();
    }

    pub fn add_installed_plugin_to_workspace(
        &mut self,
        kind_or_name: &str,
        workspace: &mut WorkspaceDefinition,
        metadata: &impl PluginMetadataSource,
    ) -> Result<u64, String> {
        let installed_index = self
            .manager
            .installed_plugins
            .iter()
            .position(|p| {
                p.manifest.kind == kind_or_name
                    || p.manifest.name.eq_ignore_ascii_case(kind_or_name)
            })
            .ok_or_else(|| "Plugin is not installed".to_string())?;

        self.manager
            .add_installed_plugin_to_workspace(installed_index, workspace, metadata)
    }

    pub fn sync_ids_from_workspace(&mut self, workspace: &WorkspaceDefinition) {
        let max_id = workspace.plugins.iter().map(|p| p.id).max();
        self.manager.sync_next_plugin_id(max_id);
    }

    pub fn remove_plugin_from_workspace(
        &mut self,
        plugin_id: u64,
        workspace: &mut WorkspaceDefinition,
    ) -> Result<(), String> {
        self.manager
            .remove_plugin_from_workspace(workspace, plugin_id)
    }

    pub fn reinstall_plugin_by_kind(
        &mut self,
        kind_or_name: &str,
        metadata: &impl PluginMetadataSource,
    ) -> Result<(), String> {
        let index = self
            .manager
            .installed_plugins
            .iter()
            .position(|p| {
                p.manifest.kind == kind_or_name
                    || p.manifest.name.eq_ignore_ascii_case(kind_or_name)
            })
            .ok_or_else(|| "Plugin not installed".to_string())?;
        let path = self.manager.installed_plugins[index].path.clone();
        if path.as_os_str().is_empty() {
            return Err("Plugin path is not set".to_string());
        }
        if self.manager.installed_plugins[index].removable {
            if !PluginManager::build_plugin(&path) {
                return Err("Failed to build plugin".to_string());
            }
        }
        self.manager.installed_plugins.remove(index);
        self.manager
            .install_plugin_from_folder(&path, true, true, metadata)
    }

    pub fn rebuild_plugin_by_kind(&mut self, kind_or_name: &str) -> Result<(), String> {
        let index = self
            .manager
            .installed_plugins
            .iter()
            .position(|p| {
                p.manifest.kind == kind_or_name
                    || p.manifest.name.eq_ignore_ascii_case(kind_or_name)
            })
            .ok_or_else(|| "Plugin not installed".to_string())?;
        let path = self.manager.installed_plugins[index].path.clone();
        if path.as_os_str().is_empty() {
            return Err("Plugin path is not set".to_string());
        }
        if !self.manager.installed_plugins[index].removable {
            return Err("Bundled plugins cannot be rebuilt".to_string());
        }
        if !PluginManager::build_plugin(&path) {
            return Err("Failed to build plugin".to_string());
        }
        self.manager.refresh_library_paths();
        Ok(())
    }

    pub fn display_kind(kind: &str) -> String {
        super::manager::PluginManager::display_kind(kind)
    }
}