use crate::protocol::{DaemonRequest, DaemonResponse, PluginSummary, DEFAULT_SOCKET_PATH};
use rtsyn_core::plugins::{empty_workspace, PluginCatalog, PluginMetadataSource};
use rtsyn_runtime::runtime::{spawn_runtime, LogicMessage};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use workspace::WorkspaceDefinition;

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
    runtime_query: RuntimeQuery,
}

impl DaemonState {
    fn new(install_db_path: PathBuf, logic_tx: mpsc::Sender<LogicMessage>) -> Self {
        let mut catalog = PluginCatalog::new(install_db_path);
        let workspace = empty_workspace();
        catalog.sync_ids_from_workspace(&workspace);
        Self {
            catalog,
            workspace,
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
    let (logic_tx, _logic_state_rx) = spawn_runtime().map_err(|e| e.to_string())?;

    let install_db_path = PathBuf::from("app_plugins").join("installed_plugins.json");
    let mut state = DaemonState::new(install_db_path, logic_tx.clone());
    state.refresh_runtime();

    if std::path::Path::new(socket_path).exists() {
        let _ = std::fs::remove_file(socket_path);
    }
    let listener = UnixListener::bind(socket_path)
        .map_err(|e| format!("Failed to bind daemon socket: {e}"))?;

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(err) = handle_client(stream, &mut state) {
                    eprintln!("Daemon client error: {err}");
                }
            }
            Err(err) => {
                eprintln!("Daemon accept error: {err}");
            }
        }
    }

    Ok(())
}

fn handle_client(stream: UnixStream, state: &mut DaemonState) -> Result<(), String> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
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
