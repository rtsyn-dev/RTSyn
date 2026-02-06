use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PluginManifest {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) version: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) library: Option<String>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct InstalledPlugin {
    pub(crate) manifest: PluginManifest,
    pub(crate) path: PathBuf,
    pub(crate) library_path: Option<PathBuf>,
    pub(crate) removable: bool,
    pub(crate) metadata_inputs: Vec<String>,
    pub(crate) metadata_outputs: Vec<String>,
    pub(crate) metadata_variables: Vec<(String, f64)>,
    pub(crate) display_schema: Option<rtsyn_plugin::ui::DisplaySchema>,
    pub(crate) ui_schema: Option<rtsyn_plugin::ui::UISchema>,
}

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceEntry {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) plugins: usize,
    pub(crate) plugin_kinds: Vec<String>,
    pub(crate) path: PathBuf,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum WorkspaceDialogMode {
    New,
    Save,
    Edit,
}

#[derive(Debug, Clone)]
pub(crate) enum ConfirmAction {
    RemovePlugin(u64),
    UninstallPlugin(usize),
    DeleteWorkspace(PathBuf),
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum WorkspaceTimingTab {
    Frequency,
    Period,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum FrequencyUnit {
    Hz,
    KHz,
    MHz,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PeriodUnit {
    Ns,
    Us,
    Ms,
    S,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TimeUnit {
    Ns,
    Us,
    Ms,
    S,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionEditMode {
    Add,
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionEditTab {
    Inputs,
    Outputs,
}

#[derive(Debug, Clone)]
pub(crate) struct DetectedPlugin {
    pub(crate) manifest: PluginManifest,
    pub(crate) path: PathBuf,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ManageTab {
    Install,
}

impl Default for ManageTab {
    fn default() -> Self {
        Self::Install
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum PluginTab {
    Add,
    Organize,
}

impl Default for PluginTab {
    fn default() -> Self {
        Self::Add
    }
}
