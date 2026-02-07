use std::path::PathBuf;

pub(crate) use rtsyn_core::plugin::{InstalledPlugin, PluginManifest};

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
