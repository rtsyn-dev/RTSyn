use serde::{Deserialize, Serialize};

pub const DEFAULT_SOCKET_PATH: &str = "/tmp/rtsyn-daemon.sock";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSummary {
    pub kind: String,
    pub name: String,
    pub version: Option<String>,
    pub removable: bool,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonRequest {
    PluginList,
    PluginInstall { path: String },
    PluginUninstall { name: String },
    PluginAdd { name: String },
    PluginRemove { id: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonResponse {
    Ok { message: String },
    Error { message: String },
    PluginList { plugins: Vec<PluginSummary> },
    PluginAdded { id: u64 },
}
