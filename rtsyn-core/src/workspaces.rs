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

pub fn scan_workspace_entries(workspace_dir: &Path) -> Vec<WorkspaceEntry> {
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

pub fn workspace_file_path(workspace_dir: &Path, name: &str) -> PathBuf {
    let safe = name.trim().replace(' ', "_");
    workspace_dir.join(format!("{safe}.json"))
}

pub fn empty_workspace(name: &str) -> WorkspaceDefinition {
    WorkspaceDefinition {
        name: name.to_string(),
        description: String::new(),
        target_hz: 1000,
        plugins: Vec::new(),
        connections: Vec::new(),
        settings: WorkspaceSettings::default(),
    }
}

pub fn load_workspace(path: &Path) -> Result<WorkspaceDefinition, String> {
    WorkspaceDefinition::load_from_file(path)
        .map_err(|e| format!("Failed to load workspace: {e}"))
}

pub fn save_workspace(workspace: &WorkspaceDefinition, path: &Path) -> Result<(), String> {
    workspace
        .save_to_file(path)
        .map_err(|e| format!("Failed to save workspace: {e}"))
}

pub fn rename_workspace_file(old_path: &Path, new_path: &Path) -> Result<(), String> {
    if old_path == new_path {
        return Ok(());
    }
    std::fs::remove_file(old_path)
        .map_err(|e| format!("Failed to remove old workspace file: {e}"))?;
    Ok(())
}
