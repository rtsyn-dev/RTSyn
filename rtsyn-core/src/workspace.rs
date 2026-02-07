use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use workspace::{WorkspaceDefinition, WorkspaceSettings};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceEntry {
    pub name: String,
    pub description: String,
    pub plugins: usize,
    pub plugin_kinds: Vec<String>,
    pub path: PathBuf,
}

pub struct WorkspaceManager {
    pub workspace: WorkspaceDefinition,
    pub workspace_path: PathBuf,
    pub workspace_dirty: bool,
    pub workspace_entries: Vec<WorkspaceEntry>,
    workspace_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct RuntimeSettings {
    pub cores: Vec<usize>,
    pub period_seconds: f64,
    pub time_scale: f64,
    pub time_label: String,
}

impl WorkspaceManager {
    fn scan_workspace_entries(workspace_dir: &Path) -> Vec<WorkspaceEntry> {
        let mut entries = Vec::new();
        let _ = std::fs::create_dir_all(workspace_dir);
        if let Ok(dir_entries) = std::fs::read_dir(workspace_dir) {
            for entry in dir_entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(data) = std::fs::read(&path) {
                    if let Ok(workspace) = serde_json::from_slice::<WorkspaceDefinition>(&data) {
                        entries.push(WorkspaceEntry {
                            name: workspace.name,
                            description: workspace.description,
                            plugins: workspace.plugins.len(),
                            plugin_kinds: workspace.plugins.iter().map(|p| p.kind.clone()).collect(),
                            path,
                        });
                    }
                }
            }
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries
    }

    fn workspace_file_path_for(workspace_dir: &Path, name: &str) -> PathBuf {
        let safe = name.trim().replace(' ', "_");
        workspace_dir.join(format!("{safe}.json"))
    }

    fn empty_workspace(name: &str) -> WorkspaceDefinition {
        WorkspaceDefinition {
            name: name.to_string(),
            description: String::new(),
            target_hz: 1000,
            plugins: Vec::new(),
            connections: Vec::new(),
            settings: WorkspaceSettings::default(),
        }
    }

    fn load_workspace_file(path: &Path) -> Result<WorkspaceDefinition, String> {
        WorkspaceDefinition::load_from_file(path)
            .map_err(|e| format!("Failed to load workspace: {e}"))
    }

    fn save_workspace_file(workspace: &WorkspaceDefinition, path: &Path) -> Result<(), String> {
        workspace
            .save_to_file(path)
            .map_err(|e| format!("Failed to save workspace: {e}"))
    }

    fn remove_old_workspace_file(old_path: &Path, new_path: &Path) -> Result<(), String> {
        if old_path == new_path {
            return Ok(());
        }
        std::fs::remove_file(old_path)
            .map_err(|e| format!("Failed to remove old workspace file: {e}"))?;
        Ok(())
    }

    pub fn new(workspace_dir: PathBuf) -> Self {
        Self {
            workspace: Self::empty_workspace("default"),
            workspace_path: PathBuf::new(),
            workspace_dirty: true,
            workspace_entries: Vec::new(),
            workspace_dir,
        }
    }

    pub fn workspace_dir(&self) -> &Path {
        &self.workspace_dir
    }

    fn normalize_frequency_unit(unit: &str) -> Result<&str, String> {
        match unit {
            "hz" | "khz" | "mhz" => Ok(unit),
            _ => Err("frequency_unit must be 'hz', 'khz', or 'mhz'".to_string()),
        }
    }

    fn normalize_period_unit(unit: &str) -> Result<&str, String> {
        match unit {
            "ns" | "us" | "ms" | "s" => Ok(unit),
            _ => Err("period_unit must be 'ns', 'us', 'ms', or 's'".to_string()),
        }
    }

    fn frequency_hz_from(value: f64, unit: &str) -> Result<f64, String> {
        let unit = Self::normalize_frequency_unit(unit)?;
        let multiplier = match unit {
            "hz" => 1.0,
            "khz" => 1_000.0,
            "mhz" => 1_000_000.0,
            _ => 1.0,
        };
        Ok(value * multiplier)
    }

    fn frequency_value_from_hz(hz: f64, unit: &str) -> Result<f64, String> {
        let unit = Self::normalize_frequency_unit(unit)?;
        let divisor = match unit {
            "hz" => 1.0,
            "khz" => 1_000.0,
            "mhz" => 1_000_000.0,
            _ => 1.0,
        };
        Ok(hz / divisor)
    }

    fn period_seconds_from(value: f64, unit: &str) -> Result<f64, String> {
        let unit = Self::normalize_period_unit(unit)?;
        let multiplier = match unit {
            "ns" => 1e-9,
            "us" => 1e-6,
            "ms" => 1e-3,
            "s" => 1.0,
            _ => 1.0,
        };
        Ok(value * multiplier)
    }

