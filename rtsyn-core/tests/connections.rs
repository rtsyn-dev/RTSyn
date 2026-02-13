use rtsyn_core::connection::{add_connection as add_connection_with_workspace, sync_extendable_input_count};
use rtsyn_core::plugin::{InstalledPlugin, PluginManifest};
use serde_json::json;
use std::path::PathBuf;
use workspace::{add_connection, ConnectionDefinition, PluginDefinition, WorkspaceDefinition, WorkspaceSettings};

#[test]
fn add_connection_rejects_duplicate_between_same_plugins() {
    let mut connections = Vec::new();
    let first = ConnectionDefinition {
        from_plugin: 1,
        from_port: "out".to_string(),
        to_plugin: 2,
        to_port: "in".to_string(),
        kind: "shared_memory".to_string(),
    };
    add_connection(&mut connections, first, 1).expect("first connection");

    let second = ConnectionDefinition {
        from_plugin: 1,
        from_port: "out".to_string(),
        to_plugin: 2,
        to_port: "in".to_string(),
        kind: "shared_memory".to_string(),
    };
    let result = add_connection(&mut connections, second, 1);
    assert!(result.is_err());
}

#[test]
fn add_connection_rejects_same_output_to_same_target() {
    let mut connections = Vec::new();
    let first = ConnectionDefinition {
        from_plugin: 1,
        from_port: "out".to_string(),
        to_plugin: 2,
        to_port: "in_0".to_string(),
        kind: "shared_memory".to_string(),
    };
    add_connection(&mut connections, first, 1).expect("first connection");

    let second = ConnectionDefinition {
        from_plugin: 1,
        from_port: "out".to_string(),
        to_plugin: 2,
        to_port: "in_1".to_string(),
        kind: "shared_memory".to_string(),
    };
    let result = add_connection(&mut connections, second, 1);
    assert!(result.is_err());
}

#[test]
fn add_connection_sets_extendable_input_and_csv_column() {
    let installed = vec![
        InstalledPlugin {
            manifest: PluginManifest {
                name: "Source Plugin".to_string(),
                kind: "source_plugin".to_string(),
                version: Some("0.1.0".to_string()),
                description: None,
                library: None,
            },
            path: PathBuf::new(),
            library_path: None,
            removable: false,
            metadata_inputs: Vec::new(),
            metadata_outputs: vec!["out".to_string()],
            metadata_variables: Vec::new(),
            display_schema: None,
            ui_schema: None,
        },
        InstalledPlugin {
            manifest: PluginManifest {
                name: "CSV Recorder".to_string(),
                kind: "csv_recorder".to_string(),
                version: Some("0.1.0".to_string()),
                description: None,
                library: None,
            },
            path: PathBuf::new(),
            library_path: None,
            removable: false,
            metadata_inputs: vec!["in".to_string()],
            metadata_outputs: Vec::new(),
            metadata_variables: Vec::new(),
            display_schema: None,
            ui_schema: None,
        },
    ];

    let mut workspace = WorkspaceDefinition {
        name: "ws".to_string(),
        description: String::new(),
        target_hz: 1000,
        plugins: vec![
            PluginDefinition {
                id: 1,
                kind: "source_plugin".to_string(),
                config: json!({}),
                priority: 99,
                running: false,
            },
            PluginDefinition {
                id: 2,
                kind: "csv_recorder".to_string(),
                config: json!({}),
                priority: 99,
                running: false,
            },
        ],
        connections: Vec::new(),
        settings: WorkspaceSettings::default(),
    };

    add_connection_with_workspace(
        &mut workspace,
        &installed,
        1,
        "out",
        2,
        "in",
        "shared_memory",
    )
    .expect("add connection");

    assert_eq!(workspace.connections.len(), 1);
    assert_eq!(workspace.connections[0].to_port, "in_0");
    assert_eq!(
        workspace.plugins[1].config.get("input_count").and_then(|v| v.as_u64()),
        Some(1)
    );
    let columns = workspace.plugins[1]
        .config
        .get("columns")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert_eq!(columns.len(), 1);
    assert_eq!(columns[0].as_str(), Some("csv_recorder_2_in_0"));
}

#[test]
fn sync_extendable_input_count_truncates_csv_columns() {
    let mut workspace = WorkspaceDefinition {
        name: "ws".to_string(),
        description: String::new(),
        target_hz: 1000,
        plugins: vec![PluginDefinition {
            id: 2,
            kind: "csv_recorder".to_string(),
            config: json!({
                "input_count": 4,
                "columns": ["a", "b", "c", "d"]
            }),
            priority: 99,
            running: false,
        }],
        connections: vec![ConnectionDefinition {
            from_plugin: 1,
            from_port: "out".to_string(),
            to_plugin: 2,
            to_port: "in_0".to_string(),
            kind: "shared_memory".to_string(),
        }],
        settings: WorkspaceSettings::default(),
    };

    sync_extendable_input_count(&mut workspace, 2);
    assert_eq!(
        workspace.plugins[0]
            .config
            .get("input_count")
            .and_then(|v| v.as_u64()),
        Some(1)
    );
    let columns = workspace.plugins[0]
        .config
        .get("columns")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert_eq!(columns.len(), 1);
    assert_eq!(columns[0].as_str(), Some("a"));
}
