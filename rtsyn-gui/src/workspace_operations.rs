use crate::state::WorkspaceDialogMode;
use crate::{spawn_file_dialog_thread, GuiApp};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use workspace::WorkspaceDefinition;

impl GuiApp {
    /// Loads a workspace from the configured workspace path.
    ///
    /// # Side Effects
    /// - Validates workspace path is not empty
    /// - Loads workspace definition from file
    /// - Refreshes and injects plugin library paths
    /// - Applies load-on-startup settings
    /// - Synchronizes plugin running states with runtime
    /// - Opens plotter viewports for running plugins
    /// - Enforces connection dependencies
    /// - Applies workspace-specific settings
    /// - Synchronizes next plugin ID counter
    /// - Clears available plugin IDs cache
    /// - Marks workspace as dirty
    /// - Shows success/error notifications
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
                    let _ = self.state_sync.logic_tx.send(
                        rtsyn_runtime::LogicMessage::SetPluginRunning(plugin.id, plugin.running),
                    );
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

    /// Scans the workspace directory for available workspace files.
    ///
    /// # Side Effects
    /// Updates the workspace manager's list of available workspaces
    pub(crate) fn scan_workspaces(&mut self) {
        self.workspace_manager.scan_workspaces();
    }

    /// Constructs the file path for a workspace with the given name.
    ///
    /// # Parameters
    /// - `name`: Name of the workspace
    ///
    /// # Returns
    /// PathBuf containing the full path to the workspace file
    pub(crate) fn workspace_file_path(&self, name: &str) -> PathBuf {
        self.workspace_manager.workspace_file_path(name)
    }

    /// Creates a new workspace using data from the workspace dialog.
    ///
    /// # Returns
    /// - `true` if workspace was created successfully
    /// - `false` if creation failed or name was empty
    ///
    /// # Side Effects
    /// - Validates workspace name is not empty
    /// - Creates new workspace with name and description from dialog
    /// - Resets plugin ID counter and cache
    /// - Shows success/error notifications
    /// - Rescans available workspaces
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

    /// Saves the current workspace with a new name using dialog data.
    ///
    /// # Returns
    /// - `true` if workspace was saved successfully
    /// - `false` if save failed or name was empty
    ///
    /// # Side Effects
    /// - Validates workspace name is not empty
    /// - Saves current workspace with new name and description from dialog
    /// - Shows success/error notifications
    /// - Rescans available workspaces
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

    /// Saves the current workspace, overwriting the existing file.
    ///
    /// # Side Effects
    /// - Opens save dialog if no current workspace path exists
    /// - Updates workspace settings from current GUI state
    /// - Overwrites existing workspace file
    /// - Shows success/error notifications
    /// - Rescans available workspaces
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

    /// Updates metadata (name and description) for an existing workspace.
    ///
    /// # Parameters
    /// - `path`: Path to the existing workspace file
    ///
    /// # Returns
    /// - `true` if metadata was updated successfully
    /// - `false` if update failed or name was empty
    ///
    /// # Side Effects
    /// - Validates workspace name is not empty
    /// - Loads existing workspace data
    /// - Updates name and description from dialog
    /// - Saves to new path if name changed
    /// - Removes old file if path changed
    /// - Shows success notifications
    /// - Rescans available workspaces
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

    /// Initiates export dialog for a workspace file.
    ///
    /// # Parameters
    /// - `source`: Path to the workspace file to export
    ///
    /// # Side Effects
    /// - Checks if export dialog is already open
    /// - Loads workspace name for default filename
    /// - Spawns file dialog thread for destination selection
    /// - Sets up channel for receiving dialog result
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

    /// Imports a workspace from an external file path.
    ///
    /// # Parameters
    /// - `path`: Path to the workspace file to import
    ///
    /// # Side Effects
    /// - Imports workspace into the workspace directory
    /// - Shows success/error notifications
    /// - Rescans available workspaces on success
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
