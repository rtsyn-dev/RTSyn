pub mod catalog;
pub mod manager;
pub mod types;

pub use catalog::PluginCatalog;
pub use manager::{is_extendable_inputs, PluginManager};
pub use types::{DetectedPlugin, InstalledPlugin, PluginManifest, PluginMetadataSource};

use workspace::WorkspaceDefinition;

pub fn plugin_display_name(
    installed: &[InstalledPlugin],
    workspace: &WorkspaceDefinition,
    plugin_id: u64,
) -> String {
    let Some(plugin) = workspace
        .plugins
        .iter()
        .find(|plugin| plugin.id == plugin_id)
    else {
        return "plugin".to_string();
    };

    installed
        .iter()
        .find(|p| p.manifest.kind == plugin.kind)
        .map(|p| p.manifest.name.clone())
        .unwrap_or_else(|| PluginManager::display_kind(&plugin.kind))
}

pub fn empty_workspace() -> WorkspaceDefinition {
    WorkspaceDefinition {
        name: "cli".to_string(),
        description: "CLI workspace".to_string(),
        target_hz: 1000,
        plugins: Vec::new(),
        connections: Vec::new(),
        settings: workspace::WorkspaceSettings::default(),
    }
}