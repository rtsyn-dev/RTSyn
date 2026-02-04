use serde::Deserialize;
use std::path::PathBuf;
use workspace::WorkspaceDefinition;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct PluginManifest {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) version: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) library: Option<String>,
    #[serde(default)]
    pub(crate) supports_start_stop: bool,
    #[serde(default)]
    pub(crate) supports_restart: bool,
    #[serde(default)]
    pub(crate) extendable_inputs: bool,
    #[serde(default = "default_auto_extend_inputs")]
    pub(crate) auto_extend_inputs: bool,
    #[serde(default)]
    pub(crate) connection_dependent: bool,
    #[serde(default = "default_loads_started")]
    pub(crate) loads_started: bool,
    #[serde(default)]
    pub(crate) inputs: Vec<PortManifest>,
    #[serde(default)]
    pub(crate) outputs: Vec<PortManifest>,
    #[serde(default)]
    pub(crate) variables: Vec<VariableManifest>,
}

fn default_loads_started() -> bool {
    true
}

fn default_auto_extend_inputs() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct PortManifest {
    pub(crate) name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct VariableManifest {
    pub(crate) name: String,
    pub(crate) default: f64,
    pub(crate) description: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct InstalledPlugin {
    pub(crate) manifest: PluginManifest,
    pub(crate) path: PathBuf,
    pub(crate) library_path: Option<PathBuf>,
    pub(crate) removable: bool,
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
    OverwriteWorkspace(PathBuf, WorkspaceDefinition),
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
    #[allow(dead_code)]
    pub(crate) library_path: Option<PathBuf>,
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
