use rtsyn_core::workspace::{RuntimeSettingsSaveTarget, WorkspaceManager};
use workspace::WorkspaceSettings;

#[test]
fn workspace_file_path_sanitizes_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    let manager = WorkspaceManager::new(dir.path().to_path_buf());
    let path = manager.workspace_file_path("My Workspace");
    assert!(path.ends_with("My_Workspace.json"));
}

#[test]
fn save_load_and_scan_workspaces() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut manager = WorkspaceManager::new(dir.path().to_path_buf());
    let path = manager.workspace_file_path("alpha");
    manager.workspace.name = "alpha".to_string();

    manager
        .save_workspace_as("alpha", "")
        .expect("save workspace");

    manager.load_workspace(&path).expect("load workspace");
    assert_eq!(manager.workspace.name, "alpha");

    manager.scan_workspaces();
    assert_eq!(manager.workspace_entries.len(), 1);
    assert_eq!(manager.workspace_entries[0].name, "alpha");
}

#[test]
fn runtime_settings_defaults_files_created_and_used() {
    let dir = tempfile::tempdir().expect("tempdir");
    let manager = WorkspaceManager::new(dir.path().to_path_buf());
    let defaults_path = dir.path().join("runtime_settings.defaults.json");
    let factory_path = dir.path().join("runtime_settings.factory.json");

    assert!(defaults_path.exists());
    assert!(factory_path.exists());
    assert_eq!(
        manager.workspace.settings.frequency_value,
        manager.runtime_defaults().frequency_value
    );
}

#[test]
fn runtime_settings_defaults_persist_when_no_workspace_loaded() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut manager = WorkspaceManager::new(dir.path().to_path_buf());
    manager
        .apply_runtime_settings_json(r#"{"frequency_value": 2000, "frequency_unit": "hz"}"#)
        .expect("apply defaults");

    let manager_reloaded = WorkspaceManager::new(dir.path().to_path_buf());
    assert_eq!(manager_reloaded.runtime_defaults().frequency_value, 2000.0);
    assert_eq!(manager_reloaded.workspace.settings.frequency_value, 2000.0);
}

#[test]
fn runtime_settings_factory_reset_restores_defaults() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut manager = WorkspaceManager::new(dir.path().to_path_buf());
    manager
        .update_runtime_defaults(WorkspaceSettings {
            frequency_value: 5000.0,
            frequency_unit: "hz".to_string(),
            period_value: 1.0,
            period_unit: "ms".to_string(),
            selected_cores: vec![0],
        })
        .expect("update defaults");
    manager
        .reset_runtime_defaults_to_factory()
        .expect("factory reset");

    let manager_reloaded = WorkspaceManager::new(dir.path().to_path_buf());
    assert_eq!(
        manager_reloaded.runtime_defaults().frequency_value,
        WorkspaceSettings::default().frequency_value
    );
}

#[test]
fn workspace_runtime_settings_do_not_override_global_defaults() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut manager = WorkspaceManager::new(dir.path().to_path_buf());
    manager
        .create_workspace("alpha", "")
        .expect("create workspace");
    manager
        .apply_runtime_settings_json(r#"{"frequency_value": 3000, "frequency_unit": "hz"}"#)
        .expect("apply workspace settings");

    let manager_reloaded = WorkspaceManager::new(dir.path().to_path_buf());
    assert_eq!(
        manager_reloaded.runtime_defaults().frequency_value,
        WorkspaceSettings::default().frequency_value
    );
}

#[test]
fn persist_runtime_settings_saves_defaults_without_workspace() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut manager = WorkspaceManager::new(dir.path().to_path_buf());
    manager.workspace.settings.frequency_value = 2222.0;

    let target = manager
        .persist_runtime_settings_current_context()
        .expect("persist settings");
    assert_eq!(target, RuntimeSettingsSaveTarget::Defaults);

    let manager_reloaded = WorkspaceManager::new(dir.path().to_path_buf());
    assert_eq!(manager_reloaded.runtime_defaults().frequency_value, 2222.0);
}

#[test]
fn persist_runtime_settings_saves_workspace_when_loaded() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut manager = WorkspaceManager::new(dir.path().to_path_buf());
    manager
        .create_workspace("alpha", "")
        .expect("create workspace");
    manager.workspace.settings.frequency_value = 3333.0;

    let target = manager
        .persist_runtime_settings_current_context()
        .expect("persist settings");
    assert_eq!(target, RuntimeSettingsSaveTarget::Workspace);

    let path = manager.workspace_file_path("alpha");
    let loaded = workspace::WorkspaceDefinition::load_from_file(path).expect("load workspace");
    assert_eq!(loaded.settings.frequency_value, 3333.0);
}

#[test]
fn restore_runtime_settings_restores_factory_for_current_context() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut manager = WorkspaceManager::new(dir.path().to_path_buf());
    manager
        .update_runtime_defaults(WorkspaceSettings {
            frequency_value: 2222.0,
            frequency_unit: "hz".to_string(),
            period_value: 1.0,
            period_unit: "ms".to_string(),
            selected_cores: vec![0],
        })
        .expect("update defaults");

    let target = manager
        .restore_runtime_settings_current_context()
        .expect("restore defaults");
    assert_eq!(target, RuntimeSettingsSaveTarget::Defaults);
    assert_eq!(
        manager.runtime_defaults().frequency_value,
        WorkspaceSettings::default().frequency_value
    );
}
