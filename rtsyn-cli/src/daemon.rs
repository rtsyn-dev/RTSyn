use crate::protocol::{ConnectionSummary, DaemonRequest, DaemonResponse, PluginSummary, WorkspaceSummary, RuntimePluginSummary, RuntimePluginState, DEFAULT_SOCKET_PATH};
use crate::protocol::RuntimeSettingsOptions;
use rtsyn_core::connection::next_available_extendable_input_index;
use rtsyn_core::plugin::{is_extendable_inputs, InstalledPlugin, PluginCatalog, PluginMetadataSource};
use rtsyn_core::workspace::WorkspaceManager;
use rtsyn_runtime::runtime::{spawn_runtime, LogicMessage, LogicState};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

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
    workspace_manager: WorkspaceManager,
    runtime_query: RuntimeQuery,
    logic_state_rx: mpsc::Receiver<LogicState>,
    logic_settings: rtsyn_runtime::runtime::LogicSettings,
    last_logic_state: Option<LogicState>,
    plotter_history: std::collections::HashMap<u64, Vec<(u64, Vec<f64>)>>,
}

impl DaemonState {
    fn new(
        install_db_path: PathBuf,
        workspace_dir: PathBuf,
        logic_tx: mpsc::Sender<LogicMessage>,
        logic_state_rx: mpsc::Receiver<LogicState>,
    ) -> Self {
        let mut catalog = PluginCatalog::new(install_db_path);
        let workspace_manager = WorkspaceManager::new(workspace_dir);
        catalog.sync_ids_from_workspace(&workspace_manager.workspace);
        let logic_settings = rtsyn_runtime::runtime::LogicSettings {
            cores: vec![0],
            period_seconds: 0.001,
            time_scale: 1000.0,
            time_label: "time_ms".to_string(),
            ui_hz: 60.0,
            max_integration_steps: 10,
        };
        Self {
            catalog,
            workspace_manager,
            runtime_query: RuntimeQuery { logic_tx },
            logic_state_rx,
            logic_settings,
            last_logic_state: None,
            plotter_history: std::collections::HashMap::new(),
        }
    }

    fn refresh_runtime(&self) {
        let _ = self
            .runtime_query
            .logic_tx
            .send(LogicMessage::UpdateWorkspace(self.workspace_manager.workspace.clone()));
    }

