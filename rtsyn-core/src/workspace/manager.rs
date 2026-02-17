use crate::validation::Validator;
use crate::workspace::{io::*, settings::*};
use std::path::{Path, PathBuf};
use workspace::{WorkspaceDefinition, WorkspaceSettings};

pub struct WorkspaceManager {
    pub workspace: WorkspaceDefinition,
    pub workspace_path: PathBuf,
    pub workspace_dirty: bool,
    pub workspace_entries: Vec<WorkspaceEntry>,
    workspace_dir: PathBuf,
    runtime_defaults: WorkspaceSettings,
    runtime_factory: WorkspaceSettings,
    runtime_defaults_path: PathBuf,
}

impl WorkspaceManager {
    const RUNTIME_DEFAULTS_FILE: &'static str = "runtime_settings.defaults.json";
    const RUNTIME_FACTORY_FILE: &'static str = "runtime_settings.factory.json";

    fn runtime_defaults_path_for(workspace_dir: &Path) -> PathBuf {
        workspace_dir.join(Self::RUNTIME_DEFAULTS_FILE)
    }

    fn runtime_factory_path_for(workspace_dir: &Path) -> PathBuf {
        workspace_dir.join(Self::RUNTIME_FACTORY_FILE)
    }

    fn empty_workspace(name: &str, settings: WorkspaceSettings) -> WorkspaceDefinition {
        WorkspaceDefinition {
            name: name.to_string(),
            description: String::new(),
            target_hz: 1000,
            plugins: Vec::new(),
            connections: Vec::new(),
            settings,
        }
    }

    fn load_or_create_runtime_settings(
        workspace_dir: &Path,
    ) -> (WorkspaceSettings, WorkspaceSettings, PathBuf, PathBuf) {
        let defaults_path = Self::runtime_defaults_path_for(workspace_dir);
        let factory_path = Self::runtime_factory_path_for(workspace_dir);
        let builtin = WorkspaceSettings::default();

        let factory = match load_runtime_settings_file(&factory_path) {
            Ok(settings) => settings,
            Err(_) => {
                let _ = save_runtime_settings_file(&factory_path, &builtin);
                builtin.clone()
            }
        };

        let defaults = match load_runtime_settings_file(&defaults_path) {
            Ok(settings) => settings,
            Err(_) => {
                let _ = save_runtime_settings_file(&defaults_path, &factory);
                factory.clone()
            }
        };

        (defaults, factory, defaults_path, factory_path)
    }

    pub fn new(workspace_dir: PathBuf) -> Self {
        let (runtime_defaults, runtime_factory, runtime_defaults_path, _) =
            Self::load_or_create_runtime_settings(&workspace_dir);
        Self {
            workspace: Self::empty_workspace("default", runtime_defaults.clone()),
            workspace_path: PathBuf::new(),
            workspace_dirty: true,
            workspace_entries: Vec::new(),
            workspace_dir,
            runtime_defaults,
            runtime_factory,
            runtime_defaults_path,
        }
    }

    pub fn workspace_dir(&self) -> &Path {
        &self.workspace_dir
    }

