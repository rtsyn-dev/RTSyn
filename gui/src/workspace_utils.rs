// Utility functions extracted from GUI to reduce coupling

use std::path::{Path, PathBuf};
use workspace::WorkspaceDefinition;
use crate::state::WorkspaceEntry;

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
