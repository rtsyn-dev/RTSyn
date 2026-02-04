use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use workspace::{ConnectionDefinition, PluginDefinition, WorkspaceDefinition, WorkspaceSettings};

#[test]
fn save_and_load_workspace() {
    let mut path = std::env::temp_dir();
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    path.push(format!("rtsyn_workspace_{unique}.json"));

    let workspace = WorkspaceDefinition {
        name: "test".to_string(),
        description: "desc".to_string(),
        target_hz: 1000,
        plugins: vec![PluginDefinition {
            id: 1,
            kind: "adder".to_string(),
            config: serde_json::json!({"x": 1.0}),
            priority: 0,
            running: true,
        }],
        connections: vec![ConnectionDefinition {
            from_plugin: 1,
            from_port: "out".to_string(),
            to_plugin: 2,
            to_port: "in".to_string(),
            kind: "shared_memory".to_string(),
        }],
        settings: WorkspaceSettings::default(),
    };

    workspace.save_to_file(&path).unwrap();
    let loaded = WorkspaceDefinition::load_from_file(&path).unwrap();

    assert_eq!(loaded.name, workspace.name);
    assert_eq!(loaded.description, workspace.description);
    assert_eq!(loaded.target_hz, workspace.target_hz);
    assert_eq!(loaded.plugins.len(), 1);
    assert_eq!(loaded.connections.len(), 1);
    assert_eq!(
        loaded.settings.frequency_value,
        workspace.settings.frequency_value
    );

    fs::remove_file(&path).unwrap();
}

#[test]
fn connection_rules() {
    let mut connections = vec![ConnectionDefinition {
        from_plugin: 1,
        from_port: "out".to_string(),
        to_plugin: 2,
        to_port: "in_0".to_string(),
        kind: "shared_memory".to_string(),
    }];

    let err = workspace::validate_connection(&connections, 2, 2, "in_0", 2).unwrap_err();
    assert_eq!(err, workspace::ConnectionRuleError::SelfConnection);

    connections.push(ConnectionDefinition {
        from_plugin: 3,
        from_port: "out".to_string(),
        to_plugin: 2,
        to_port: "in_0".to_string(),
        kind: "shared_memory".to_string(),
    });

    let err = workspace::validate_connection(&connections, 4, 2, "in_0", 2).unwrap_err();
    assert_eq!(err, workspace::ConnectionRuleError::InputLimitExceeded);
}

#[test]
fn prune_extendable_inputs_plugin_connections_removes_excess_inputs() {
    let mut connections = vec![
        ConnectionDefinition {
            from_plugin: 1,
            from_port: "out".to_string(),
            to_plugin: 99,
            to_port: "in_0".to_string(),
            kind: "shared_memory".to_string(),
        },
        ConnectionDefinition {
            from_plugin: 2,
            from_port: "out".to_string(),
            to_plugin: 99,
            to_port: "in_2".to_string(),
            kind: "shared_memory".to_string(),
        },
    ];

    workspace::prune_extendable_inputs_plugin_connections(&mut connections, 99, 1);
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0].to_port, "in_0");
}

#[test]
fn input_sum_helpers() {
    let connections = vec![
        ConnectionDefinition {
            from_plugin: 1,
            from_port: "out".to_string(),
            to_plugin: 2,
            to_port: "in_a".to_string(),
            kind: "shared_memory".to_string(),
        },
        ConnectionDefinition {
            from_plugin: 3,
            from_port: "out".to_string(),
            to_plugin: 2,
            to_port: "in_b".to_string(),
            kind: "shared_memory".to_string(),
        },
    ];
    let mut outputs = std::collections::HashMap::new();
    outputs.insert((1, "out".to_string()), 1.5);
    outputs.insert((3, "out".to_string()), -0.5);

    let sum_a = workspace::input_sum(&connections, &outputs, 2, "in_a");
    let sum_any = workspace::input_sum_any(
        &connections,
        &outputs,
        2,
        &vec!["in_a".to_string(), "in_b".to_string()],
    );
    assert_eq!(sum_a, 1.5);
    assert_eq!(sum_any, 1.0);
}

#[test]
fn order_plugins_for_execution_respects_priority_and_id() {
    let plugins = vec![
        PluginDefinition {
            id: 2,
            kind: "a".to_string(),
            config: serde_json::json!({}),
            priority: 5,
            running: true,
        },
        PluginDefinition {
            id: 1,
            kind: "b".to_string(),
            config: serde_json::json!({}),
            priority: 5,
            running: true,
        },
        PluginDefinition {
            id: 3,
            kind: "c".to_string(),
            config: serde_json::json!({}),
            priority: 1,
            running: true,
        },
    ];
    let ordered = workspace::order_plugins_for_execution(&plugins, &[]);
    let ids: Vec<u64> = ordered.iter().map(|p| p.id).collect();
    assert_eq!(ids, vec![3, 1, 2]);
}
