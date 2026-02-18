use crate::state::ConfirmAction;
use crate::GuiApp;
use workspace::WorkspaceDefinition;

impl GuiApp {
    /// ```
    pub(crate) fn show_info(&mut self, title: &str, message: &str) {
        self.notification_handler.show_info(title, message);
    }

    /// ```
    pub(crate) fn show_plugin_info(&mut self, plugin_id: u64, title: &str, message: &str) {
        self.notification_handler
            .show_plugin_info(plugin_id, title, message);
    }

    /// ```
    pub(crate) fn show_confirm(
        &mut self,
        title: &str,
        message: &str,
        action_label: &str,
        action: ConfirmAction,
    ) {
        self.confirm_dialog.title = title.to_string();
        self.confirm_dialog.message = message.to_string();
        self.confirm_dialog.action_label = action_label.to_string();
        self.confirm_dialog.action = Some(action);
        self.confirm_dialog.open = true;
    }

    /// - Rescans available workspaces
    pub(crate) fn perform_confirm_action(&mut self, action: ConfirmAction) {
        match action {
            ConfirmAction::RemovePlugin(plugin_id) => {
                if let Some(index) = self
                    .workspace_manager
                    .workspace
                    .plugins
                    .iter()
                    .position(|plugin| plugin.id == plugin_id)
                {
                    self.remove_plugin(index);
                }
            }
            ConfirmAction::UninstallPlugin(index) => {
                self.uninstall_plugin(index);
            }
            ConfirmAction::DeleteWorkspace(path) => {
                let name = WorkspaceDefinition::load_from_file(&path)
                    .map(|ws| ws.name)
                    .unwrap_or_else(|_| {
                        path.file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("workspace")
                            .replace('_', " ")
                    });
                match self.workspace_manager.delete_workspace(&name) {
                    Ok(()) => {
                        if self.workspace_manager.workspace_path.as_os_str().is_empty() {
                            self.plotter_manager.plotters.clear();
                            self.apply_workspace_settings();
                            self.plugin_positions.clear();
                            self.state_plugin_positions.clear();
                        }
                        self.scan_workspaces();
                        self.show_info("Workspace", &format!("Workspace '{}' deleted", name));
                    }
                    Err(err) => {
                        self.show_info("Workspace Error", &err);
                    }
                }
            }
        }
    }
}
