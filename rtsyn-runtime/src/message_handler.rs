use std::collections::HashMap;
use std::sync::mpsc::Sender;
use workspace::WorkspaceDefinition;

#[derive(Debug, Clone)]
pub struct LogicSettings {
    pub cores: Vec<usize>,
    pub period_seconds: f64,
    pub time_scale: f64,
    pub time_label: String,
    pub ui_hz: f64,
    pub max_integration_steps: usize,
}

#[derive(Debug, Clone)]
pub struct LogicState {
    pub outputs: HashMap<(u64, String), f64>,
    pub input_values: HashMap<(u64, String), f64>,
    pub internal_variable_values: HashMap<(u64, String), serde_json::Value>,
    pub viewer_values: HashMap<u64, f64>,
    pub tick: u64,
    pub plotter_samples: HashMap<u64, Vec<(u64, Vec<f64>)>>,
}

#[derive(Debug, Clone)]
pub enum LogicMessage {
    UpdateSettings(LogicSettings),
    UpdateWorkspace(WorkspaceDefinition),
    SetPluginRunning(u64, bool),
    RestartPlugin(u64),
    QueryPluginBehavior(
        String,
        Option<String>,
        Sender<Option<rtsyn_plugin::ui::PluginBehavior>>,
    ),
    QueryPluginMetadata(
        String,
        Sender<
            Option<(
                Vec<String>,
                Vec<String>,
                Vec<(String, f64)>,
                Option<rtsyn_plugin::ui::DisplaySchema>,
                Option<rtsyn_plugin::ui::UISchema>,
            )>,
        >,
    ),
    GetPluginVariable(u64, String, Sender<Option<serde_json::Value>>),
    SetPluginVariable(u64, String, serde_json::Value),
}