    fn period_value_from_seconds(seconds: f64, unit: &str) -> Result<f64, String> {
        let unit = Self::normalize_period_unit(unit)?;
        let divisor = match unit {
            "ns" => 1e-9,
            "us" => 1e-6,
            "ms" => 1e-3,
            "s" => 1.0,
            _ => 1.0,
        };
        Ok(seconds / divisor)
    }

    fn time_scale_and_label(period_unit: &str) -> Result<(f64, String), String> {
        let (scale, label) = match Self::normalize_period_unit(period_unit)? {
            "ns" => (1e9, "time_ns"),
            "us" => (1e6, "time_us"),
            "ms" => (1e3, "time_ms"),
            "s" => (1.0, "time_s"),
            _ => (1.0, "time_s"),
        };
        Ok((scale, label.to_string()))
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
            Self::normalize_frequency_unit(unit)?;
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
            Self::normalize_period_unit(unit)?;
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

        if settings.selected_cores.is_empty() {
            settings.selected_cores = vec![0];
        }

        if freq_changed && period_changed {
            return Err(
                "Provide either frequency_* or period_* values, not both at once.".to_string(),
            );
        }

        if freq_changed {
            let hz = Self::frequency_hz_from(settings.frequency_value, &settings.frequency_unit)?;
            let period_seconds = 1.0 / hz;
            settings.period_value = Self::period_value_from_seconds(
                period_seconds,
                &settings.period_unit,
            )?;
        } else if period_changed {
            let period_seconds =
                Self::period_seconds_from(settings.period_value, &settings.period_unit)?;
            let hz = 1.0 / period_seconds;
            settings.frequency_value =
                Self::frequency_value_from_hz(hz, &settings.frequency_unit)?;
        }

        self.workspace.settings = settings;
        Ok(())
    }

    pub fn runtime_settings(&self) -> Result<RuntimeSettings, String> {
        let settings = &self.workspace.settings;
        Self::normalize_frequency_unit(&settings.frequency_unit)?;
        Self::normalize_period_unit(&settings.period_unit)?;
        let period_seconds =
            Self::period_seconds_from(settings.period_value, &settings.period_unit)?;
        let (time_scale, time_label) = Self::time_scale_and_label(&settings.period_unit)?;
        let cores = if settings.selected_cores.is_empty() {
            vec![0]
        } else {
            settings.selected_cores.clone()
        };
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
        Self::workspace_file_path_for(&self.workspace_dir, name)
    }

    pub fn scan_workspaces(&mut self) {
        self.workspace_entries = Self::scan_workspace_entries(&self.workspace_dir);
    }

    pub fn load_workspace(&mut self, path: &Path) -> Result<(), String> {
        let loaded = Self::load_workspace_file(path)?;
        self.workspace = loaded;
        self.workspace_path = path.to_path_buf();
        self.workspace_dirty = false;
        Ok(())
    }

    pub fn save_workspace_overwrite_current(&mut self) -> Result<(), String> {
        if self.workspace_path.as_os_str().is_empty() {
            return Err("No workspace path set".to_string());
        }
        Self::save_workspace_file(&self.workspace, &self.workspace_path)?;
        self.workspace_dirty = false;
        Ok(())
    }

    pub fn save_workspace_as(&mut self, name: &str, description: &str) -> Result<(), String> {
        self.workspace.name = name.to_string();
        self.workspace.description = description.to_string();

        let path = self.workspace_file_path(name);
        let _ = std::fs::create_dir_all(&self.workspace_dir);
        Self::save_workspace_file(&self.workspace, &path)?;
        self.workspace_path = path;
        self.workspace_dirty = false;
        Ok(())
    }

    pub fn create_workspace(&mut self, name: &str, description: &str) -> Result<(), String> {
        let path = self.workspace_file_path(name);
        if path.exists() {
            return Err("Workspace already exists".to_string());
        }

        self.workspace = Self::empty_workspace(name);
        self.workspace.description = description.to_string();

        let _ = std::fs::create_dir_all(&self.workspace_dir);
        Self::save_workspace_file(&self.workspace, &path)?;
        self.workspace_path = path;
        self.workspace_dirty = false;
        Ok(())
    }

    pub fn import_workspace(&mut self, source: &Path) -> Result<(), String> {
        let loaded = Self::load_workspace_file(source)?;
        let dest_path = self.workspace_file_path(&loaded.name);
        if dest_path.exists() {
            return Err("Workspace with this name already exists".to_string());
        }
        let _ = std::fs::create_dir_all(&self.workspace_dir);
        std::fs::copy(source, &dest_path)
            .map_err(|e| format!("Failed to copy: {e}"))?;
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
        Self::save_workspace_file(&workspace, &new_path)?;
        Self::remove_old_workspace_file(&current_path, &new_path)?;
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
        std::fs::remove_file(&path)
            .map_err(|e| format!("Failed to delete workspace: {e}"))?;
        if self.workspace_path == path {
            self.workspace = Self::empty_workspace("default");
            self.workspace_path = PathBuf::new();
            self.workspace_dirty = true;
        }
        Ok(())
    }
}
