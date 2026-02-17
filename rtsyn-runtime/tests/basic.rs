use rtsyn_runtime::{LogicMessage, LogicSettings};
use workspace::WorkspaceDefinition;

#[test]
fn runtime_spawns_and_responds() {
    let (tx, rx) = rtsyn_runtime::spawn_runtime().unwrap();

    // Send workspace update
    let workspace = WorkspaceDefinition {
        name: "test".to_string(),
        description: String::new(),
        target_hz: 1000,
        plugins: Vec::new(),
        connections: Vec::new(),
        settings: workspace::WorkspaceSettings::default(),
    };

    tx.send(LogicMessage::UpdateWorkspace(workspace)).unwrap();

    // Should receive initial state
    let state = rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
    assert!(state.outputs.is_empty()); // No plugins, no outputs
}

#[test]
fn runtime_settings_update() {
    let (tx, _rx) = rtsyn_runtime::spawn_runtime().unwrap();

    let settings = LogicSettings {
        cores: vec![0],
        period_seconds: 0.001,
        time_scale: 1000.0,
        time_label: "time_ms".to_string(),
        ui_hz: 60.0,
        max_integration_steps: 10,
    };

    // Should not panic
    tx.send(LogicMessage::UpdateSettings(settings)).unwrap();
}
