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
pub struct WorkspaceSummary {
    pub index: usize,
    pub name: String,
    pub description: String,
    pub plugins: usize,
    pub plugin_kinds: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionSummary {
    pub index: usize,
    pub from_plugin: u64,
    pub from_port: String,
    pub to_plugin: u64,
    pub to_port: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimePluginSummary {
    pub id: u64,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimePluginState {
    pub outputs: Vec<(String, f64)>,
    pub inputs: Vec<(String, f64)>,
    pub internal_variables: Vec<(String, serde_json::Value)>,
    pub variables: Vec<(String, serde_json::Value)>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonRequest {
    PluginList,
    PluginInstall { path: String },
    PluginUninstall { name: String },
    PluginReinstall { name: String },
    PluginRebuild { name: String },
    PluginAdd { name: String },
    PluginRemove { id: u64 },
    WorkspaceList,
    WorkspaceLoad { name: String },
    WorkspaceNew { name: String },
    WorkspaceSave { name: Option<String> },
    WorkspaceEdit { name: String },
    WorkspaceDelete { name: String },
    ConnectionList,
    ConnectionShow { plugin_id: u64 },
    ConnectionAdd {
        from_plugin: u64,
        from_port: String,
        to_plugin: u64,
        to_port: String,
        kind: String,
    },
    ConnectionRemove {
        from_plugin: u64,
        from_port: String,
        to_plugin: u64,
        to_port: String,
    },
    ConnectionRemoveIndex { index: usize },
    DaemonStop,
    DaemonReload,
    RuntimeList,
    RuntimeShow { id: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonResponse {
    Ok { message: String },
    Error { message: String },
    PluginList { plugins: Vec<PluginSummary> },
    PluginAdded { id: u64 },
    WorkspaceList { workspaces: Vec<WorkspaceSummary> },
    ConnectionList { connections: Vec<ConnectionSummary> },
    RuntimeList { plugins: Vec<RuntimePluginSummary> },
    RuntimeShow { id: u64, kind: String, state: RuntimePluginState },
}
