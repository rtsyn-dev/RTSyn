use clap::{Parser, Subcommand};
use rtsyn_gui::{run_gui, GuiConfig};
use rtsyn_cli::{client, daemon, protocol::{DaemonRequest, DaemonResponse}};

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
    Run,
    Stop,
    Restart,
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
}

#[derive(Subcommand)]
enum PluginCommands {
    Add { name: String },
    Install { path: String },
    Reinstall { name: String },
    Remove { id: u64 },
    Uninstall { name: String },
    List,
}

#[derive(Subcommand)]
enum WorkspaceCommands {
    List,
    Load { name: String },
    New { name: String },
    Save { name: Option<String> },
    Edit { name: String },
}

#[derive(Subcommand)]
enum ConnectionCommands {
    List,
    Show { plugin_id: u64 },
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
    RemoveIndex { index: usize },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        None => {
            run_gui(GuiConfig::default())?;
        }
        Some(Commands::Daemon { command }) => match command {
            DaemonCommands::Run => {
                if let Err(err) = daemon::run_daemon() {
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
            DaemonCommands::Restart => {
                let request = DaemonRequest::DaemonStop;
                let _ = client::send_request(&request);
                if let Err(err) = daemon::run_daemon() {
                    eprintln!("[RTSyn][ERROR]: {err}");
                }
            }
            DaemonCommands::Plugin { command } => {
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
                                eprintln!("[RTSyn][ERROR]: Failed to resolve install path: {err}");
                                return Ok(());
                            }
                        }
                    };
                    DaemonRequest::PluginInstall {
                        path: absolute_path.to_string_lossy().to_string(),
                    }
                    }
                    PluginCommands::Reinstall { name } => DaemonRequest::PluginReinstall { name },
                    PluginCommands::Remove { id } => DaemonRequest::PluginRemove { id },
                    PluginCommands::Uninstall { name } => DaemonRequest::PluginUninstall { name },
                    PluginCommands::List => DaemonRequest::PluginList,
                };
            match client::send_request(&request) {
                    Ok(response) => match response {
                        DaemonResponse::Ok { message } => println!("{message}"),
                        DaemonResponse::Error { message } => eprintln!("[RTSyn][ERROR]: {message}"),
                        DaemonResponse::PluginAdded { id } => println!("[RTSyn][INFO] Plugin added with id {id}"),
                        DaemonResponse::PluginList { plugins } => {
                            if plugins.is_empty() {
                                println!("[RTSyn][INFO] No plugins installed");
                            } else {
                                for plugin in plugins {
                                    let version = plugin.version.unwrap_or_else(|| "unknown".to_string());
                                    let removable = if plugin.removable { "removable" } else { "bundled" };
                                    if let Some(path) = plugin.path {
                                        println!("[RTSyn][INFO] {} ({}) [{}] {}", plugin.name, plugin.kind, removable, path);
                                    } else {
                                        println!("[RTSyn][INFO] {} ({}) [{}] v{}", plugin.name, plugin.kind, removable, version);
                                    }
                                }
                            }
                        }
                        DaemonResponse::WorkspaceList { .. } => {}
                        DaemonResponse::ConnectionList { .. } => {}
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
                };
                match client::send_request(&request) {
                    Ok(response) => match response {
                        DaemonResponse::Ok { message } => println!("[RTSyn][INFO] {message}"),
                        DaemonResponse::Error { message } => eprintln!("[RTSyn][ERROR]: {message}"),
                        DaemonResponse::WorkspaceList { workspaces } => {
                            if workspaces.is_empty() {
                                println!("[RTSyn][INFO] No workspaces found");
                            } else {
                                for ws in workspaces {
                                    let plugins = if ws.plugins == 1 { "plugin" } else { "plugins" };
                                    println!(
                                        "[RTSyn][INFO] [{}] {} - {} {} ({})",
                                        ws.index,
                                        ws.name,
                                        ws.plugins,
                                        plugins,
                                        ws.description
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
                                for conn in connections {
                                    println!(
                                        "[RTSyn][INFO] [{}] {}:{} -> {}:{} ({})",
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
        },
    }
    Ok(())
}
