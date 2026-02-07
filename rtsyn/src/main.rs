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
    Add { name: String },
    Install { path: String },
    Reinstall { name: String },
    Rebuild { name: String },
    Remove { id: u64 },
    Uninstall { name: String },
    List {
        #[arg(long, alias = "jq")]
        json_query: bool,
    },
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

#[derive(Subcommand)]
enum RuntimeCommands {
    Add { name: String },
    Remove { id: u64 },
    List {
        #[arg(long, alias = "jq")]
        json_query: bool,
    },
    Show { id: u64 },
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
                let mut list_json_query = false;
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
                    PluginCommands::Rebuild { name } => DaemonRequest::PluginRebuild { name },
                    PluginCommands::Remove { id } => DaemonRequest::PluginRemove { id },
                    PluginCommands::Uninstall { name } => DaemonRequest::PluginUninstall { name },
                    PluginCommands::List { json_query } => {
                        list_json_query = json_query;
                        DaemonRequest::PluginList
                    }
                };
            match client::send_request(&request) {
                    Ok(response) => match response {
                        DaemonResponse::Ok { message } => println!("[RTSyn][INFO] {message}"),
                        DaemonResponse::Error { message } => eprintln!("[RTSyn][ERROR]: {message}"),
                        DaemonResponse::PluginAdded { id } => println!("[RTSyn][INFO] Plugin added with id {id}"),
                        DaemonResponse::PluginList { plugins } => {
                            if list_json_query {
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
                                    let version = plugin.version.unwrap_or_else(|| "unknown".to_string());
                                    let removable = if plugin.removable { "removable" } else { "bundled" };
                                    if let Some(path) = plugin.path {
                                        println!("{} ({}) [{}] {}", plugin.name, plugin.kind, removable, path);
                                    } else {
                                        println!("{} ({}) [{}] v{}", plugin.name, plugin.kind, removable, version);
                                    }
                                }
                            }
                        }
                        DaemonResponse::WorkspaceList { .. } => {}
                        DaemonResponse::ConnectionList { .. } => {}
                        DaemonResponse::RuntimeList { .. } => {}
                        DaemonResponse::RuntimeShow { .. } => {}
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
                                println!("[RTSyn][INFO] List of workspaces:");
                                for ws in workspaces {
                                    let plugins = if ws.plugins == 1 { "plugin" } else { "plugins" };
                                    println!(
                                        "[{}] {} - {} {} ({})",
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
                let mut list_json_query = false;
                let request = match command {
                    RuntimeCommands::Add { name } => DaemonRequest::PluginAdd { name },
                    RuntimeCommands::Remove { id } => DaemonRequest::PluginRemove { id },
                    RuntimeCommands::List { json_query } => {
                        list_json_query = json_query;
                        DaemonRequest::RuntimeList
                    },
                    RuntimeCommands::Show { id } => DaemonRequest::RuntimeShow { id },
                };
                match client::send_request(&request) {
                    Ok(response) => match response {
                        DaemonResponse::Ok { message } => println!("[RTSyn][INFO] {message}"),
                        DaemonResponse::Error { message } => eprintln!("[RTSyn][ERROR]: {message}"),
                        DaemonResponse::PluginAdded { id } => {
                            println!("[RTSyn][INFO] Plugin added with id {id}")
                        }
                        DaemonResponse::PluginList { plugins } => {
                            if plugins.is_empty() {
                                println!("[RTSyn][INFO] No plugins installed");
                            } else {
                                println!("[RTSyn][INFO] List of available plugins:");
                                for plugin in plugins {
                                    let version =
                                        plugin.version.unwrap_or_else(|| "unknown".to_string());
                                    let removable =
                                        if plugin.removable { "removable" } else { "bundled" };
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
                            if list_json_query {
                                let json = serde_json::to_string_pretty(&plugins)
                                    .unwrap_or_else(|_| "[]".to_string());
                                println!("{json}");
                                return Ok(());
                            }
                            if plugins.is_empty() {
                                println!("[RTSyn][INFO] No runtime plugins");
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
                        _ => {}
                    },
                    Err(err) => eprintln!("[RTSyn][ERROR]: {err}"),
                }
            }
        },
    }
    Ok(())
}
