use clap::{Parser, Subcommand};
use rtsyn_cli::{
    client, daemon,
    protocol::{DaemonRequest, DaemonResponse, DEFAULT_SOCKET_PATH},
};
use rtsyn_gui::{run_gui, GuiConfig};
use std::process::{Command, Stdio};

#[derive(Parser)]
#[command(name = "rtsyn", version, about = "RTSyn MVP CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Daemon {
        #[command(subcommand)]
        command: DaemonCommands,
    },
}

#[derive(Subcommand)]
enum DaemonCommands {
    Run {
        #[arg(long)]
        detach: bool,
    },
    Stop,
    Reload,
    Plugin {
        #[command(subcommand)]
        command: PluginCommands,
    },
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommands,
    },
    Connection {
        #[command(subcommand)]
        command: ConnectionCommands,
    },
    Runtime {
        #[command(subcommand)]
        command: RuntimeCommands,
    },
}

#[derive(Subcommand)]
enum PluginCommands {
    Add {
        name: String,
    },
    Install {
        path: String,
    },
    Reinstall {
        name: String,
    },
    Rebuild {
        name: String,
    },
    Remove {
        id: u64,
    },
    Uninstall {
        name: String,
    },
    Available {
        #[arg(long, alias = "jq")]
        json_query: bool,
    },
    List {
        #[arg(long, alias = "jq")]
        json_query: bool,
    },
    Show {
        id: u64,
    },
    Set {
        id: u64,
        json: String,
    },
    View {
        id: u64,
    },
    Start {
        id: u64,
    },
    Stop {
        id: u64,
    },
    Restart {
        id: u64,
    },
}

#[derive(Subcommand)]
enum WorkspaceCommands {
    List,
    Load { name: String },
    New { name: String },
    Save { name: Option<String> },
    Edit { name: String },
    Delete { name: String },
}

#[derive(Subcommand)]
enum ConnectionCommands {
    List,
    Show {
        plugin_id: u64,
    },
    Add {
        #[arg(long)]
        from_plugin: u64,
        #[arg(long)]
        from_port: String,
        #[arg(long)]
        to_plugin: u64,
        #[arg(long)]
        to_port: String,
        #[arg(long, default_value = "shared_memory")]
        kind: String,
    },
    Remove {
        #[arg(long)]
        from_plugin: u64,
        #[arg(long)]
        from_port: String,
        #[arg(long)]
        to_plugin: u64,
        #[arg(long)]
        to_port: String,
    },
    RemoveIndex {
        index: usize,
    },
}

#[derive(Subcommand)]
enum RuntimeCommands {
    Settings {
        #[command(subcommand)]
        command: RuntimeSettingsCommands,
    },
    UmlDiagram,
}

