use rtsyn_core::workspace::WorkspaceManager;

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
