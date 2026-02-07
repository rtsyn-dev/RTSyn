use workspace::{add_connection, ConnectionDefinition};

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
