use crate::protocol::{DaemonResponse, PluginSummary};
use rtsyn_core::plugin::{PluginCatalog, PluginMetadataSource};
use rtsyn_core::workspace::WorkspaceManager;
use rtsyn_runtime::LogicMessage;
use std::path::PathBuf;
use std::sync::mpsc;

pub fn plugin_available(catalog: &PluginCatalog) -> DaemonResponse {
    // This would typically check for available plugins in repositories or scan directories
    // For now, return the list of installed plugins as available
    plugin_list(catalog)
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

pub fn plugin_rebuild(catalog: &mut PluginCatalog, name: String) -> DaemonResponse {
    let key = normalize_plugin_key(&name);
    match catalog.rebuild_plugin_by_kind(&key) {
        Ok(()) => DaemonResponse::Ok {
            message: "Plugin rebuilt".to_string(),
        },
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

pub fn plugin_show(catalog: &PluginCatalog, name: String) -> DaemonResponse {
    let key = normalize_plugin_key(&name);
    let plugin = catalog
        .list_installed()
        .iter()
        .find(|p| p.manifest.kind == key || p.manifest.name == key);

    match plugin {
        Some(p) => {
            let summary = PluginSummary {
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
            };
            DaemonResponse::PluginList {
                plugins: vec![summary],
            }
        }
        None => DaemonResponse::Error {
            message: "Plugin not found".to_string(),
        },
    }
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

pub fn plugin_view(workspace_manager: &WorkspaceManager, id: u64) -> DaemonResponse {
    if let Some(plugin) = workspace_manager
        .workspace
        .plugins
        .iter()
        .find(|p| p.id == id)
    {
        let variables: Vec<(String, serde_json::Value)> = match plugin.config {
            serde_json::Value::Object(ref map) => {
                map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            }
            _ => Vec::new(),
        };
        DaemonResponse::Ok {
            message: format!("Plugin {} config: {:?}", id, variables),
        }
    } else {
        DaemonResponse::Error {
            message: "Plugin not found".to_string(),
        }
    }
}

pub fn plugin_start(
    workspace_manager: &mut WorkspaceManager,
    id: u64,
    logic_tx: &mpsc::Sender<LogicMessage>,
    refresh_fn: impl Fn(),
) -> DaemonResponse {
    if let Some(plugin) = workspace_manager
        .workspace
        .plugins
        .iter_mut()
        .find(|p| p.id == id)
    {
        plugin.running = true;
        let _ = logic_tx.send(LogicMessage::SetPluginRunning(id, true));
        refresh_fn();
        DaemonResponse::Ok {
            message: "Plugin started".to_string(),
        }
    } else {
        DaemonResponse::Error {
            message: "Plugin not found in runtime".to_string(),
        }
    }
}

pub fn plugin_stop(
    workspace_manager: &mut WorkspaceManager,
    id: u64,
    logic_tx: &mpsc::Sender<LogicMessage>,
    refresh_fn: impl Fn(),
) -> DaemonResponse {
    if let Some(plugin) = workspace_manager
        .workspace
        .plugins
        .iter_mut()
        .find(|p| p.id == id)
    {
        plugin.running = false;
        let _ = logic_tx.send(LogicMessage::SetPluginRunning(id, false));
        refresh_fn();
        DaemonResponse::Ok {
            message: "Plugin stopped".to_string(),
        }
    } else {
        DaemonResponse::Error {
            message: "Plugin not found in runtime".to_string(),
        }
    }
}

pub fn plugin_restart(
    workspace_manager: &WorkspaceManager,
    id: u64,
    logic_tx: &mpsc::Sender<LogicMessage>,
) -> DaemonResponse {
    let exists = workspace_manager
        .workspace
        .plugins
        .iter()
        .any(|p| p.id == id);
    if exists {
        let _ = logic_tx.send(LogicMessage::RestartPlugin(id));
        DaemonResponse::Ok {
            message: "Plugin restarted".to_string(),
        }
    } else {
        DaemonResponse::Error {
            message: "Plugin not found in runtime".to_string(),
        }
    }
}

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