    pub fn apply_runtime_settings_json(&mut self, json: &str) -> Result<(), String> {
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|e| format!("Invalid JSON: {e}"))?;
        self.apply_runtime_settings_patch(&value)
    }

    pub fn apply_runtime_settings_patch(
        &mut self,
        patch: &serde_json::Value,
    ) -> Result<(), String> {
        let obj = patch
            .as_object()
            .ok_or_else(|| "Settings patch must be a JSON object".to_string())?;

        let mut settings = self.workspace.settings.clone();
        let mut freq_changed = false;
        let mut period_changed = false;

        if let Some(value) = obj.get("frequency_value") {
            let freq = value
                .as_f64()
                .ok_or_else(|| "frequency_value must be a number".to_string())?;
            settings.frequency_value = freq.max(1.0);
            freq_changed = true;
        }
        if let Some(value) = obj.get("frequency_unit") {
            let unit = value
                .as_str()
                .ok_or_else(|| "frequency_unit must be a string".to_string())?;
            normalize_frequency_unit(unit)?;
            settings.frequency_unit = unit.to_string();
            freq_changed = true;
        }
        if let Some(value) = obj.get("period_value") {
            let period = value
                .as_f64()
                .ok_or_else(|| "period_value must be a number".to_string())?;
            settings.period_value = period.max(1.0);
            period_changed = true;
        }
        if let Some(value) = obj.get("period_unit") {
            let unit = value
                .as_str()
                .ok_or_else(|| "period_unit must be a string".to_string())?;
            normalize_period_unit(unit)?;
            settings.period_unit = unit.to_string();
            period_changed = true;
        }
        if let Some(value) = obj.get("selected_cores") {
            let array = value
                .as_array()
                .ok_or_else(|| "selected_cores must be an array".to_string())?;
            let mut cores = Vec::new();
            for item in array {
                let core = item
                    .as_u64()
                    .ok_or_else(|| "selected_cores must contain numbers".to_string())?;
                cores.push(core as usize);
            }
            settings.selected_cores = cores;
        }

        Validator::normalize_cores(&mut settings.selected_cores);

        if freq_changed && period_changed {
            return Err(
                "Provide either frequency_* or period_* values, not both at once.".to_string(),
            );
        }

        if freq_changed {
            let hz = frequency_hz_from(settings.frequency_value, &settings.frequency_unit)?;
            let period_seconds = 1.0 / hz;
            settings.period_value =
                period_value_from_seconds(period_seconds, &settings.period_unit)?;
        } else if period_changed {
            let period_seconds = period_seconds_from(settings.period_value, &settings.period_unit)?;
            let hz = 1.0 / period_seconds;
            settings.frequency_value = frequency_value_from_hz(hz, &settings.frequency_unit)?;
        }

        self.workspace.settings = settings;
        if self.workspace_path.as_os_str().is_empty() {
            self.update_runtime_defaults(self.workspace.settings.clone())?;
        }
        Ok(())
    }

    pub fn runtime_settings(&self) -> Result<RuntimeSettings, String> {
        let settings = &self.workspace.settings;
        normalize_frequency_unit(&settings.frequency_unit)?;
        normalize_period_unit(&settings.period_unit)?;
        let period_seconds = period_seconds_from(settings.period_value, &settings.period_unit)?;
        let (time_scale, time_label) = time_scale_and_label(&settings.period_unit)?;
        let mut cores = settings.selected_cores.clone();
        Validator::normalize_cores(&mut cores);
        Ok(RuntimeSettings {
            cores,
            period_seconds,
            time_scale,
            time_label,
        })
    }

    pub fn mark_dirty(&mut self) {
        self.workspace_dirty = true;
    }

    pub fn workspace_file_path(&self, name: &str) -> PathBuf {
        workspace_file_path_for(&self.workspace_dir, name)
    }

    pub fn scan_workspaces(&mut self) {
        self.workspace_entries = scan_workspace_entries(&self.workspace_dir);
    }

    pub fn load_workspace(&mut self, path: &Path) -> Result<(), String> {
        let loaded = load_workspace_file(path)?;
        self.workspace = loaded;
        self.workspace_path = path.to_path_buf();
        self.workspace_dirty = false;
        Ok(())
    }

    pub fn save_workspace_overwrite_current(&mut self) -> Result<(), String> {
        if self.workspace_path.as_os_str().is_empty() {
            return Err("No workspace path set".to_string());
        }
        save_workspace_file(&self.workspace, &self.workspace_path)?;
        self.workspace_dirty = false;
        Ok(())
    }

    pub fn save_workspace_as(&mut self, name: &str, description: &str) -> Result<(), String> {
        self.workspace.name = name.to_string();
        self.workspace.description = description.to_string();

        let path = self.workspace_file_path(name);
        let _ = std::fs::create_dir_all(&self.workspace_dir);
        save_workspace_file(&self.workspace, &path)?;
        self.workspace_path = path;
        self.workspace_dirty = false;
        Ok(())
    }

    pub fn create_workspace(&mut self, name: &str, description: &str) -> Result<(), String> {
        let path = self.workspace_file_path(name);
        if path.exists() {
            return Err("Workspace already exists".to_string());
        }

        self.workspace = Self::empty_workspace(name, self.runtime_defaults.clone());
        self.workspace.description = description.to_string();

        let _ = std::fs::create_dir_all(&self.workspace_dir);
        save_workspace_file(&self.workspace, &path)?;
        self.workspace_path = path;
        self.workspace_dirty = false;
        Ok(())
    }

    pub fn import_workspace(&mut self, source: &Path) -> Result<(), String> {
        let loaded = load_workspace_file(source)?;
        let dest_path = self.workspace_file_path(&loaded.name);
        if dest_path.exists() {
            return Err("Workspace with this name already exists".to_string());
        }
        let _ = std::fs::create_dir_all(&self.workspace_dir);
        std::fs::copy(source, &dest_path).map_err(|e| format!("Failed to copy: {e}"))?;
        Ok(())
    }

    pub fn rename_workspace(&mut self, name: &str) -> Result<(), String> {
        if self.workspace_path.as_os_str().is_empty() {
            return Err("No workspace loaded to edit".to_string());
        }
        let current_path = self.workspace_path.clone();
        let new_path = self.workspace_file_path(name);
        let mut workspace = self.workspace.clone();
        workspace.name = name.to_string();
        save_workspace_file(&workspace, &new_path)?;
        remove_old_workspace_file(&current_path, &new_path)?;
        self.workspace = workspace;
        self.workspace_path = new_path;
        self.workspace_dirty = false;
        Ok(())
    }

    pub fn delete_workspace(&mut self, name: &str) -> Result<(), String> {
        let path = self.workspace_file_path(name);
        if !path.exists() {
            return Err("Workspace not found".to_string());
        }
        std::fs::remove_file(&path).map_err(|e| format!("Failed to delete workspace: {e}"))?;
        if self.workspace_path == path {
            self.workspace = Self::empty_workspace("default", self.runtime_defaults.clone());
            self.workspace_path = PathBuf::new();
            self.workspace_dirty = true;
        }
        Ok(())
    }

    pub fn update_runtime_defaults(&mut self, settings: WorkspaceSettings) -> Result<(), String> {
        let normalized = normalize_workspace_settings(settings)?;
        save_runtime_settings_file(&self.runtime_defaults_path, &normalized)?;
        self.runtime_defaults = normalized.clone();
        if self.workspace_path.as_os_str().is_empty() {
            self.workspace.settings = normalized;
        }
        Ok(())
    }

    pub fn persist_runtime_settings_current_context(
        &mut self,
    ) -> Result<RuntimeSettingsSaveTarget, String> {
        if self.workspace_path.as_os_str().is_empty() {
            self.update_runtime_defaults(self.workspace.settings.clone())?;
            self.workspace_dirty = false;
            Ok(RuntimeSettingsSaveTarget::Defaults)
        } else {
            self.save_workspace_overwrite_current()?;
            Ok(RuntimeSettingsSaveTarget::Workspace)
        }
    }

    pub fn restore_runtime_settings_current_context(
        &mut self,
    ) -> Result<RuntimeSettingsSaveTarget, String> {
        self.reset_runtime_defaults_to_factory()?;
        if self.workspace_path.as_os_str().is_empty() {
            self.workspace_dirty = false;
            Ok(RuntimeSettingsSaveTarget::Defaults)
        } else {
            self.workspace.settings = self.runtime_defaults.clone();
            self.workspace_dirty = true;
            Ok(RuntimeSettingsSaveTarget::Workspace)
        }
    }

    pub fn reset_runtime_defaults_to_factory(&mut self) -> Result<(), String> {
        let factory = self.runtime_factory.clone();
        save_runtime_settings_file(&self.runtime_defaults_path, &factory)?;
        self.runtime_defaults = factory.clone();
        if self.workspace_path.as_os_str().is_empty() {
            self.workspace.settings = factory;
        }
        Ok(())
    }

    pub fn runtime_defaults(&self) -> &WorkspaceSettings {
        &self.runtime_defaults
    }

    pub fn runtime_factory(&self) -> &WorkspaceSettings {
        &self.runtime_factory
    }

    pub fn current_workspace_uml_diagram(&self) -> String {
        workspace_to_uml_diagram(&self.workspace)
    }
}
