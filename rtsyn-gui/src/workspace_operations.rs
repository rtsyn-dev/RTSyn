use crate::state::WorkspaceDialogMode;
use crate::{spawn_file_dialog_thread, GuiApp};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use workspace::WorkspaceDefinition;

impl GuiApp {
    pub(crate) fn load_workspace(&mut self) {
        if self.workspace_manager.workspace_path.as_os_str().is_empty() {
            self.show_info("Workspace", "Workspace path is required");
            return;
        }

        match self
            .workspace_manager
            .load_workspace(&self.workspace_manager.workspace_path.clone())
        {
            Ok(()) => {
                let name = self.workspace_manager.workspace.name.clone();
                self.refresh_installed_library_paths();
                self.inject_library_paths_into_workspace();
                self.apply_loads_started_on_load();
                for plugin in &self.workspace_manager.workspace.plugins {
                    let _ = self
                        .state_sync
                        .logic_tx
                        .send(rtsyn_runtime::LogicMessage::SetPluginRunning(
                            plugin.id,
                            plugin.running,
                        ));
                }
                self.open_running_plotters();
                self.enforce_connection_dependent();
                self.apply_workspace_settings();
                self.sync_next_plugin_id();
                self.plugin_manager.available_plugin_ids.clear();
                self.mark_workspace_dirty();
                self.show_info("Workspace", &format!("Workspace '{}' loaded", name));
            }
            Err(err) => {
                self.show_info("Workspace", &format!("Load failed: {err}"));
            }
        }
    }

    pub(crate) fn scan_workspaces(&mut self) {
        self.workspace_manager.scan_workspaces();
    }

    pub(crate) fn workspace_file_path(&self, name: &str) -> PathBuf {
        self.workspace_manager.workspace_file_path(name)
    }

    pub(crate) fn create_workspace_from_dialog(&mut self) -> bool {
        let name = self.workspace_dialog.name_input.trim();
        if name.is_empty() {
            self.show_info("Workspace", "Workspace name is required");
            return false;
        }

        if let Err(e) = self
            .workspace_manager
            .create_workspace(name, self.workspace_dialog.description_input.trim())
        {
            self.show_info("Workspace Error", &e);
            return false;
        }

        self.plugin_manager.next_plugin_id = 1;
        self.plugin_manager.available_plugin_ids.clear();
        self.show_info("Workspace", &format!("Workspace '{}' created", name));
        self.scan_workspaces();
        true
    }

    pub(crate) fn save_workspace_as(&mut self) -> bool {
        let name = self.workspace_dialog.name_input.trim();
        if name.is_empty() {
            self.show_info("Workspace", "Workspace name is required");
            return false;
        }

        if let Err(e) = self
            .workspace_manager
            .save_workspace_as(name, self.workspace_dialog.description_input.trim())
        {
            self.show_info("Workspace Error", &e);
            return false;
        }

        self.show_info("Workspace", &format!("Workspace '{}' saved", name));
        self.scan_workspaces();
        true
    }

    pub(crate) fn save_workspace_overwrite_current(&mut self) {
        if self.workspace_manager.workspace_path.as_os_str().is_empty() {
            self.open_workspace_dialog(WorkspaceDialogMode::Save);
            return;
        }
        self.workspace_manager.workspace.settings = self.current_workspace_settings();
        if let Err(e) = self.workspace_manager.save_workspace_overwrite_current() {
            self.show_info("Workspace Error", &e);
            return;
        }
        let display_name = self.workspace_manager.workspace.name.clone();
        self.show_info(
            "Workspace",
            &format!("Workspace '{}' updated", display_name),
        );
        self.scan_workspaces();
    }

    pub(crate) fn update_workspace_metadata(&mut self, path: &Path) -> bool {
        let name = self.workspace_dialog.name_input.trim();
        if name.is_empty() {
            self.show_info("Workspace", "Workspace name is required");
            return false;
        }
        let new_path = self.workspace_file_path(name);
        let mut updated = false;
        if let Ok(data) = fs::read(path) {
            if let Ok(mut workspace) = serde_json::from_slice::<WorkspaceDefinition>(&data) {
                workspace.name = name.to_string();
                workspace.description = self.workspace_dialog.description_input.trim().to_string();
                let _ = workspace.save_to_file(&new_path);
                if new_path != path {
                    let _ = fs::remove_file(path);
                }
                self.show_info("Workspace", &format!("Workspace '{}' updated", name));
                updated = true;
            }
        }
        self.scan_workspaces();
        updated
    }

    pub(crate) fn export_workspace_path(&mut self, source: &Path) {
        if self.file_dialogs.export_dialog_rx.is_some() {
            self.show_info("Workspace", "Dialog already open");
            return;
        }
        let source = source.to_path_buf();
        let workspace_name = match WorkspaceDefinition::load_from_file(&source) {
            Ok(ws) => format!("{}.json", ws.name),
            Err(_) => String::new(),
        };
        let (tx, rx) = mpsc::channel();
        self.file_dialogs.export_dialog_rx = Some(rx);
        spawn_file_dialog_thread(move || {
            let dest = if crate::has_rt_capabilities() {
                let filename = if !workspace_name.is_empty() {
                    Some(workspace_name.as_str())
                } else {
                    None
                };
                crate::zenity_file_dialog_with_name("save", None, filename)
            } else {
                let mut dialog = rfd::FileDialog::new();
                if !workspace_name.is_empty() {
                    dialog = dialog.set_file_name(&workspace_name);
                }
                dialog.save_file()
            };
            let _ = tx.send((source, dest));
        });
    }

    pub(crate) fn import_workspace_from_path(&mut self, path: &Path) {
        match self.workspace_manager.import_workspace(path) {
            Ok(()) => {
                self.show_info("Workspace", "Workspace imported");
                self.scan_workspaces();
            }
            Err(e) => {
                self.show_info("Workspace Error", &e);
            }
        }
    }
}
