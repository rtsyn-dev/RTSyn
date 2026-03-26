use std::{path::PathBuf, sync::mpsc};

use rtsyn_core::{
    plugin::{InstalledPlugin, PluginCatalog, PluginMetadataSource},
    workspace::WorkspaceManager,
};
use rtsyn_runtime::LogicMessage;

use crate::{
    daemon::DaemonState,
    protocol::{DaemonResponse, PluginRequest, PluginSummary},
};

fn normalize_plugin_key(input: &str) -> String {
    let trimmed = input.trim();
    if let Some(start) = trimmed.rfind('(') {
        if let Some(end) = trimmed.rfind(')') {
            if end > start + 1 {
                return trimmed[start + 1..end].trim().to_string();
            }
        }
    }
    trimmed.to_string()
}

pub fn plugin_set(
    workspace_manager: &mut WorkspaceManager,
    id: u64,
    json: String,
    logic_tx: &mpsc::Sender<LogicMessage>,
) -> DaemonResponse {
    if let Some(plugin) = workspace_manager
        .workspace
        .plugins
        .iter_mut()
        .find(|p| p.id == id)
    {
        match serde_json::from_str::<serde_json::Value>(&json) {
            Ok(value) => {
                if let Some(obj) = value.as_object() {
                    let map_result = match plugin.config {
                        serde_json::Value::Object(ref mut map) => Ok(map),
                        _ => {
                            plugin.config = serde_json::Value::Object(serde_json::Map::new());
                            match plugin.config {
                                serde_json::Value::Object(ref mut map) => Ok(map),
                                _ => Err("Failed to update plugin config".to_string()),
                            }
                        }
                    };

                    match map_result {
                        Ok(map) => {
                            for (key, val) in obj {
                                map.insert(key.clone(), val.clone());
                                let _ = logic_tx.send(LogicMessage::SetPluginVariable(
                                    id,
                                    key.clone(),
                                    val.clone(),
                                ));
                            }
                            DaemonResponse::Ok {
                                message: "Runtime variables updated".to_string(),
                            }
                        }
                        Err(message) => DaemonResponse::Error { message },
                    }
                } else {
                    DaemonResponse::Error {
                        message: "Variables must be a JSON object".to_string(),
                    }
                }
            }
            Err(err) => DaemonResponse::Error {
                message: format!("Invalid JSON: {err}"),
            },
        }
    } else {
        DaemonResponse::Error {
            message: "Plugin not found in runtime".to_string(),
        }
    }
}
pub fn plugin_remove(
    catalog: &mut PluginCatalog,
    workspace_manager: &mut WorkspaceManager,
    id: u64,
    refresh_fn: impl Fn(),
) -> DaemonResponse {
    match catalog.remove_plugin_from_workspace(id, &mut workspace_manager.workspace) {
        Ok(()) => {
            refresh_fn();
            DaemonResponse::Ok {
                message: "Plugin removed".to_string(),
            }
        }
        Err(err) => DaemonResponse::Error { message: err },
    }
}

pub fn plugin_add<T: PluginMetadataSource>(
    catalog: &mut PluginCatalog,
    workspace_manager: &mut WorkspaceManager,
    name: String,
    runtime_query: &T,
    refresh_fn: impl Fn(),
) -> DaemonResponse {
    let key = normalize_plugin_key(&name);
    match catalog.add_installed_plugin_to_workspace(
        &key,
        &mut workspace_manager.workspace,
        runtime_query,
    ) {
        Ok(id) => {
            refresh_fn();
            DaemonResponse::PluginAdded { id }
        }
        Err(err) => DaemonResponse::Error { message: err },
    }
}
pub fn plugin_rebuild(catalog: &mut PluginCatalog, name: String) -> DaemonResponse {
    let key = normalize_plugin_key(&name);
    match catalog.rebuild_plugin_by_kind(&key) {
        Ok(()) => DaemonResponse::Ok {
            message: "Plugin rebuilt".to_string(),
        },
        Err(err) => DaemonResponse::Error { message: err },
    }
}

pub fn plugin_reinstall<T: PluginMetadataSource>(
    catalog: &mut PluginCatalog,
    name: String,
    runtime_query: &T,
) -> DaemonResponse {
    let key = normalize_plugin_key(&name);
    match catalog.reinstall_plugin_by_kind(&key, runtime_query) {
        Ok(()) => DaemonResponse::Ok {
            message: "Plugin reinstalled".to_string(),
        },
        Err(err) => DaemonResponse::Error { message: err },
    }
}

