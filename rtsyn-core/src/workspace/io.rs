use crate::plugin::PluginManager;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use workspace::WorkspaceDefinition;

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
                        plugin_kinds: workspace
                            .plugins
                            .iter()
                            .map(|p| p.kind.clone())
                            .collect(),
                        path,
                    });
                }
            }
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

pub fn workspace_file_path_for(workspace_dir: &Path, name: &str) -> PathBuf {
    let safe = name.trim().replace(' ', "_");
    workspace_dir.join(format!("{safe}.json"))
}

pub fn load_workspace_file(path: &Path) -> Result<WorkspaceDefinition, String> {
    WorkspaceDefinition::load_from_file(path)
        .map_err(|e| format!("Failed to load workspace: {e}"))
}

pub fn save_workspace_file(workspace: &WorkspaceDefinition, path: &Path) -> Result<(), String> {
    workspace
        .save_to_file(path)
        .map_err(|e| format!("Failed to save workspace: {e}"))
}

pub fn remove_old_workspace_file(old_path: &Path, new_path: &Path) -> Result<(), String> {
    if old_path == new_path {
        return Ok(());
    }
    std::fs::remove_file(old_path)
        .map_err(|e| format!("Failed to remove old workspace file: {e}"))?;
    Ok(())
}

fn uml_escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn workspace_to_uml_diagram(workspace: &WorkspaceDefinition) -> String {
    let mut lines = Vec::new();
    lines.push("@startuml".to_string());
    lines.push("skinparam componentStyle rectangle".to_string());
    lines.push("skinparam ranksep 120".to_string());
    lines.push("skinparam nodesep 120".to_string());
    lines.push("skinparam ArrowFontSize 11".to_string());
    lines.push(String::new());
    lines.push(format!(
        "title RTSyn Workspace - {}",
        uml_escape(&workspace.name)
    ));
    lines.push(String::new());

    for plugin in &workspace.plugins {
        let display_name = PluginManager::display_kind(&plugin.kind);
        let plugin_name = plugin
            .config
            .get("name")
            .and_then(|v| v.as_str())
            .filter(|v| !v.trim().is_empty())
            .unwrap_or(&display_name);
        let label = format!("{}-{}", uml_escape(plugin_name), plugin.id);
        lines.push(format!("component \"{label}\" as P{}", plugin.id));
    }
    lines.push(String::new());

    if workspace.plugins.is_empty() {
        lines.push("note \"No plugins in workspace\" as N0".to_string());
    }

    for conn in &workspace.connections {
        let from_port = uml_escape(&conn.from_port);
        let to_port = uml_escape(&conn.to_port);
        lines.push(format!("P{} --> P{}", conn.from_plugin, conn.to_plugin));
        lines.push("note on link".to_string());
        if from_port == to_port {
            lines.push(from_port);
        } else {
            lines.push(format!("{from_port} to {to_port}"));
        }
        lines.push("end note".to_string());
        lines.push(String::new());
    }

    if workspace.connections.is_empty() {
        lines.push("note \"No connections in workspace\" as N1".to_string());
    }

    lines.push("@enduml".to_string());
    lines.join("\n")
}