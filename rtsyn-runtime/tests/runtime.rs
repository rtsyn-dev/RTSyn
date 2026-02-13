use rtsyn_runtime::runtime::{spawn_runtime, LogicMessage, LogicSettings};
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
use workspace::{PluginDefinition, WorkspaceDefinition, WorkspaceSettings};

fn find_cdylib(crate_name: &str) -> PathBuf {
    // Workspace root = CARGO_MANIFEST_DIR/..
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.pop(); // from collection/ to workspace root

    let deps = root.join("target").join("debug").join("deps");

    let (prefix, suffix) = if cfg!(target_os = "windows") {
        (crate_name.to_string(), ".dll")
    } else if cfg!(target_os = "macos") {
        (format!("lib{crate_name}"), ".dylib")
    } else {
        (format!("lib{crate_name}"), ".so")
    };

    let entries =
        fs::read_dir(&deps).unwrap_or_else(|e| panic!("failed to read deps dir {:?}: {e}", deps));

    for e in entries.flatten() {
        let p = e.path();
        if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
            if name.starts_with(&prefix) && name.ends_with(suffix) {
                return p;
            }
        }
    }

    panic!(
        "could not find cdylib for crate `{crate_name}` in {:?}",
        deps
    );
}

#[test]
fn runtime_executes_dynamic_mock_plugin_and_emits_outputs() {
    let (logic_tx, logic_state_rx) = spawn_runtime().expect("failed to spawn runtime");

    // Faster UI publishing to avoid test races
    let settings = LogicSettings {
        cores: vec![0],
        period_seconds: 0.001,
        time_scale: 1_000.0,
        time_label: "time_ms".to_string(),
        max_integration_steps: 50,
        ui_hz: 500.0,
    };
    logic_tx
        .send(LogicMessage::UpdateSettings(settings))
        .unwrap();

    // Locate the actual built cdylib
    let lib_path = find_cdylib("mock_out_5_rs_runtime");

    let plugins = vec![PluginDefinition {
        id: 1,
        kind: "mock_out_5_rs_runtime".to_string(), // falls into dynamic branch
        config: json!({
            "library_path": lib_path.to_string_lossy().to_string()
        }),
        priority: 0,
        running: true,
    }];

    let workspace = WorkspaceDefinition {
        name: "test".to_string(),
        description: "".to_string(),
        target_hz: 1000,
        plugins,
        connections: vec![],
        settings: WorkspaceSettings::default(),
    };

    logic_tx
        .send(LogicMessage::UpdateWorkspace(workspace))
        .unwrap();
    logic_tx
        .send(LogicMessage::SetPluginRunning(1, true))
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut last_tick = None;

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }

        let state = logic_state_rx
            .recv_timeout(remaining)
            .expect("did not receive runtime state in time");

        last_tick = Some(state.tick);

        if let Some(v) = state.outputs.get(&(1, "out".to_string())).copied() {
            if (v - 5.0).abs() < 1e-6 {
                return;
            }
        }
    }

    panic!(
        "dynamic plugin never produced out=5.0 (last_tick={:?})",
        last_tick
    );
}