    fn drain_logic_states(&mut self) {
        while let Ok(state_msg) = self.logic_state_rx.try_recv() {
            for (plugin_id, samples) in &state_msg.plotter_samples {
                let entry = self.plotter_history.entry(*plugin_id).or_default();
                entry.extend(samples.iter().cloned());
                if entry.len() > 5000 {
                    let excess = entry.len() - 5000;
                    entry.drain(0..excess);
                }
            }
            self.last_logic_state = Some(state_msg);
        }
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
    let (logic_tx, logic_state_rx) = spawn_runtime().map_err(|e| e.to_string())?;

    let install_db_path = PathBuf::from("app_plugins").join("installed_plugins.json");
    let workspace_dir = PathBuf::from("app_workspaces");
    let mut state = DaemonState::new(install_db_path, workspace_dir, logic_tx.clone(), logic_state_rx);
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
        DaemonRequest::PluginRebuild { name } => {
            let key = normalize_plugin_key(&name);
            match state.catalog.rebuild_plugin_by_kind(&key) {
                Ok(()) => DaemonResponse::Ok {
                    message: "Plugin rebuilt".to_string(),
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
                    .remove_plugins_by_kind_from_workspace(
                        &plugin.manifest.kind,
                        &mut state.workspace_manager.workspace,
                    );
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
                .add_installed_plugin_to_workspace(
                    &key,
                    &mut state.workspace_manager.workspace,
                    &state.runtime_query,
                )
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
            .remove_plugin_from_workspace(id, &mut state.workspace_manager.workspace)
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
            state.workspace_manager.scan_workspaces();
            let workspaces = state
                .workspace_manager
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
        DaemonRequest::WorkspaceLoad { name } => {
            let path = state.workspace_manager.workspace_file_path(&name);
            match state.workspace_manager.load_workspace(&path) {
                Ok(()) => {
                    let mut workspace = state.workspace_manager.workspace.clone();
                    state.catalog.refresh_library_paths();
                    state.catalog.inject_library_paths_into_workspace(&mut workspace);
                    state.catalog.sync_ids_from_workspace(&workspace);
                    state.workspace_manager.workspace = workspace;
                    state.refresh_runtime();
                    DaemonResponse::Ok {
                        message: format!("Workspace '{}' loaded", name),
                    }
                }
                Err(err) => DaemonResponse::Error { message: err },
            }
        }
        DaemonRequest::WorkspaceNew { name } => {
            match state
                .workspace_manager
                .create_workspace(&name, "")
            {
                Ok(()) => {
                    state
                        .catalog
                        .sync_ids_from_workspace(&state.workspace_manager.workspace);
                    state.refresh_runtime();
                    DaemonResponse::Ok {
                        message: "Workspace created".to_string(),
                    }
                }
                Err(err) => DaemonResponse::Error { message: err },
            }
        }
        DaemonRequest::WorkspaceSave { name } => {
            let result = match name.as_ref() {
                Some(name) => {
                    let description = state.workspace_manager.workspace.description.clone();
                    state
                        .workspace_manager
                        .save_workspace_as(name, &description)
                }
                None => state.workspace_manager.save_workspace_overwrite_current(),
            };
            match result {
                Ok(()) => {
                    state.refresh_runtime();
                    DaemonResponse::Ok {
                        message: "Workspace saved".to_string(),
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
        DaemonRequest::WorkspaceEdit { name } => {
            match state.workspace_manager.rename_workspace(&name) {
                Ok(()) => {
                    state.refresh_runtime();
                    DaemonResponse::Ok {
                        message: "Workspace updated".to_string(),
                    }
                }
                Err(err) => DaemonResponse::Error { message: err },
            }
        }
        DaemonRequest::WorkspaceDelete { name } => {
            match state.workspace_manager.delete_workspace(&name) {
                Ok(()) => {
                    state.refresh_runtime();
                    DaemonResponse::Ok {
                        message: "Workspace deleted".to_string(),
                    }
                }
                Err(err) => DaemonResponse::Error { message: err },
            }
        }
        DaemonRequest::ConnectionList => {
            let connections = state
                .workspace_manager
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
                .workspace_manager
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
            let from_exists = state
                .workspace_manager
                .workspace
                .plugins
                .iter()
                .any(|p| p.id == from_plugin);
            if !from_exists {
                DaemonResponse::Error {
                    message: "Source plugin not found in workspace".to_string(),
                }
            } else {
                let from_kind = state
                    .workspace_manager
                    .workspace
                    .plugins
                    .iter()
                    .find(|p| p.id == from_plugin)
                    .map(|p| p.kind.clone())
                    .unwrap_or_default();
                let to_plugin_def = state
                    .workspace_manager
                    .workspace
                    .plugins
                    .iter()
                    .find(|p| p.id == to_plugin)
                    .cloned();
                let to_exists = to_plugin_def.is_some();
                if !to_exists {
                    DaemonResponse::Error {
                        message: "Target plugin not found in workspace".to_string(),
                    }
                } else if from_port.trim().is_empty()
                    || to_port.trim().is_empty()
                    || kind.trim().is_empty()
                {
                    DaemonResponse::Error {
                        message: "Connection fields cannot be empty".to_string(),
                    }
                } else {
                    let installed = &state.catalog.manager.installed_plugins;
                    let from_outputs = plugin_outputs(installed, &from_kind);
                    if from_outputs.is_empty() {
                        DaemonResponse::Error {
                            message: "Source plugin outputs not available".to_string(),
                        }
                    } else if !from_outputs.iter().any(|p| p == &from_port) {
                        DaemonResponse::Error {
                            message: "Source port not found".to_string(),
                        }
                    } else {
                        let to_kind = to_plugin_def
                            .as_ref()
                            .map(|p| p.kind.clone())
                            .unwrap_or_default();
                        let to_inputs = plugin_inputs(installed, &to_kind);
                    if is_extendable_inputs(&to_kind) {
                        let next_idx = next_available_extendable_input_index(
                            &state.workspace_manager.workspace,
                            to_plugin,
                        );
                        let next_port = format!("in_{next_idx}");
                        if !to_inputs.iter().any(|p| p == &to_port) && to_port != next_port {
                            DaemonResponse::Error {
                                message:
                                    "Target port must be the next in_<number> or an existing input"
                                        .to_string(),
                            }
                        } else {
                            match rtsyn_core::connection::add_connection(
                                &mut state.workspace_manager.workspace,
                                &state.catalog.manager.installed_plugins,
                                from_plugin,
                                &from_port,
                                to_plugin,
                                &to_port,
                                &kind,
                            ) {
                                Ok(()) => {
                                    state.refresh_runtime();
                                    DaemonResponse::Ok {
                                        message: "Connection added".to_string(),
                                    }
                                }
                                Err(err) => DaemonResponse::Error {
                                    message: format!("{err}"),
                                },
                            }
                        }
                    } else if to_inputs.is_empty() {
                            DaemonResponse::Error {
                                message: "Target plugin inputs not available".to_string(),
                            }
                        } else if !to_inputs.iter().any(|p| p == &to_port) {
                            DaemonResponse::Error {
                                message: "Target port not found".to_string(),
                            }
                        } else {
                            match rtsyn_core::connection::add_connection(
                                &mut state.workspace_manager.workspace,
                                &state.catalog.manager.installed_plugins,
                                from_plugin,
                                &from_port,
                                to_plugin,
                                &to_port,
                                &kind,
                            ) {
                                Ok(()) => {
                                    state.refresh_runtime();
                                    DaemonResponse::Ok {
                                        message: "Connection added".to_string(),
                                    }
                                }
                                Err(err) => DaemonResponse::Error {
                                    message: format!("{err}"),
                                },
                            }
                        }
                    }
                }
            }
        }
        DaemonRequest::ConnectionRemove {
            from_plugin,
            from_port,
            to_plugin,
            to_port,
        } => {
            let index = state
                .workspace_manager
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
                    state.workspace_manager.workspace.connections.remove(idx);
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
            if index >= state.workspace_manager.workspace.connections.len() {
                DaemonResponse::Error {
                    message: "Invalid connection index".to_string(),
                }
            } else {
                state.workspace_manager.workspace.connections.remove(index);
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
            let workspace_dir = state.workspace_manager.workspace_dir().to_path_buf();
            state.workspace_manager = WorkspaceManager::new(workspace_dir);
            state.catalog.sync_ids_from_workspace(&state.workspace_manager.workspace);
            state.refresh_runtime();
            DaemonResponse::Ok {
                message: "Daemon reloaded".to_string(),
            }
        }
        DaemonRequest::RuntimeList => {
            let plugins = state
                .workspace_manager
                .workspace
                .plugins
                .iter()
                .map(|plugin| RuntimePluginSummary {
                    id: plugin.id,
                    kind: plugin.kind.clone(),
                })
                .collect();
            DaemonResponse::RuntimeList { plugins }
        }
        DaemonRequest::RuntimeSettingsShow => DaemonResponse::RuntimeSettings {
            settings: state.workspace_manager.workspace.settings.clone(),
        },
        DaemonRequest::RuntimeSettingsOptions => DaemonResponse::RuntimeSettingsOptions {
            options: RuntimeSettingsOptions {
                frequency_units: vec!["hz".to_string(), "khz".to_string(), "mhz".to_string()],
                period_units: vec!["ns".to_string(), "us".to_string(), "ms".to_string(), "s".to_string()],
                min_frequency_value: 1.0,
                min_period_value: 1.0,
                max_integration_steps_min: 1,
                max_integration_steps_max: 100,
            },
        },
        DaemonRequest::RuntimeSettingsSet { json } => {
            match state.workspace_manager.apply_runtime_settings_json(&json) {
                Ok(()) => match state.workspace_manager.runtime_settings() {
                    Ok(runtime_settings) => {
                        state.logic_settings.cores = runtime_settings.cores;
                        state.logic_settings.period_seconds = runtime_settings.period_seconds;
                        state.logic_settings.time_scale = runtime_settings.time_scale;
                        state.logic_settings.time_label = runtime_settings.time_label;
                        let _ = state
                            .runtime_query
                            .logic_tx
                            .send(LogicMessage::UpdateSettings(state.logic_settings.clone()));
                        state.refresh_runtime();
                        DaemonResponse::Ok {
                            message: "Runtime settings updated".to_string(),
                        }
                    }
                    Err(err) => DaemonResponse::Error { message: err },
                },
                Err(err) => DaemonResponse::Error { message: err },
            }
        }
        DaemonRequest::RuntimeShow { id } => {
            state.drain_logic_states();
            let latest_state = state.last_logic_state.clone();
            match build_runtime_state(&state.workspace_manager, id, latest_state) {
                Ok((kind, state, _)) => DaemonResponse::RuntimeShow { id, kind, state },
                Err(message) => DaemonResponse::Error { message },
            }
        }
        DaemonRequest::RuntimePluginView { id } => {
            state.drain_logic_states();
            let latest_state = state.last_logic_state.clone();
            let samples = state.plotter_history.get(&id).cloned().unwrap_or_default();
            match build_runtime_state(&state.workspace_manager, id, latest_state) {
                Ok((kind, plugin_state, _)) => DaemonResponse::RuntimePluginView {
                    id,
                    kind,
                    state: plugin_state,
                    samples,
                    period_seconds: state.logic_settings.period_seconds,
                    time_scale: state.logic_settings.time_scale,
                    time_label: state.logic_settings.time_label.clone(),
                },
                Err(message) => DaemonResponse::Error { message },
            }
        }
        DaemonRequest::RuntimePluginStart { id } => {
            if let Some(plugin) = state
                .workspace_manager
                .workspace
                .plugins
                .iter_mut()
                .find(|p| p.id == id)
            {
                plugin.running = true;
                let _ = state
                    .runtime_query
                    .logic_tx
                    .send(LogicMessage::SetPluginRunning(id, true));
                state.refresh_runtime();
                DaemonResponse::Ok {
                    message: "Plugin started".to_string(),
                }
            } else {
                DaemonResponse::Error {
                    message: "Plugin not found in runtime".to_string(),
                }
            }
        }
        DaemonRequest::RuntimePluginStop { id } => {
            if let Some(plugin) = state
                .workspace_manager
                .workspace
                .plugins
                .iter_mut()
                .find(|p| p.id == id)
            {
                plugin.running = false;
                let _ = state
                    .runtime_query
                    .logic_tx
                    .send(LogicMessage::SetPluginRunning(id, false));
                state.refresh_runtime();
                DaemonResponse::Ok {
                    message: "Plugin stopped".to_string(),
                }
            } else {
                DaemonResponse::Error {
                    message: "Plugin not found in runtime".to_string(),
                }
            }
        }
        DaemonRequest::RuntimePluginRestart { id } => {
            let exists = state
                .workspace_manager
                .workspace
                .plugins
                .iter()
                .any(|p| p.id == id);
            if !exists {
                DaemonResponse::Error {
                    message: "Plugin not found in runtime".to_string(),
                }
            } else {
                let _ = state
                    .runtime_query
                    .logic_tx
                    .send(LogicMessage::RestartPlugin(id));
                DaemonResponse::Ok {
                    message: "Plugin restarted".to_string(),
                }
            }
        }
        DaemonRequest::RuntimeSetVariables { id, json } => {
            if let Some(plugin) = state
                .workspace_manager
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
                                    plugin.config =
                                        serde_json::Value::Object(serde_json::Map::new());
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
                                        let _ = state.runtime_query.logic_tx.send(
                                            LogicMessage::SetPluginVariable(
                                                id,
                                                key.clone(),
                                                val.clone(),
                                            ),
                                        );
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

fn build_runtime_state(
    manager: &WorkspaceManager,
    id: u64,
    latest_state: Option<LogicState>,
) -> Result<(String, RuntimePluginState, Vec<(u64, Vec<f64>)>), String> {
    let plugin = manager.workspace.plugins.iter().find(|p| p.id == id);
    match (plugin, latest_state) {
        (None, _) => Err("Plugin not found in runtime".to_string()),
        (_, None) => Err("No runtime state available".to_string()),
        (Some(plugin), Some(latest_state)) => {
            let kind = plugin.kind.clone();
            let mut variables: Vec<(String, serde_json::Value)> = match plugin.config {
                serde_json::Value::Object(ref map) => {
                    map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                }
                _ => Vec::new(),
            };
            variables.sort_by(|a, b| a.0.cmp(&b.0));
            let mut outputs: Vec<(String, f64)> = latest_state
                .outputs
                .iter()
                .filter(|((pid, _), _)| *pid == id)
                .map(|((_, name), value)| (name.clone(), *value))
                .collect();
            outputs.sort_by(|a, b| a.0.cmp(&b.0));
            let mut inputs: Vec<(String, f64)> = latest_state
                .input_values
                .iter()
                .filter(|((pid, _), _)| *pid == id)
                .map(|((_, name), value)| (name.clone(), *value))
                .collect();
            inputs.sort_by(|a, b| a.0.cmp(&b.0));
            let mut internals: Vec<(String, serde_json::Value)> = latest_state
                .internal_variable_values
                .iter()
                .filter(|((pid, _), _)| *pid == id)
                .map(|((_, name), value)| (name.clone(), value.clone()))
                .collect();
            internals.sort_by(|a, b| a.0.cmp(&b.0));
            Ok((
                kind,
                RuntimePluginState {
                    outputs,
                    inputs,
                    internal_variables: internals,
                    variables,
                },
                Vec::new(),
            ))
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
