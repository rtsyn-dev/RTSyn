use std::path::PathBuf;

pub use rtsyn_core::plugin::{InstalledPlugin, PluginManifest};

#[derive(Debug, Clone, Copy)]
pub enum WorkspaceDialogMode {
    New,
    Save,
    Edit,
}

#[derive(Debug, Clone)]
pub enum ConfirmAction {
    RemovePlugin(u64),
    UninstallPlugin(usize),
    DeleteWorkspace(PathBuf),
}

#[derive(Debug, Clone, Copy)]
pub enum WorkspaceTimingTab {
    Frequency,
    Period,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpTopic {
    Plugins,
    Workspaces,
    Runtime,
    RTSyn,
    CLI,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrequencyUnit {
    Hz,
    KHz,
    MHz,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PeriodUnit {
    Ns,
    Us,
    Ms,
    S,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimeUnit {
    Ns,
    Us,
    Ms,
    S,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionEditMode {
    Add,
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionEditTab {
    Inputs,
    Outputs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionEditorHost {
    Main,
    PluginWindow(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Cards,
    State,
}

impl Default for ViewMode {
    fn default() -> Self {
        ViewMode::Cards
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginOrderMode {
    Name,
    Id,
    Priority,
    Connections,
}
