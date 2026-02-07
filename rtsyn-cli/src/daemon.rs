use crate::protocol::{ConnectionSummary, DaemonRequest, DaemonResponse, PluginSummary, WorkspaceSummary, DEFAULT_SOCKET_PATH};
use rtsyn_core::plugins::{is_extendable_inputs, PluginCatalog, PluginMetadataSource};
use rtsyn_core::connections::{ensure_extendable_input_count, next_available_extendable_input_index};
use rtsyn_core::workspaces::{empty_workspace, scan_workspace_entries, workspace_file_path, load_workspace, save_workspace, rename_workspace_file};
use rtsyn_runtime::runtime::{spawn_runtime, LogicMessage};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use workspace::WorkspaceDefinition;
use workspace::ConnectionDefinition;

struct RuntimeQuery {
    logic_tx: mpsc::Sender<LogicMessage>,
}

impl PluginMetadataSource for RuntimeQuery {
    fn query_plugin_metadata(
        &self,
        library_path: &str,
        timeout: Duration,
    ) -> Option<(Vec<String>, Vec<String>, Vec<(String, f64)>, Option<rtsyn_plugin::ui::DisplaySchema>, Option<rtsyn_plugin::ui::UISchema>)> {
        let (tx, rx) = mpsc::channel();
        let _ = self
            .logic_tx
            .send(LogicMessage::QueryPluginMetadata(library_path.to_string(), tx));
        rx.recv_timeout(timeout).ok().flatten()
    }

    fn query_plugin_behavior(
        &self,
        kind: &str,
        library_path: Option<&str>,
        timeout: Duration,
    ) -> Option<rtsyn_plugin::ui::PluginBehavior> {
        let (tx, rx) = mpsc::channel();
        let _ = self.logic_tx.send(LogicMessage::QueryPluginBehavior(
            kind.to_string(),
            library_path.map(|s| s.to_string()),
            tx,
        ));
        rx.recv_timeout(timeout).ok().flatten()
    }
}

struct DaemonState {
    catalog: PluginCatalog,
    workspace: WorkspaceDefinition,
    workspace_path: Option<PathBuf>,
    workspace_dir: PathBuf,
    runtime_query: RuntimeQuery,
}

impl DaemonState {
    fn new(install_db_path: PathBuf, workspace_dir: PathBuf, logic_tx: mpsc::Sender<LogicMessage>) -> Self {
        let mut catalog = PluginCatalog::new(install_db_path);
        let workspace = empty_workspace("default");
        catalog.sync_ids_from_workspace(&workspace);
        Self {
            catalog,
            workspace,
            workspace_path: None,
            workspace_dir,
            runtime_query: RuntimeQuery { logic_tx },
        }
    }

    fn refresh_runtime(&self) {
        let _ = self
            .runtime_query
            .logic_tx
            .send(LogicMessage::UpdateWorkspace(self.workspace.clone()));
    }
}

pub fn run_daemon() -> Result<(), String> {
    run_daemon_at(DEFAULT_SOCKET_PATH)
}

pub fn run_daemon_at(socket_path: &str) -> Result<(), String> {
    if std::path::Path::new(socket_path).exists() {
        if UnixStream::connect(socket_path).is_ok() {
            return Err("Daemon already running".to_string());
        }
        let _ = std::fs::remove_file(socket_path);
    }
    let (logic_tx, _logic_state_rx) = spawn_runtime().map_err(|e| e.to_string())?;

    let install_db_path = PathBuf::from("app_plugins").join("installed_plugins.json");
    let workspace_dir = PathBuf::from("app_workspaces");
    let mut state = DaemonState::new(install_db_path, workspace_dir, logic_tx.clone());
    state.refresh_runtime();

    let listener = UnixListener::bind(socket_path)
        .map_err(|e| format!("Failed to bind daemon socket: {e}"))?;

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                match handle_client(stream, &mut state) {
                    Ok(()) => {}
                    Err(err) if err == "daemon_stop" => {
                        let _ = std::fs::remove_file(socket_path);
                        return Ok(());
                    }
                    Err(err) => {
                        eprintln!("[RTSyn][ERROR]: Daemon client error: {err}");
                    }
                }
            }
            Err(err) => {
                eprintln!("[RTSyn][ERROR]: Daemon accept error: {err}");
            }
        }
    }

    Ok(())
}

