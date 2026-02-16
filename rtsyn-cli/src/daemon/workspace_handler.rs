use crate::protocol::{DaemonResponse, WorkspaceSummary};
use rtsyn_core::workspace::WorkspaceManager;
use rtsyn_core::plugin::PluginCatalog;
use rtsyn_runtime::LogicMessage;
use std::sync::mpsc;
use std::time::Duration;

pub fn workspace_list(workspace_manager: &mut WorkspaceManager) -> DaemonResponse {
    workspace_manager.scan_workspaces();
    let workspaces = workspace_manager
        .workspace_entries
        .clone()
        .into_iter()
        .enumerate()
        .map(|(index, entry)| WorkspaceSummary {
            index,
            name: entry.name,
            description: entry.description,
            plugins: entry.plugins,
            plugin_kinds: entry.plugin_kinds,
        })
        .collect();
    DaemonResponse::WorkspaceList { workspaces }
}

pub fn workspace_load(
    name: String,
    workspace_manager: &mut WorkspaceManager,
    catalog: &mut PluginCatalog,
    logic_tx: &mpsc::Sender<LogicMessage>,
    logic_settings: &mut rtsyn_runtime::LogicSettings,
) -> DaemonResponse {
    let path = workspace_manager.workspace_file_path(&name);
    match workspace_manager.load_workspace(&path) {
        Ok(()) => {
            let mut workspace = workspace_manager.workspace.clone();
            catalog.refresh_library_paths();
            catalog.inject_library_paths_into_workspace(&mut workspace);
            catalog.sync_ids_from_workspace(&workspace);
            
            for plugin in &mut workspace.plugins {
                if plugin.running {
                    continue;
                }
                let config_path = plugin
                    .config
                    .get("library_path")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string());
                let library_path = config_path.or_else(|| {
                    catalog.manager.installed_plugins
                        .iter()
                        .find(|p| p.manifest.kind == plugin.kind)
                        .and_then(|p| p.library_path.as_ref())
                        .map(|p| p.to_string_lossy().to_string())
                });
                
                if let Some(behavior) = query_plugin_behavior(
                    logic_tx,
                    &plugin.kind,
                    library_path.as_deref(),
                ) {
                    if behavior.loads_started {
                        plugin.running = true;
                    }
                }
            }
            
            workspace_manager.workspace = workspace;
            if let Ok(runtime_settings) = workspace_manager.runtime_settings() {
                logic_settings.cores = runtime_settings.cores;
                logic_settings.period_seconds = runtime_settings.period_seconds;
                logic_settings.time_scale = runtime_settings.time_scale;
                logic_settings.time_label = runtime_settings.time_label;
                let _ = logic_tx.send(LogicMessage::UpdateSettings(logic_settings.clone()));
            }
            let _ = logic_tx.send(LogicMessage::UpdateWorkspace(workspace_manager.workspace.clone()));
            
            DaemonResponse::Ok {
                message: format!("Workspace '{}' loaded", name),
            }
        }
        Err(err) => DaemonResponse::Error { message: err },
    }
}

pub fn workspace_new(
    name: String,
    workspace_manager: &mut WorkspaceManager,
    catalog: &mut PluginCatalog,
    logic_tx: &mpsc::Sender<LogicMessage>,
) -> DaemonResponse {
    match workspace_manager.create_workspace(&name, "") {
        Ok(()) => {
            catalog.sync_ids_from_workspace(&workspace_manager.workspace);
            let _ = logic_tx.send(LogicMessage::UpdateWorkspace(workspace_manager.workspace.clone()));
            DaemonResponse::Ok {
                message: format!("Workspace '{}' created", name),
            }
        }
        Err(err) => DaemonResponse::Error { message: err },
    }
}

pub fn workspace_save(
    name: Option<String>,
    workspace_manager: &mut WorkspaceManager,
    logic_tx: &mpsc::Sender<LogicMessage>,
) -> DaemonResponse {
    let result = match name.as_ref() {
        Some(name) => {
            let description = workspace_manager.workspace.description.clone();
            workspace_manager.save_workspace_as(name, &description)
        }
        None => workspace_manager.save_workspace_overwrite_current(),
    };
    
    match result {
        Ok(()) => {
            let display_name = name
                .as_ref()
                .cloned()
                .unwrap_or_else(|| workspace_manager.workspace.name.clone());
            let _ = logic_tx.send(LogicMessage::UpdateWorkspace(workspace_manager.workspace.clone()));
            DaemonResponse::Ok {
                message: format!("Workspace '{}' saved", display_name),
            }
        }
        Err(err) => {
            let message = if name.is_none() && err == "No workspace path set" {
                "No workspace loaded. Use 'rtsyn daemon workspace save <name>' to save the current workspace.".to_string()
            } else {
                err
            };
            DaemonResponse::Error { message }
        }
    }
}

pub fn workspace_edit(
    name: String,
    workspace_manager: &mut WorkspaceManager,
    logic_tx: &mpsc::Sender<LogicMessage>,
) -> DaemonResponse {
    match workspace_manager.rename_workspace(&name) {
        Ok(()) => {
            let _ = logic_tx.send(LogicMessage::UpdateWorkspace(workspace_manager.workspace.clone()));
            DaemonResponse::Ok {
                message: format!("Workspace '{}' updated", name),
            }
        }
        Err(err) => DaemonResponse::Error { message: err },
    }
}

pub fn workspace_delete(
    name: String,
    workspace_manager: &mut WorkspaceManager,
    logic_tx: &mpsc::Sender<LogicMessage>,
    logic_settings: &mut rtsyn_runtime::LogicSettings,
) -> DaemonResponse {
    match workspace_manager.delete_workspace(&name) {
        Ok(()) => {
            if let Ok(runtime_settings) = workspace_manager.runtime_settings() {
                logic_settings.cores = runtime_settings.cores;
                logic_settings.period_seconds = runtime_settings.period_seconds;
                logic_settings.time_scale = runtime_settings.time_scale;
                logic_settings.time_label = runtime_settings.time_label;
                let _ = logic_tx.send(LogicMessage::UpdateSettings(logic_settings.clone()));
            }
            let _ = logic_tx.send(LogicMessage::UpdateWorkspace(workspace_manager.workspace.clone()));
            DaemonResponse::Ok {
                message: format!("Workspace '{}' deleted", name),
            }
        }
        Err(err) => DaemonResponse::Error { message: err },
    }
}

fn query_plugin_behavior(
    logic_tx: &mpsc::Sender<LogicMessage>,
    kind: &str,
    library_path: Option<&str>,
) -> Option<rtsyn_plugin::ui::PluginBehavior> {
    let (tx, rx) = mpsc::channel();
    let _ = logic_tx.send(LogicMessage::QueryPluginBehavior(
        kind.to_string(),
        library_path.map(|s| s.to_string()),
        tx,
    ));
    rx.recv_timeout(Duration::from_secs(1)).ok().flatten()
}