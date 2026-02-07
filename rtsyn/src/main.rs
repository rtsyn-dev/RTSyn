use clap::{Parser, Subcommand};
use rtsyn_runtime::daemon::DaemonService;
use rtsyn_gui::{run_gui, GuiConfig};
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
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        None => {
            if cli.no_gui {
                eprintln!("--no-gui set but no command provided.");
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
    }
    Ok(())
}
