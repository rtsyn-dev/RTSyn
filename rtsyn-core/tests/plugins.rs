use rtsyn_core::plugin::{PluginCatalog, PluginMetadataSource};
use std::path::PathBuf;
use std::time::Duration;
use workspace::WorkspaceDefinition;

struct NoMetadata;

impl PluginMetadataSource for NoMetadata {
    fn query_plugin_metadata(
        &self,
        _library_path: &str,
        _timeout: Duration,
    ) -> Option<(
        Vec<String>,
        Vec<String>,
        Vec<(String, f64)>,
        Option<rtsyn_plugin::ui::DisplaySchema>,
        Option<rtsyn_plugin::ui::UISchema>,
    )> {
        None
    }

    fn query_plugin_behavior(
        &self,
        _kind: &str,
        _library_path: Option<&str>,
        _timeout: Duration,
    ) -> Option<rtsyn_plugin::ui::PluginBehavior> {
        None
    }
}

fn write_plugin_manifest(dir: &PathBuf, kind: &str, name: &str) {
    let manifest = format!(
        "name = \"{}\"\nkind = \"{}\"\nversion = \"0.1.0\"\n",
        name, kind
    );
    std::fs::write(dir.join("plugin.toml"), manifest).expect("write plugin.toml");
}

#[test]
fn install_add_uninstall_plugin() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plugin_dir = temp.path().join("test-plugin");
    std::fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    write_plugin_manifest(&plugin_dir, "test_plugin", "Test Plugin");

    let install_db = temp.path().join("installed_plugins.json");
    let mut catalog = PluginCatalog::new(install_db);

    catalog
        .install_plugin_from_folder(&plugin_dir, true, true, &NoMetadata)
        .expect("install plugin");

    assert!(catalog
        .list_installed()
        .iter()
        .any(|p| p.manifest.kind == "test_plugin"));

    let mut workspace = WorkspaceDefinition {
        name: "ws".to_string(),
        description: String::new(),
        target_hz: 1000,
        plugins: Vec::new(),
        connections: Vec::new(),
        settings: workspace::WorkspaceSettings::default(),
    };

    let id = catalog
        .add_installed_plugin_to_workspace("test_plugin", &mut workspace, &NoMetadata)
        .expect("add plugin");
    assert_eq!(id, 1);
    assert_eq!(workspace.plugins.len(), 1);
    assert_eq!(workspace.plugins[0].kind, "test_plugin");

    let removed = catalog
        .uninstall_plugin_by_kind("test_plugin")
        .expect("uninstall plugin");
    assert_eq!(removed.manifest.kind, "test_plugin");
}
