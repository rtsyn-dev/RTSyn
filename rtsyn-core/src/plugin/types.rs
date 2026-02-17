use rtsyn_plugin::ui::{DisplaySchema, PluginBehavior, UISchema};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub kind: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub library: Option<String>,
    pub api_version: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    pub manifest: PluginManifest,
    pub path: PathBuf,
    pub library_path: Option<PathBuf>,
    pub removable: bool,
    pub metadata_inputs: Vec<String>,
    pub metadata_outputs: Vec<String>,
    pub metadata_variables: Vec<(String, f64)>,
    pub display_schema: Option<DisplaySchema>,
    pub ui_schema: Option<UISchema>,
}

#[derive(Debug, Clone)]
pub struct DetectedPlugin {
    pub manifest: PluginManifest,
    pub path: PathBuf,
}

pub trait PluginMetadataSource {
    fn query_plugin_metadata(
        &self,
        library_path: &str,
        timeout: Duration,
    ) -> Option<(
        Vec<String>,
        Vec<String>,
        Vec<(String, f64)>,
        Option<DisplaySchema>,
        Option<UISchema>,
    )>;

    fn query_plugin_behavior(
        &self,
        kind: &str,
        library_path: Option<&str>,
        timeout: Duration,
    ) -> Option<PluginBehavior>;
}
