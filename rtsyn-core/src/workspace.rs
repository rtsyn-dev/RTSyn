use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use workspace::{WorkspaceDefinition, WorkspaceSettings};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceEntry {
    pub name: String,
    pub description: String,
    pub plugins: usize,
    pub plugin_kinds: Vec<String>,
    pub path: PathBuf,
}

pub struct WorkspaceManager {
    pub workspace: WorkspaceDefinition,
    pub workspace_path: PathBuf,
    pub workspace_dirty: bool,
    pub workspace_entries: Vec<WorkspaceEntry>,
    workspace_dir: PathBuf,
}

impl WorkspaceManager {
    fn scan_workspace_entries(workspace_dir: &Path) -> Vec<WorkspaceEntry> {
        let mut entries = Vec::new();
        let _ = std::fs::create_dir_all(workspace_dir);
        if let Ok(dir_entries) = std::fs::read_dir(workspace_dir) {
            for entry in dir_entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(data) = std::fs::read(&path) {
                    if let Ok(workspace) = serde_json::from_slice::<WorkspaceDefinition>(&data) {
                        entries.push(WorkspaceEntry {
                            name: workspace.name,
                            description: workspace.description,
                            plugins: workspace.plugins.len(),
                            plugin_kinds: workspace.plugins.iter().map(|p| p.kind.clone()).collect(),
                            path,
                        });
                    }
                }
            }
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries
    }

    fn workspace_file_path_for(workspace_dir: &Path, name: &str) -> PathBuf {
        let safe = name.trim().replace(' ', "_");
        workspace_dir.join(format!("{safe}.json"))
    }

    fn empty_workspace(name: &str) -> WorkspaceDefinition {
        WorkspaceDefinition {
            name: name.to_string(),
            description: String::new(),
            target_hz: 1000,
            plugins: Vec::new(),
            connections: Vec::new(),
            settings: WorkspaceSettings::default(),
        }
    }

    fn load_workspace_file(path: &Path) -> Result<WorkspaceDefinition, String> {
        WorkspaceDefinition::load_from_file(path)
            .map_err(|e| format!("Failed to load workspace: {e}"))
    }

    fn save_workspace_file(workspace: &WorkspaceDefinition, path: &Path) -> Result<(), String> {
        workspace
            .save_to_file(path)
            .map_err(|e| format!("Failed to save workspace: {e}"))
    }

    fn remove_old_workspace_file(old_path: &Path, new_path: &Path) -> Result<(), String> {
        if old_path == new_path {
            return Ok(());
        }
        std::fs::remove_file(old_path)
            .map_err(|e| format!("Failed to remove old workspace file: {e}"))?;
        Ok(())
    }

    pub fn new(workspace_dir: PathBuf) -> Self {
        Self {
            workspace: Self::empty_workspace("default"),
            workspace_path: PathBuf::new(),
            workspace_dirty: true,
            workspace_entries: Vec::new(),
            workspace_dir,
        }
    }

    pub fn workspace_dir(&self) -> &Path {
        &self.workspace_dir
    }

    pub fn mark_dirty(&mut self) {
        self.workspace_dirty = true;
    }

    pub fn workspace_file_path(&self, name: &str) -> PathBuf {
        Self::workspace_file_path_for(&self.workspace_dir, name)
    }

    pub fn scan_workspaces(&mut self) {
        self.workspace_entries = Self::scan_workspace_entries(&self.workspace_dir);
    }

    pub fn load_workspace(&mut self, path: &Path) -> Result<(), String> {
        let loaded = Self::load_workspace_file(path)?;
        self.workspace = loaded;
        self.workspace_path = path.to_path_buf();
        self.workspace_dirty = false;
        Ok(())
    }

    pub fn save_workspace_overwrite_current(&mut self) -> Result<(), String> {
        if self.workspace_path.as_os_str().is_empty() {
            return Err("No workspace path set".to_string());
        }
        Self::save_workspace_file(&self.workspace, &self.workspace_path)?;
        self.workspace_dirty = false;
        Ok(())
    }

    pub fn save_workspace_as(&mut self, name: &str, description: &str) -> Result<(), String> {
        self.workspace.name = name.to_string();
        self.workspace.description = description.to_string();

        let path = self.workspace_file_path(name);
        let _ = std::fs::create_dir_all(&self.workspace_dir);
        Self::save_workspace_file(&self.workspace, &path)?;
        self.workspace_path = path;
        self.workspace_dirty = false;
        Ok(())
    }

    pub fn create_workspace(&mut self, name: &str, description: &str) -> Result<(), String> {
        let path = self.workspace_file_path(name);
        if path.exists() {
            return Err("Workspace already exists".to_string());
        }

        self.workspace = Self::empty_workspace(name);
        self.workspace.description = description.to_string();

        let _ = std::fs::create_dir_all(&self.workspace_dir);
        Self::save_workspace_file(&self.workspace, &path)?;
        self.workspace_path = path;
        self.workspace_dirty = false;
        Ok(())
    }

    pub fn import_workspace(&mut self, source: &Path) -> Result<(), String> {
        let loaded = Self::load_workspace_file(source)?;
        let dest_path = self.workspace_file_path(&loaded.name);
        if dest_path.exists() {
            return Err("Workspace with this name already exists".to_string());
        }
        let _ = std::fs::create_dir_all(&self.workspace_dir);
        std::fs::copy(source, &dest_path)
            .map_err(|e| format!("Failed to copy: {e}"))?;
        Ok(())
    }

    pub fn rename_workspace(&mut self, name: &str) -> Result<(), String> {
        if self.workspace_path.as_os_str().is_empty() {
            return Err("No workspace loaded to edit".to_string());
        }
        let current_path = self.workspace_path.clone();
        let new_path = self.workspace_file_path(name);
        let mut workspace = self.workspace.clone();
        workspace.name = name.to_string();
        Self::save_workspace_file(&workspace, &new_path)?;
        Self::remove_old_workspace_file(&current_path, &new_path)?;
        self.workspace = workspace;
        self.workspace_path = new_path;
        self.workspace_dirty = false;
        Ok(())
    }

    pub fn delete_workspace(&mut self, name: &str) -> Result<(), String> {
        let path = self.workspace_file_path(name);
        if !path.exists() {
            return Err("Workspace not found".to_string());
        }
        std::fs::remove_file(&path)
            .map_err(|e| format!("Failed to delete workspace: {e}"))?;
        if self.workspace_path == path {
            self.workspace = Self::empty_workspace("default");
            self.workspace_path = PathBuf::new();
            self.workspace_dirty = true;
        }
        Ok(())
    }
}