#[derive(Subcommand)]
enum RuntimeSettingsCommands {
    Show {
        #[arg(long, alias = "jq")]
        json_query: bool,
    },
    Set {
        json: String,
    },
    Save,
    Restore,
    Options {
        #[arg(long, alias = "jq")]
        json_query: bool,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        None => {
            run_gui(GuiConfig::default())?;
        }
        Some(Commands::Daemon { command }) => match command {
            DaemonCommands::Run { detach } => {
                if detach {
                    if let Err(err) = spawn_detached_daemon() {
                        eprintln!("[RTSyn][ERROR]: {err}");
                    } else {
                        println!("[RTSyn][INFO] Daemon started");
                    }
                } else if let Err(err) = daemon::run_daemon() {
                    eprintln!("[RTSyn][ERROR]: {err}");
                }
            }
            DaemonCommands::Stop => {
                let request = DaemonRequest::DaemonStop;
                match client::send_request(&request) {
                    Ok(response) => match response {
                        DaemonResponse::Ok { message } => println!("[RTSyn][INFO] {message}"),
                        DaemonResponse::Error { message } => eprintln!("[RTSyn][ERROR]: {message}"),
                        _ => {}
                    },
                    Err(err) => eprintln!("[RTSyn][ERROR]: {err}"),
                }
            }
            DaemonCommands::Reload => {
                let request = DaemonRequest::DaemonReload;
                match client::send_request(&request) {
                    Ok(response) => match response {
                        DaemonResponse::Ok { message } => println!("[RTSyn][INFO] {message}"),
                        DaemonResponse::Error { message } => eprintln!("[RTSyn][ERROR]: {message}"),
                        _ => {}
                    },
                    Err(err) => eprintln!("[RTSyn][ERROR]: {err}"),
                }
            }
            DaemonCommands::Plugin { command } => {
                let mut available_json_query = false;
                let mut runtime_list_json_query = false;
                let request = match command {
                    PluginCommands::Add { name } => DaemonRequest::PluginAdd { name },
                    PluginCommands::Install { path } => {
                        let install_path = std::path::Path::new(&path);
                        let absolute_path = if install_path.is_absolute() {
                            install_path.to_path_buf()
                        } else {
                            match std::fs::canonicalize(install_path) {
                                Ok(resolved) => resolved,
                                Err(err) => {
                                    eprintln!(
                                        "[RTSyn][ERROR]: Failed to resolve install path: {err}"
                                    );
                                    return Ok(());
                                }
                            }
                        };
                        DaemonRequest::PluginInstall {
                            path: absolute_path.to_string_lossy().to_string(),
                        }
                    }
                    PluginCommands::Reinstall { name } => DaemonRequest::PluginReinstall { name },
                    PluginCommands::Rebuild { name } => DaemonRequest::PluginRebuild { name },
                    PluginCommands::Remove { id } => DaemonRequest::PluginRemove { id },
                    PluginCommands::Uninstall { name } => DaemonRequest::PluginUninstall { name },
                    PluginCommands::Available { json_query } => {
                        available_json_query = json_query;
                        DaemonRequest::PluginList
                    }
                    PluginCommands::List { json_query } => {
                        runtime_list_json_query = json_query;
                        DaemonRequest::RuntimeList
                    }
                    PluginCommands::Show { id } => DaemonRequest::RuntimeShow { id },
                    PluginCommands::Set { id, json } => {
                        DaemonRequest::RuntimeSetVariables { id, json }
                    }
                    PluginCommands::View { id } => {
                        if let Err(err) = spawn_daemon_viewer(id) {
                            eprintln!("[RTSyn][ERROR]: {err}");
                        }
                        return Ok(());
                    }
                    PluginCommands::Start { id } => DaemonRequest::RuntimePluginStart { id },
                    PluginCommands::Stop { id } => DaemonRequest::RuntimePluginStop { id },
                    PluginCommands::Restart { id } => DaemonRequest::RuntimePluginRestart { id },
                };
                match client::send_request(&request) {
                    Ok(response) => match response {
                        DaemonResponse::Ok { message } => println!("[RTSyn][INFO] {message}"),
                        DaemonResponse::Error { message } => eprintln!("[RTSyn][ERROR]: {message}"),
                        DaemonResponse::PluginAdded { id } => {
                            println!("[RTSyn][INFO] Plugin added with id {id}")
                        }
                        DaemonResponse::PluginList { plugins } => {
                            if available_json_query {
                                let json = serde_json::to_string_pretty(&plugins)
                                    .unwrap_or_else(|_| "[]".to_string());
                                println!("{json}");
                                return Ok(());
                            }
                            if plugins.is_empty() {
                                println!("[RTSyn][INFO] No plugins installed");
                            } else {
                                println!("[RTSyn][INFO] List of available plugins:");
                                for plugin in plugins {
                                    let version =
                                        plugin.version.unwrap_or_else(|| "unknown".to_string());
                                    let removable = if plugin.removable {
                                        "removable"
                                    } else {
                                        "bundled"
                                    };
                                    if let Some(path) = plugin.path {
                                        println!(
                                            "{} ({}) [{}] {}",
                                            plugin.name, plugin.kind, removable, path
                                        );
                                    } else {
                                        println!(
                                            "{} ({}) [{}] v{}",
                                            plugin.name, plugin.kind, removable, version
                                        );
                                    }
                                }
                            }
                        }
                        DaemonResponse::RuntimeList { plugins } => {
                            if runtime_list_json_query {
                                let json = serde_json::to_string_pretty(&plugins)
                                    .unwrap_or_else(|_| "[]".to_string());
                                println!("{json}");
                                return Ok(());
                            }
                            if plugins.is_empty() {
                                println!("[RTSyn][INFO] No plugins in runtime");
                            } else {
                                println!("[RTSyn][INFO] Runtime plugins:");
                                for plugin in plugins {
                                    println!("{} ({})", plugin.id, plugin.kind);
                                }
                            }
                        }
                        DaemonResponse::RuntimeShow { id, kind, state } => {
                            println!("[RTSyn][INFO] {id} - {kind}");
                            println!("Variables:");
                            if state.variables.is_empty() {
                                println!("\t(none)");
                            } else {
                                for (name, value) in state.variables {
                                    println!("\t{name}: {value}");
                                }
                            }
                            println!("Outputs:");
                            if state.outputs.is_empty() {
                                println!("\t(none)");
                            } else {
                                for (name, value) in state.outputs {
                                    println!("\t{name}: {value}");
                                }
                            }
                            println!("Inputs:");
                            if state.inputs.is_empty() {
                                println!("\t(none)");
                            } else {
                                for (name, value) in state.inputs {
                                    println!("\t{name}: {value}");
                                }
                            }
                            println!("Internal variables:");
                            if state.internal_variables.is_empty() {
                                println!("\t(none)");
                            } else {
                                for (name, value) in state.internal_variables {
                                    println!("\t{name}: {value}");
                                }
                            }
                        }
                        DaemonResponse::WorkspaceList { .. } => {}
                        DaemonResponse::ConnectionList { .. } => {}
                        DaemonResponse::RuntimePluginView { .. } => {}
                        DaemonResponse::RuntimeSettings { .. } => {}
                        DaemonResponse::RuntimeSettingsOptions { .. } => {}
                        DaemonResponse::RuntimeUmlDiagram { .. } => {}
                    },
                    Err(err) => eprintln!("[RTSyn][ERROR]: {err}"),
                }
            }
            DaemonCommands::Workspace { command } => {
                let request = match command {
                    WorkspaceCommands::List => DaemonRequest::WorkspaceList,
                    WorkspaceCommands::Load { name } => DaemonRequest::WorkspaceLoad { name },
                    WorkspaceCommands::New { name } => DaemonRequest::WorkspaceNew { name },
                    WorkspaceCommands::Save { name } => DaemonRequest::WorkspaceSave { name },
                    WorkspaceCommands::Edit { name } => DaemonRequest::WorkspaceEdit { name },
                    WorkspaceCommands::Delete { name } => DaemonRequest::WorkspaceDelete { name },
                };
                match client::send_request(&request) {
                    Ok(response) => match response {
                        DaemonResponse::Ok { message } => println!("[RTSyn][INFO] {message}"),
                        DaemonResponse::Error { message } => eprintln!("[RTSyn][ERROR]: {message}"),
                        DaemonResponse::WorkspaceList { workspaces } => {
                            if workspaces.is_empty() {
                                println!("[RTSyn][INFO] No workspaces found");
                            } else {
                                println!("[RTSyn][INFO] List of workspaces:");
                                for ws in workspaces {
                                    let plugins =
                                        if ws.plugins == 1 { "plugin" } else { "plugins" };
                                    println!(
                                        "{} - {} {} ({})",
                                        ws.name, ws.plugins, plugins, ws.description
                                    );
                                }
                            }
                        }
                        _ => {}
                    },
                    Err(err) => eprintln!("[RTSyn][ERROR]: {err}"),
                }
            }
            DaemonCommands::Connection { command } => {
                let request = match command {
                    ConnectionCommands::List => DaemonRequest::ConnectionList,
                    ConnectionCommands::Show { plugin_id } => {
                        DaemonRequest::ConnectionShow { plugin_id }
                    }
                    ConnectionCommands::Add {
                        from_plugin,
                        from_port,
                        to_plugin,
                        to_port,
                        kind,
                    } => DaemonRequest::ConnectionAdd {
                        from_plugin,
                        from_port,
                        to_plugin,
                        to_port,
                        kind,
                    },
                    ConnectionCommands::Remove {
                        from_plugin,
                        from_port,
                        to_plugin,
                        to_port,
                    } => DaemonRequest::ConnectionRemove {
                        from_plugin,
                        from_port,
                        to_plugin,
                        to_port,
                    },
                    ConnectionCommands::RemoveIndex { index } => {
                        DaemonRequest::ConnectionRemoveIndex { index }
                    }
                };
                match client::send_request(&request) {
                    Ok(response) => match response {
                        DaemonResponse::Ok { message } => println!("[RTSyn][INFO] {message}"),
                        DaemonResponse::Error { message } => eprintln!("[RTSyn][ERROR]: {message}"),
                        DaemonResponse::ConnectionList { connections } => {
                            if connections.is_empty() {
                                println!("[RTSyn][INFO] No connections found");
                            } else {
                                println!("[RTSyn][INFO] List of connections:");
                                for conn in connections {
                                    println!(
                                        "[{}] {}:{} -> {}:{} ({})",
                                        conn.index,
                                        conn.from_plugin,
                                        conn.from_port,
                                        conn.to_plugin,
                                        conn.to_port,
                                        conn.kind
                                    );
                                }
                            }
                        }
                        _ => {}
                    },
                    Err(err) => eprintln!("[RTSyn][ERROR]: {err}"),
                }
            }
            DaemonCommands::Runtime { command } => {
                let mut settings_json_query = false;
                let request = match command {
                    RuntimeCommands::Settings { command } => match command {
                        RuntimeSettingsCommands::Show { json_query } => {
                            settings_json_query = json_query;
                            DaemonRequest::RuntimeSettingsShow
                        }
                        RuntimeSettingsCommands::Set { json } => {
                            DaemonRequest::RuntimeSettingsSet { json }
                        }
                        RuntimeSettingsCommands::Save => DaemonRequest::RuntimeSettingsSave,
                        RuntimeSettingsCommands::Restore => {
                            DaemonRequest::RuntimeSettingsRestore
                        }
                        RuntimeSettingsCommands::Options { json_query } => {
                            settings_json_query = json_query;
                            DaemonRequest::RuntimeSettingsOptions
                        }
                    },
                    RuntimeCommands::UmlDiagram => DaemonRequest::RuntimeUmlDiagram,
                };
                match client::send_request(&request) {
                    Ok(response) => match response {
                        DaemonResponse::Ok { message } => println!("[RTSyn][INFO] {message}"),
                        DaemonResponse::Error { message } => eprintln!("[RTSyn][ERROR]: {message}"),
                        DaemonResponse::RuntimeSettings { settings } => {
                            if settings_json_query {
                                let json = serde_json::to_string_pretty(&settings)
                                    .unwrap_or_else(|_| "{}".to_string());
                                println!("{json}");
                                return Ok(());
                            }
                            println!("[RTSyn][INFO] Runtime settings:");
                            println!("frequency_value: {}", settings.frequency_value);
                            println!("frequency_unit: {}", settings.frequency_unit);
                            println!("period_value: {}", settings.period_value);
                            println!("period_unit: {}", settings.period_unit);
                            println!("selected_cores: {:?}", settings.selected_cores);
                        }
                        DaemonResponse::RuntimeSettingsOptions { options } => {
                            if settings_json_query {
                                let json = serde_json::to_string_pretty(&options)
                                    .unwrap_or_else(|_| "{}".to_string());
                                println!("{json}");
                                return Ok(());
                            }
                            println!("[RTSyn][INFO] Runtime settings options:");
                            println!("frequency_units: {}", options.frequency_units.join(", "));
                            println!("period_units: {}", options.period_units.join(", "));
                            println!("min_frequency_value: {}", options.min_frequency_value);
                            println!("min_period_value: {}", options.min_period_value);
                            println!(
                                "max_integration_steps: {}..={}",
                                options.max_integration_steps_min,
                                options.max_integration_steps_max
                            );
                        }
                        DaemonResponse::RuntimeUmlDiagram { uml } => {
                            println!("{uml}");
                        }
                        _ => {}
                    },
                    Err(err) => eprintln!("[RTSyn][ERROR]: {err}"),
                }
            }
        },
    }
    Ok(())
}

