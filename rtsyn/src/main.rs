use clap::{Parser, Subcommand};
use rtsyn_runtime::daemon::DaemonService;
use rtsyn_gui::{run_gui, GuiConfig};
use rtsyn_cli::{client, daemon, protocol::{DaemonRequest, DaemonResponse}};
use std::time::Duration;

#[derive(Parser)]
#[command(name = "rtsyn", version, about = "RTSyn MVP CLI")]
struct Cli {
    /// Disable the GUI (GUI is the default)
    #[arg(long)]
    no_gui: bool,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Run {
        #[arg(long, default_value_t = 1000)]
        ticks: u64,
        /// Run without GUI
        #[arg(long)]
        no_gui: bool,
    },
    Daemon {
        #[arg(long, default_value_t = 60)]
        duration_seconds: u64,
        #[arg(long)]
        workspace: Option<String>,
    },
    Plugin {
        #[command(subcommand)]
        command: PluginCommands,
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
    Remove {
        id: u64,
    },
    Uninstall {
        name: String,
    },
    List,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        None => {
            if cli.no_gui {
                daemon::run_daemon()?;
                return Ok(());
            }
            run_gui(GuiConfig::default())?;
        }
        Some(Commands::Run { ticks, no_gui }) => {
            if no_gui || cli.no_gui {
                // Daemon mode - run without GUI
                let daemon = DaemonService::new()?;
                daemon.run_for_ticks(ticks)?;
            } else {
                // Legacy mode - run collection directly then GUI
                let mut collection =
                    rtsyn_runtime::Runtime::new(workspace::WorkspaceDefinition {
                        name: "test".to_string(),
                        description: String::new(),
                        target_hz: 1000,
                        plugins: Vec::new(),
                        connections: Vec::new(),
                        settings: workspace::WorkspaceSettings::default(),
                    });
                for _ in 0..ticks {
                    collection.tick()?;
                }
                run_gui(GuiConfig::default())?;
            }
        }
        Some(Commands::Daemon { duration_seconds, workspace }) => {
            let daemon = DaemonService::new()?;
            
            if let Some(workspace_path) = workspace {
                if let Ok(workspace_def) = workspace::WorkspaceDefinition::load_from_file(&workspace_path) {
                    daemon.load_workspace(workspace_def);
                } else {
                    eprintln!("Failed to load workspace: {}", workspace_path);
                    return Ok(());
                }
            }
            
            daemon.run_for_duration(Duration::from_secs(duration_seconds))?;
        }
        Some(Commands::Plugin { command }) => {
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
                                eprintln!("Failed to resolve install path: {err}");
                                return Ok(());
                            }
                        }
                    };
                    DaemonRequest::PluginInstall {
                        path: absolute_path.to_string_lossy().to_string(),
                    }
                }
                PluginCommands::Remove { id } => DaemonRequest::PluginRemove { id },
                PluginCommands::Uninstall { name } => DaemonRequest::PluginUninstall { name },
                PluginCommands::List => DaemonRequest::PluginList,
            };
            match client::send_request(&request) {
                Ok(response) => match response {
                    DaemonResponse::Ok { message } => println!("{message}"),
                    DaemonResponse::Error { message } => eprintln!("{message}"),
                    DaemonResponse::PluginAdded { id } => println!("Plugin added with id {id}"),
                    DaemonResponse::PluginList { plugins } => {
                        if plugins.is_empty() {
                            println!("No plugins installed");
                        } else {
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
                },
                Err(err) => eprintln!("{err}"),
            }
        }
    }
    Ok(())
}
