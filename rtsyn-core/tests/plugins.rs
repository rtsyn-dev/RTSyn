use rtsyn_core::plugin::{
    plugin_display_name, InstalledPlugin, PluginCatalog, PluginManifest, PluginMetadataSource,
};
use std::path::PathBuf;
use std::time::Duration;
use workspace::{PluginDefinition, WorkspaceDefinition};

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

fn write_dynamic_manifest_without_api(dir: &PathBuf, kind: &str, name: &str) {
    let manifest = format!(
        "name = \"{}\"\nkind = \"{}\"\nversion = \"0.1.0\"\nlibrary = \"lib{}.so\"\n",
        name, kind, kind
    );
    std::fs::write(dir.join("plugin.toml"), manifest).expect("write plugin.toml");
}

fn write_dynamic_manifest_with_api(dir: &PathBuf, kind: &str, name: &str, api_version: u32) {
    let manifest = format!(
        "name = \"{}\"\nkind = \"{}\"\nversion = \"0.1.0\"\nlibrary = \"lib{}.so\"\napi_version = {}\n",
        name, kind, kind, api_version
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

#[test]
fn plugin_display_name_prefers_manifest_name_then_fallback() {
    let installed = vec![InstalledPlugin {
        manifest: PluginManifest {
            name: "Named Plugin".to_string(),
            kind: "named_plugin".to_string(),
            version: Some("0.1.0".to_string()),
            description: None,
            library: None,
            api_version: None,
        },
        path: PathBuf::new(),
        library_path: None,
        removable: false,
        metadata_inputs: Vec::new(),
        metadata_outputs: Vec::new(),
        metadata_variables: Vec::new(),
        display_schema: None,
        ui_schema: None,
    }];

    let workspace = WorkspaceDefinition {
        name: "ws".to_string(),
        description: String::new(),
        target_hz: 1000,
        plugins: vec![
            PluginDefinition {
                id: 1,
                kind: "named_plugin".to_string(),
                config: serde_json::json!({}),
                priority: 99,
                running: false,
            },
            PluginDefinition {
                id: 2,
                kind: "unknown_kind".to_string(),
                config: serde_json::json!({}),
                priority: 99,
                running: false,
            },
        ],
        connections: Vec::new(),
        settings: workspace::WorkspaceSettings::default(),
    };

    assert_eq!(
        plugin_display_name(&installed, &workspace, 1),
        "Named Plugin"
    );
    assert_eq!(
        plugin_display_name(&installed, &workspace, 2),
        "Unknown Kind"
    );
    assert_eq!(plugin_display_name(&installed, &workspace, 999), "plugin");
}

#[test]
fn install_rejects_dynamic_plugin_without_api_version() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plugin_dir = temp.path().join("missing-api-plugin");
    std::fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    write_dynamic_manifest_without_api(&plugin_dir, "missing_api_plugin", "Missing API Plugin");

    let install_db = temp.path().join("installed_plugins.json");
    let mut catalog = PluginCatalog::new(install_db);

    let err = catalog
        .install_plugin_from_folder(&plugin_dir, true, true, &NoMetadata)
        .expect_err("install should fail");
    assert!(err.contains("Missing plugin API version in manifest"));
}

#[test]
fn install_rejects_dynamic_plugin_with_wrong_api_version() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plugin_dir = temp.path().join("wrong-api-plugin");
    std::fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    write_dynamic_manifest_with_api(&plugin_dir, "wrong_api_plugin", "Wrong API Plugin", 999);

    let install_db = temp.path().join("installed_plugins.json");
    let mut catalog = PluginCatalog::new(install_db);

    let err = catalog
        .install_plugin_from_folder(&plugin_dir, true, true, &NoMetadata)
        .expect_err("install should fail");
    assert!(err.contains("Incompatible plugin API version in manifest"));
}