fn spawn_detached_daemon() -> Result<(), String> {
    let socket_path = std::path::Path::new(DEFAULT_SOCKET_PATH);
    if socket_path.exists() {
        if std::os::unix::net::UnixStream::connect(socket_path).is_ok() {
            return Err("Daemon already running".to_string());
        }
        let _ = std::fs::remove_file(socket_path);
    }

    let exe = std::env::current_exe().map_err(|e| format!("Failed to get executable path: {e}"))?;
    let mut cmd = Command::new(exe);
    cmd.args(["daemon", "run"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to start daemon: {e}"))
}

fn spawn_daemon_viewer(plugin_id: u64) -> Result<(), String> {
    if std::os::unix::net::UnixStream::connect(DEFAULT_SOCKET_PATH).is_err() {
        return Err("Daemon is not running".to_string());
    }
    match client::send_request(&DaemonRequest::RuntimeShow { id: plugin_id }) {
        Ok(DaemonResponse::Error { message }) => {
            if message.contains("Plugin not found") {
                return Err("Plugin not found in runtime".to_string());
            }
        }
        Ok(_) => {}
        Err(err) => return Err(err),
    }
    let exe = std::env::current_exe().map_err(|e| format!("Failed to get executable path: {e}"))?;
    Command::new(exe)
        .env("RTSYN_DAEMON_VIEW_PLUGIN_ID", plugin_id.to_string())
        .env("RTSYN_DAEMON_SOCKET", DEFAULT_SOCKET_PATH)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to open viewer: {e}"))
}
