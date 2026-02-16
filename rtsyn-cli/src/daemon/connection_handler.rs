use crate::protocol::{ConnectionSummary, DaemonResponse};
use rtsyn_core::connection::{extendable_input_index, next_available_extendable_input_index};
use rtsyn_core::plugin::{is_extendable_inputs, InstalledPlugin};
use rtsyn_core::workspace::WorkspaceManager;
use rtsyn_runtime::LogicMessage;
use std::sync::mpsc;

fn plugin_inputs(installed: &[InstalledPlugin], kind: &str) -> Vec<String> {
    installed
        .iter()
        .find(|p| p.manifest.kind == kind)
        .map(|p| p.metadata_inputs.clone())
        .unwrap_or_default()
}

fn plugin_outputs(installed: &[InstalledPlugin], kind: &str) -> Vec<String> {
    installed
        .iter()
        .find(|p| p.manifest.kind == kind)
        .map(|p| p.metadata_outputs.clone())
        .unwrap_or_default()
}

fn source_port_is_valid(kind: &str, requested_port: &str, outputs: &[String]) -> bool {
    if outputs.iter().any(|p| p == requested_port) {
        return true;
    }
    if kind == "performance_monitor" {
        return matches!(
            requested_port,
            "period_us" | "latency_us" | "jitter_us" | "max_period_us"
        );
    }
    false
}

pub fn connection_list(workspace_manager: &WorkspaceManager) -> DaemonResponse {
    let connections = workspace_manager
        .workspace
        .connections
        .iter()
        .enumerate()
        .map(|(index, conn)| ConnectionSummary {
            index,
            from_plugin: conn.from_plugin,
            from_port: conn.from_port.clone(),
            to_plugin: conn.to_plugin,
            to_port: conn.to_port.clone(),
            kind: conn.kind.clone(),
        })
        .collect();
    DaemonResponse::ConnectionList { connections }
}

pub fn connection_show(workspace_manager: &WorkspaceManager, plugin_id: u64) -> DaemonResponse {
    let connections = workspace_manager
        .workspace
        .connections
        .iter()
        .enumerate()
        .filter(|(_, conn)| conn.from_plugin == plugin_id || conn.to_plugin == plugin_id)
        .map(|(index, conn)| ConnectionSummary {
            index,
            from_plugin: conn.from_plugin,
            from_port: conn.from_port.clone(),
            to_plugin: conn.to_plugin,
            to_port: conn.to_port.clone(),
            kind: conn.kind.clone(),
        })
        .collect();
    DaemonResponse::ConnectionList { connections }
}

pub fn connection_add(
    workspace_manager: &mut WorkspaceManager,
    installed: &[InstalledPlugin],
    logic_tx: &mpsc::Sender<LogicMessage>,
    from_plugin: u64,
    from_port: String,
    to_plugin: u64,
    to_port: String,
    kind: String,
) -> DaemonResponse {
    let from_exists = workspace_manager
        .workspace
        .plugins
        .iter()
        .any(|p| p.id == from_plugin);
    if !from_exists {
        return DaemonResponse::Error {
            message: "Source plugin not found in workspace".to_string(),
        };
    }

    let from_kind = workspace_manager
        .workspace
        .plugins
        .iter()
        .find(|p| p.id == from_plugin)
        .map(|p| p.kind.clone())
        .unwrap_or_default();

    let to_plugin_def = workspace_manager
        .workspace
        .plugins
        .iter()
        .find(|p| p.id == to_plugin)
        .cloned();

    if to_plugin_def.is_none() {
        return DaemonResponse::Error {
            message: "Target plugin not found in workspace".to_string(),
        };
    }

    if from_port.trim().is_empty() || to_port.trim().is_empty() || kind.trim().is_empty() {
        return DaemonResponse::Error {
            message: "Connection fields cannot be empty".to_string(),
        };
    }

    let from_outputs = plugin_outputs(installed, &from_kind);
    if from_outputs.is_empty() {
        return DaemonResponse::Error {
            message: "Source plugin outputs not available".to_string(),
        };
    }

    if !source_port_is_valid(&from_kind, &from_port, &from_outputs) {
        return DaemonResponse::Error {
            message: "Source port not found".to_string(),
        };
    }

    let to_kind = to_plugin_def
        .as_ref()
        .map(|p| p.kind.clone())
        .unwrap_or_default();

    if is_extendable_inputs(&to_kind) {
        if to_port == "in" {
            return DaemonResponse::Error {
                message: "Target port must be the next in_<number> or an existing input"
                    .to_string(),
            };
        }

        let next_idx = next_available_extendable_input_index(&workspace_manager.workspace, to_plugin);
        let to_idx = extendable_input_index(&to_port);
        let has_existing_port = workspace_manager
            .workspace
            .connections
            .iter()
            .any(|c| c.to_plugin == to_plugin && c.to_port == to_port);

        let valid_extendable = match to_idx {
            Some(idx) if idx == next_idx => true,
            Some(idx) if idx < next_idx => has_existing_port,
            _ => false,
        };

        if !valid_extendable {
            return DaemonResponse::Error {
                message: "Target port must be the next in_<number> or an existing input"
                    .to_string(),
            };
        }
    } else {
        let to_inputs = plugin_inputs(installed, &to_kind);
        if to_inputs.is_empty() {
            return DaemonResponse::Error {
                message: "Target plugin inputs not available".to_string(),
            };
        }

        if !to_inputs.iter().any(|p| p == &to_port) {
            return DaemonResponse::Error {
                message: "Target port not found".to_string(),
            };
        }
    }

    match rtsyn_core::connection::add_connection(
        &mut workspace_manager.workspace,
        installed,
        from_plugin,
        &from_port,
        to_plugin,
        &to_port,
        &kind,
    ) {
        Ok(()) => {
            let _ = logic_tx.send(LogicMessage::UpdateWorkspace(
                workspace_manager.workspace.clone(),
            ));
            DaemonResponse::Ok {
                message: "Connection added".to_string(),
            }
        }
        Err(err) => DaemonResponse::Error {
            message: format!("{err}"),
        },
    }
}

pub fn connection_remove(
    workspace_manager: &mut WorkspaceManager,
    logic_tx: &mpsc::Sender<LogicMessage>,
    from_plugin: u64,
    from_port: String,
    to_plugin: u64,
    to_port: String,
) -> DaemonResponse {
    let index = workspace_manager
        .workspace
        .connections
        .iter()
        .position(|conn| {
            conn.from_plugin == from_plugin
                && conn.from_port == from_port
                && conn.to_plugin == to_plugin
                && conn.to_port == to_port
        });

    match index {
        Some(idx) => {
            workspace_manager.workspace.connections.remove(idx);
            let _ = logic_tx.send(LogicMessage::UpdateWorkspace(
                workspace_manager.workspace.clone(),
            ));
            DaemonResponse::Ok {
                message: "Connection removed".to_string(),
            }
        }
        None => DaemonResponse::Error {
            message: "Connection not found".to_string(),
        },
    }
}

pub fn connection_remove_index(
    workspace_manager: &mut WorkspaceManager,
    logic_tx: &mpsc::Sender<LogicMessage>,
    index: usize,
) -> DaemonResponse {
    if index >= workspace_manager.workspace.connections.len() {
        DaemonResponse::Error {
            message: "Invalid connection index".to_string(),
        }
    } else {
        workspace_manager.workspace.connections.remove(index);
        let _ = logic_tx.send(LogicMessage::UpdateWorkspace(
            workspace_manager.workspace.clone(),
        ));
        DaemonResponse::Ok {
            message: "Connection removed".to_string(),
        }
    }
}