fn handle_client(stream: UnixStream, state: &mut DaemonState) -> Result<(), String> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let bytes = reader.read_line(&mut line).map_err(|e| e.to_string())?;
    if bytes == 0 || line.trim().is_empty() {
        return Ok(());
    }
    let request: DaemonRequest =
        serde_json::from_str(line.trim()).map_err(|e| e.to_string())?;
    let mut stream = reader.into_inner();

    let response = match request {
        DaemonRequest::PluginList => {
            let plugins = state
                .catalog
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
        DaemonRequest::PluginInstall { path } => {
            let install_path = PathBuf::from(&path);
            if !install_path.is_absolute() {
                DaemonResponse::Error {
                    message: "Plugin install path must be absolute".to_string(),
                }
            } else {
                let resolved = std::fs::canonicalize(&install_path)
                    .unwrap_or(install_path);
                match state.catalog.install_plugin_from_folder(
                    resolved,
                    true,
                    true,
                    &state.runtime_query,
                ) {
                    Ok(()) => DaemonResponse::Ok {
                        message: "Plugin installed".to_string(),
                    },
                    Err(err) => DaemonResponse::Error { message: err },
                }
            }
        }
        DaemonRequest::PluginReinstall { name } => {
            let key = normalize_plugin_key(&name);
            match state.catalog.reinstall_plugin_by_kind(&key, &state.runtime_query) {
                Ok(()) => DaemonResponse::Ok {
                    message: "Plugin reinstalled".to_string(),
                },
                Err(err) => DaemonResponse::Error { message: err },
            }
        }
        DaemonRequest::PluginUninstall { name } => {
            let key = normalize_plugin_key(&name);
            match state.catalog.uninstall_plugin_by_kind(&key) {
            Ok(plugin) => {
                let removed_ids = state
                    .catalog
                    .remove_plugins_by_kind_from_workspace(&plugin.manifest.kind, &mut state.workspace);
                if !removed_ids.is_empty() {
                    state.refresh_runtime();
                }
                DaemonResponse::Ok {
                    message: "Plugin uninstalled".to_string(),
                }
            }
            Err(err) => DaemonResponse::Error { message: err },
        }
        },
        DaemonRequest::PluginAdd { name } => {
            let key = normalize_plugin_key(&name);
            match state
                .catalog
                .add_installed_plugin_to_workspace(&key, &mut state.workspace, &state.runtime_query)
        {
            Ok(id) => {
                state.refresh_runtime();
                DaemonResponse::PluginAdded { id }
            }
            Err(err) => DaemonResponse::Error { message: err },
        }
        },
        DaemonRequest::PluginRemove { id } => match state
            .catalog
            .remove_plugin_from_workspace(id, &mut state.workspace)
        {
            Ok(()) => {
                state.refresh_runtime();
                DaemonResponse::Ok {
                    message: "Plugin removed".to_string(),
                }
            }
            Err(err) => DaemonResponse::Error { message: err },
        },
        DaemonRequest::WorkspaceList => {
            let entries = scan_workspace_entries(&state.workspace_dir);
            let workspaces = entries
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
        DaemonRequest::WorkspaceLoad { name } => {
            let path = workspace_file_path(&state.workspace_dir, &name);
            match load_workspace(&path) {
                Ok(mut workspace) => {
                    state.catalog.refresh_library_paths();
                    state.catalog.inject_library_paths_into_workspace(&mut workspace);
                    state.catalog.sync_ids_from_workspace(&workspace);
                    state.workspace = workspace;
                    state.workspace_path = Some(path);
                    state.refresh_runtime();
                    DaemonResponse::Ok {
                        message: format!("Workspace '{}' loaded", name),
                    }
                }
                Err(err) => DaemonResponse::Error { message: err },
            }
        }
        DaemonRequest::WorkspaceNew { name } => {
            let path = workspace_file_path(&state.workspace_dir, &name);
            if path.exists() {
                DaemonResponse::Error {
                    message: "Workspace already exists".to_string(),
                }
            } else {
            let workspace = empty_workspace(&name);
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match save_workspace(&workspace, &path) {
                    Ok(()) => {
                        state.catalog.sync_ids_from_workspace(&workspace);
                        state.workspace = workspace;
                        state.workspace_path = Some(path);
                        state.refresh_runtime();
                        DaemonResponse::Ok {
                            message: "Workspace created".to_string(),
                        }
                    }
                    Err(err) => DaemonResponse::Error { message: err },
                }
            }
        }
        DaemonRequest::WorkspaceSave { name } => {
            let target_path = match name.as_ref() {
                Some(name) => Some(workspace_file_path(&state.workspace_dir, name)),
                None => state.workspace_path.clone(),
            };
            match target_path {
                Some(target_path) => {
                    if let Some(parent) = target_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let mut workspace = state.workspace.clone();
                    if let Some(name) = name {
                        workspace.name = name.clone();
                    }
                    match save_workspace(&workspace, &target_path) {
                        Ok(()) => {
                            state.workspace = workspace;
                            state.workspace_path = Some(target_path);
                            state.refresh_runtime();
                            DaemonResponse::Ok {
                                message: "Workspace saved".to_string(),
                            }
                        }
                        Err(err) => DaemonResponse::Error { message: err },
                    }
                }
                None => DaemonResponse::Error {
                    message: "No workspace loaded. Use 'rtsyn daemon workspace save <name>' to save the current workspace.".to_string(),
                },
            }
        }
        DaemonRequest::WorkspaceEdit { name } => {
            match state.workspace_path.clone() {
                Some(current_path) => {
                    let new_path = workspace_file_path(&state.workspace_dir, &name);
                    let mut workspace = state.workspace.clone();
                    workspace.name = name.clone();
                    match save_workspace(&workspace, &new_path) {
                        Ok(()) => {
                            let _ = rename_workspace_file(&current_path, &new_path);
                            state.workspace = workspace;
                            state.workspace_path = Some(new_path);
                            state.refresh_runtime();
                            DaemonResponse::Ok {
                                message: "Workspace updated".to_string(),
                            }
                        }
                        Err(err) => DaemonResponse::Error { message: err },
                    }
                }
                None => DaemonResponse::Error {
                    message: "No workspace loaded to edit".to_string(),
                },
            }
        }
        DaemonRequest::ConnectionList => {
            let connections = state
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
        DaemonRequest::ConnectionShow { plugin_id } => {
            let connections = state
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
        DaemonRequest::ConnectionAdd {
            from_plugin,
            from_port,
            to_plugin,
            to_port,
            kind,
        } => {
            let mut to_port_string = to_port.clone();
            if let Some(target) = state.workspace.plugins.iter().find(|p| p.id == to_plugin) {
                if is_extendable_inputs(&target.kind) && to_port_string == "in" {
                    let next_idx = next_available_extendable_input_index(&state.workspace, to_plugin);
                    to_port_string = format!("in_{next_idx}");
                }
            }

            let connection = ConnectionDefinition {
                from_plugin,
                from_port,
                to_plugin,
                to_port: to_port_string.clone(),
                kind,
            };
            match workspace::add_connection(&mut state.workspace.connections, connection, 1) {
                Ok(()) => {
                    if let Some(idx) = to_port_string.strip_prefix("in_").and_then(|v| v.parse::<usize>().ok()) {
                        ensure_extendable_input_count(&mut state.workspace, to_plugin, idx + 1);
                    }
                    state.refresh_runtime();
                    DaemonResponse::Ok {
                        message: "Connection added".to_string(),
                    }
                }
                Err(err) => DaemonResponse::Error { message: format!("{err}") },
            }
        }
        DaemonRequest::ConnectionRemove {
            from_plugin,
            from_port,
            to_plugin,
            to_port,
        } => {
            let index = state.workspace.connections.iter().position(|conn| {
                conn.from_plugin == from_plugin
                    && conn.from_port == from_port
                    && conn.to_plugin == to_plugin
                    && conn.to_port == to_port
            });
            match index {
                Some(idx) => {
                    state.workspace.connections.remove(idx);
                    state.refresh_runtime();
                    DaemonResponse::Ok {
                        message: "Connection removed".to_string(),
                    }
                }
                None => DaemonResponse::Error {
                    message: "Connection not found".to_string(),
                },
            }
        }
        DaemonRequest::ConnectionRemoveIndex { index } => {
            if index >= state.workspace.connections.len() {
                DaemonResponse::Error {
                    message: "Invalid connection index".to_string(),
                }
            } else {
                state.workspace.connections.remove(index);
                state.refresh_runtime();
                DaemonResponse::Ok {
                    message: "Connection removed".to_string(),
                }
            }
        }
        DaemonRequest::DaemonStop => {
            let response = DaemonResponse::Ok {
                message: "Daemon stopping".to_string(),
            };
            send_response(&mut stream, &response)?;
            return Err("daemon_stop".to_string());
        }
        DaemonRequest::DaemonReload => {
            state.catalog.manager.load_installed_plugins();
            state.catalog.refresh_library_paths();
            state.catalog.scan_detected_plugins();
            state.workspace = empty_workspace("default");
            state.workspace_path = None;
            state.catalog.sync_ids_from_workspace(&state.workspace);
            state.refresh_runtime();
            DaemonResponse::Ok {
                message: "Daemon reloaded".to_string(),
            }
        }
    };

    send_response(&mut stream, &response)?;
    Ok(())
}

fn send_response(stream: &mut impl Write, response: &DaemonResponse) -> Result<(), String> {
    let payload = serde_json::to_string(response).map_err(|e| e.to_string())?;
    stream
        .write_all(format!("{payload}\n").as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(())
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
