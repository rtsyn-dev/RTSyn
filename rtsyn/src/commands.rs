use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "rtsyn", version, about = "RTSyn MVP CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    Daemon {
        #[command(subcommand)]
        command: DaemonCommands,
    },
}

#[derive(Subcommand)]
pub enum DaemonCommands {
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
pub enum PluginCommands {
    New,
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
pub enum WorkspaceCommands {
    List,
    Load { name: String },
    New { name: String },
    Save { name: Option<String> },
    Edit { name: String },
    Delete { name: String },
}

#[derive(Subcommand)]
pub enum ConnectionCommands {
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
pub enum RuntimeCommands {
    Settings {
        #[command(subcommand)]
        command: RuntimeSettingsCommands,
    },
    UmlDiagram,
}

#[derive(Subcommand)]
pub enum RuntimeSettingsCommands {
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
