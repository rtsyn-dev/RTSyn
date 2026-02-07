use rtsyn_core::workspaces::{
    empty_workspace, load_workspace, save_workspace, scan_workspace_entries, workspace_file_path,
};

#[test]
fn workspace_file_path_sanitizes_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = workspace_file_path(dir.path(), "My Workspace");
    assert!(path.ends_with("My_Workspace.json"));
}

#[test]
fn save_load_and_scan_workspaces() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = workspace_file_path(dir.path(), "alpha");
    let workspace = empty_workspace("alpha");

    save_workspace(&workspace, &path).expect("save workspace");

    let loaded = load_workspace(&path).expect("load workspace");
    assert_eq!(loaded.name, "alpha");

    let entries = scan_workspace_entries(dir.path());
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "alpha");
}
