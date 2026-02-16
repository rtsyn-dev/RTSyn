use workspace::{WorkspaceDefinition, WorkspaceSettings};

pub struct WorkspaceOperations {
    pub workspace: WorkspaceDefinition,
    pub workspace_dirty: bool,
}

impl WorkspaceOperations {
    pub fn new(workspace: WorkspaceDefinition) -> Self {
        Self {
            workspace,
            workspace_dirty: false,
        }
    }

    pub fn mark_dirty(&mut self) {
        self.workspace_dirty = true;
    }

    pub fn current_workspace_settings(&self) -> WorkspaceSettings {
        self.workspace.settings.clone()
    }

    pub fn apply_workspace_settings(&mut self, settings: &WorkspaceSettings) {
        self.workspace.settings = settings.clone();
        self.mark_dirty();
    }
}