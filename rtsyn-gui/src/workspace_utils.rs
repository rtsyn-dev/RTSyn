// Utility functions extracted from GUI to reduce coupling

use std::path::{Path, PathBuf};
use rtsyn_core::workspaces::{scan_workspace_entries as core_scan_workspace_entries, workspace_file_path as core_workspace_file_path};

pub fn scan_workspace_entries(workspace_dir: &Path) -> Vec<crate::state::WorkspaceEntry> {
    core_scan_workspace_entries(workspace_dir)
}

pub fn workspace_file_path(workspace_dir: &Path, name: &str) -> PathBuf {
    core_workspace_file_path(workspace_dir, name)
}
