use crate::state::WorkspaceEntry;
use crate::workspace_utils::{scan_workspace_entries, workspace_file_path};
use std::fs;
use std::path::{Path, PathBuf};
use workspace::{WorkspaceDefinition, WorkspaceSettings};

pub struct WorkspaceManager {
    pub workspace: WorkspaceDefinition,
    pub workspace_path: PathBuf,
    pub workspace_dirty: bool,
    pub workspace_entries: Vec<WorkspaceEntry>,
    workspace_dir: PathBuf,
}

impl WorkspaceManager {
    pub fn new(workspace_dir: PathBuf) -> Self {
        Self {
            workspace: WorkspaceDefinition {
                name: "default".to_string(),
                description: String::new(),
                target_hz: 1000,
                plugins: Vec::new(),
                connections: Vec::new(),
                settings: WorkspaceSettings::default(),
            },
            workspace_path: PathBuf::new(),
            workspace_dirty: true,
            workspace_entries: Vec::new(),
            workspace_dir,
        }
    }

    pub fn mark_dirty(&mut self) {
        self.workspace_dirty = true;
    }

    pub fn workspace_file_path(&self, name: &str) -> PathBuf {
        workspace_file_path(&self.workspace_dir, name)
    }

    pub fn scan_workspaces(&mut self) {
        self.workspace_entries = scan_workspace_entries(&self.workspace_dir);
    }

    pub fn load_workspace(&mut self, path: &Path) -> Result<(), String> {
        let loaded = WorkspaceDefinition::load_from_file(path)
            .map_err(|e| format!("Failed to load workspace: {}", e))?;
        
        self.workspace = loaded;
        self.workspace_path = path.to_path_buf();
        self.workspace_dirty = false;
        Ok(())
    }

    pub fn save_workspace_overwrite_current(&mut self) -> Result<(), String> {
        if self.workspace_path.as_os_str().is_empty() {
            return Err("No workspace path set".to_string());
        }
        
        self.workspace.save_to_file(&self.workspace_path)
            .map_err(|e| format!("Failed to save: {}", e))?;
        
        self.workspace_dirty = false;
        Ok(())
    }

    pub fn save_workspace_as(&mut self, name: &str, description: &str) -> Result<(), String> {
        self.workspace.name = name.to_string();
        self.workspace.description = description.to_string();
        
        let path = self.workspace_file_path(name);
        let _ = fs::create_dir_all(&self.workspace_dir);
        
        self.workspace.save_to_file(&path)
            .map_err(|e| format!("Failed to save: {}", e))?;
        
        self.workspace_path = path;
        self.workspace_dirty = false;
        Ok(())
    }

    pub fn create_workspace(&mut self, name: &str, description: &str) -> Result<(), String> {
        let path = self.workspace_file_path(name);
        
        if path.exists() {
            return Err("Workspace already exists".to_string());
        }
        
        self.workspace = WorkspaceDefinition {
            name: name.to_string(),
            description: description.to_string(),
            target_hz: 1000,
            plugins: Vec::new(),
            connections: Vec::new(),
            settings: WorkspaceSettings::default(),
        };
        
        let _ = fs::create_dir_all(&self.workspace_dir);
        self.workspace.save_to_file(&path)
            .map_err(|e| format!("Failed to create: {}", e))?;
        
        self.workspace_path = path;
        self.workspace_dirty = false;
        Ok(())
    }

    pub fn import_workspace(&mut self, source: &Path) -> Result<(), String> {
        let loaded = WorkspaceDefinition::load_from_file(source)
            .map_err(|e| format!("Failed to import: {}", e))?;
        
        let dest_path = self.workspace_file_path(&loaded.name);
        
        if dest_path.exists() {
            return Err("Workspace with this name already exists".to_string());
        }
        
        let _ = fs::create_dir_all(&self.workspace_dir);
        fs::copy(source, &dest_path)
            .map_err(|e| format!("Failed to copy: {}", e))?;
        
        Ok(())
    }
}