pub fn plugin_uninstall(
    catalog: &mut PluginCatalog,
    workspace_manager: &mut WorkspaceManager,
    name: String,
    refresh_fn: impl Fn(),
) -> DaemonResponse {
    let key = normalize_plugin_key(&name);
    match catalog.uninstall_plugin_by_kind(&key) {
        Ok(plugin) => {
            let removed_ids = catalog.remove_plugins_by_kind_from_workspace(
                &mut workspace_manager.workspace,
                &plugin.manifest.kind,
            );
            if !removed_ids.is_empty() {
                refresh_fn();
            }
            DaemonResponse::Ok {
                message: "Plugin uninstalled".to_string(),
            }
        }
        Err(err) => DaemonResponse::Error { message: err },
    }
}

pub fn plugin_install<T: PluginMetadataSource>(
    catalog: &mut PluginCatalog,
    path: String,
    runtime_query: &T,
) -> DaemonResponse {
    let install_path = PathBuf::from(&path);
    if !install_path.is_absolute() {
        return DaemonResponse::Error {
            message: "Plugin install path must be absolute".to_string(),
        };
    }
    let resolved = std::fs::canonicalize(&install_path).unwrap_or(install_path);
    match catalog.install_plugin_from_folder(resolved, true, true, runtime_query) {
        Ok(()) => DaemonResponse::Ok {
            message: "Plugin installed".to_string(),
        },
        Err(err) => DaemonResponse::Error { message: err },
    }
}

pub fn plugin_list(catalog: &PluginCatalog) -> DaemonResponse {
    let plugins = catalog
        .list_installed()
        .iter()
        .map(|p| PluginSummary {
            kind: p.manifest.kind.clone(),
            name: p.manifest.name.clone(),
            version: p.manifest.version.clone(),
            removable: p.removable,
            path: if p.path.as_os_str().is_empty() {
                None
            } else {
                let canonical = std::fs::canonicalize(&p.path)
                    .ok()
                    .map(|path| path.to_string_lossy().to_string());
                Some(canonical.unwrap_or_else(|| p.path.to_string_lossy().to_string()))
            },
        })
        .collect();
    DaemonResponse::PluginList { plugins }
}

pub fn plugin_inputs(installed: &[InstalledPlugin], kind: &str) -> Vec<String> {
    installed
        .iter()
        .find(|p| p.manifest.kind == kind)
        .map(|p| p.metadata_inputs.clone())
        .unwrap_or_default()
}

pub fn plugin_outputs(installed: &[InstalledPlugin], kind: &str) -> Vec<String> {
    installed
        .iter()
        .find(|p| p.manifest.kind == kind)
        .map(|p| p.metadata_outputs.clone())
        .unwrap_or_default()
}

pub(super) fn plugin_handle(request: PluginRequest, state: &mut DaemonState) -> DaemonResponse {
    let response = match request {
        PluginRequest::PluginList => plugin_list(&state.catalog),
        PluginRequest::PluginInstall { path } => {
            plugin_install(&mut state.catalog, path, &state.runtime_query)
        }
        PluginRequest::PluginReinstall { name } => {
            plugin_reinstall(&mut state.catalog, name, &state.runtime_query)
        }
        PluginRequest::PluginRebuild { name } => plugin_rebuild(&mut state.catalog, name),
        PluginRequest::PluginUninstall { name } => {
            let response = plugin_uninstall(
                &mut state.catalog,
                &mut state.workspace_manager,
                name,
                || {},
            );
            if matches!(response, DaemonResponse::Ok { .. }) {
                state.refresh_runtime();
            }
            response
        }
        PluginRequest::PluginAdd { name } => {
            let response = plugin_add(
                &mut state.catalog,
                &mut state.workspace_manager,
                name,
                &state.runtime_query,
                || {},
            );
            if matches!(response, DaemonResponse::PluginAdded { .. }) {
                state.refresh_runtime();
            }
            response
        }
        PluginRequest::PluginRemove { id } => {
            let response =
                plugin_remove(&mut state.catalog, &mut state.workspace_manager, id, || {});
            if matches!(response, DaemonResponse::Ok { .. }) {
                state.refresh_runtime();
            }
            response
        }
    };

    response
}
