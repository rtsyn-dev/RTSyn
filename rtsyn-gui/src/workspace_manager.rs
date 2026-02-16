use workspace::{WorkspaceDefinition, WorkspaceSettings};

/// Manages workspace operations and state tracking.
/// 
/// This struct handles workspace definition management and tracks modification
/// state to enable proper save/load workflows and change detection.
pub struct WorkspaceOperations {
    /// The current workspace definition containing all workspace data
    pub workspace: WorkspaceDefinition,
    /// Flag indicating whether the workspace has unsaved changes
    pub workspace_dirty: bool,
}

impl WorkspaceOperations {
    /// Creates a new WorkspaceOperations instance with the given workspace definition.
    /// 
    /// # Parameters
    /// - `workspace`: The initial workspace definition to manage
    /// 
    /// # Returns
    /// A new WorkspaceOperations instance with the workspace set to clean state.
    /// 
    /// # Initial State
    /// - Sets `workspace_dirty` to false, indicating no unsaved changes
    pub fn new(workspace: WorkspaceDefinition) -> Self {
        Self {
            workspace,
            workspace_dirty: false,
        }
    }

    /// Marks the workspace as having unsaved changes.
    /// 
    /// This function sets the dirty flag to indicate that the workspace has been
    /// modified and requires saving. This is used by the UI to show save indicators
    /// and prevent data loss by prompting for saves before closing or loading.
    /// 
    /// # Behavior
    /// - Sets `workspace_dirty` to true
    /// - Should be called whenever workspace data is modified
    /// - Used to trigger save prompts and UI indicators
    pub fn mark_dirty(&mut self) {
        self.workspace_dirty = true;
    }

    /// Retrieves a copy of the current workspace settings.
    /// 
    /// This function returns a cloned copy of the workspace settings, allowing
    /// for safe access to configuration data without risking modification of
    /// the original workspace state.
    /// 
    /// # Returns
    /// A cloned copy of the current WorkspaceSettings.
    /// 
    /// # Use Cases
    /// - Reading configuration values for UI display
    /// - Passing settings to other components safely
    /// - Creating modified copies for settings dialogs
    pub fn current_workspace_settings(&self) -> WorkspaceSettings {
        self.workspace.settings.clone()
    }

    /// Applies new workspace settings and marks the workspace as modified.
    /// 
    /// This function updates the workspace settings with the provided configuration
    /// and automatically marks the workspace as dirty to indicate unsaved changes.
    /// 
    /// # Parameters
    /// - `settings`: The new workspace settings to apply
    /// 
    /// # Behavior
    /// - Clones and stores the new settings in the workspace definition
    /// - Automatically calls `mark_dirty()` to indicate unsaved changes
    /// - Triggers the need for workspace saving to persist changes
    /// 
    /// # Use Cases
    /// - Applying settings from configuration dialogs
    /// - Updating workspace configuration programmatically
    /// - Batch updating multiple settings at once
    pub fn apply_workspace_settings(&mut self, settings: &WorkspaceSettings) {
        self.workspace.settings = settings.clone();
        self.mark_dirty();
    }
}