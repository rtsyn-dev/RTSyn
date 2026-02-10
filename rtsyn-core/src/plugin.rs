#[cfg(feature = "comedi")]
use comedi_daq_plugin::ComediDaqPlugin;
use csv_recorder_plugin::CsvRecorderedPlugin;
use live_plotter_plugin::LivePlotterPlugin;
use performance_monitor_plugin::PerformanceMonitorPlugin;
use rtsyn_plugin::ui::{DisplaySchema, PluginBehavior, UISchema};
use rtsyn_plugin::Plugin;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use workspace::{PluginDefinition, WorkspaceDefinition};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub kind: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub library: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    pub manifest: PluginManifest,
    pub path: PathBuf,
    pub library_path: Option<PathBuf>,
    pub removable: bool,
    pub metadata_inputs: Vec<String>,
    pub metadata_outputs: Vec<String>,
    pub metadata_variables: Vec<(String, f64)>,
    pub display_schema: Option<DisplaySchema>,
    pub ui_schema: Option<UISchema>,
}

#[derive(Debug, Clone)]
pub struct DetectedPlugin {
    pub manifest: PluginManifest,
    pub path: PathBuf,
}

pub trait PluginMetadataSource {
    fn query_plugin_metadata(
        &self,
        library_path: &str,
        timeout: Duration,
    ) -> Option<(
        Vec<String>,
        Vec<String>,
        Vec<(String, f64)>,
        Option<DisplaySchema>,
        Option<UISchema>,
    )>;

    fn query_plugin_behavior(
        &self,
        kind: &str,
        library_path: Option<&str>,
        timeout: Duration,
    ) -> Option<PluginBehavior>;
}

pub struct PluginManager {
    pub installed_plugins: Vec<InstalledPlugin>,
    pub plugin_behaviors: HashMap<String, PluginBehavior>,
    pub detected_plugins: Vec<DetectedPlugin>,
    pub next_plugin_id: u64,
    pub available_plugin_ids: Vec<u64>,
    install_db_path: PathBuf,
}

impl PluginManager {
    pub fn new(install_db_path: PathBuf) -> Self {
        let mut manager = Self {
            installed_plugins: Vec::new(),
            plugin_behaviors: HashMap::new(),
            detected_plugins: Vec::new(),
            next_plugin_id: 1,
            available_plugin_ids: Vec::new(),
            install_db_path,
        };
        manager.load_installed_plugins();
        manager
    }

    pub fn sync_next_plugin_id(&mut self, max_id: Option<u64>) {
        if let Some(max) = max_id {
            self.next_plugin_id = max + 1;
        } else {
            self.next_plugin_id = 1;
        }
    }

    pub fn load_installed_plugins(&mut self) {
        self.installed_plugins.clear();
        self.load_bundled_plugins();
        if let Ok(data) = fs::read(&self.install_db_path) {
            if let Ok(plugins) = serde_json::from_slice::<Vec<InstalledPlugin>>(&data) {
                self.installed_plugins.extend(plugins);
            }
        }
    }

    fn load_bundled_plugins(&mut self) {
        let bundled = vec![
            ("csv_recorder", "CSV Recorder", "Records data to CSV files"),
            (
                "live_plotter",
                "Live Plotter",
                "Real-time data visualization",
            ),
            (
                "performance_monitor",
                "Performance Monitor",
                "Monitors system performance",
            ),
            #[cfg(feature = "comedi")]
            ("comedi_daq", "Comedi DAQ", "Data acquisition via Comedi"),
        ];

        for (kind, name, desc) in bundled {
            let (metadata_inputs, metadata_outputs, metadata_variables, display_schema, ui_schema) =
                match kind {
                    "performance_monitor" => {
                        let plugin = PerformanceMonitorPlugin::new(0);
                        let inputs: Vec<String> =
                            plugin.inputs().iter().map(|p| p.id.0.clone()).collect();
                        let outputs: Vec<String> =
                            plugin.outputs().iter().map(|p| p.id.0.clone()).collect();
                        (
                            inputs,
                            outputs,
                            vec![("max_latency_us".to_string(), 1000.0)],
                            plugin.display_schema(),
                            plugin.ui_schema(),
                        )
                    }
                    "csv_recorder" => {
                        let plugin = CsvRecorderedPlugin::new(0);
                        let inputs: Vec<String> =
                            plugin.inputs().iter().map(|p| p.id.0.clone()).collect();
                        let outputs: Vec<String> =
                            plugin.outputs().iter().map(|p| p.id.0.clone()).collect();
                        (
                            inputs,
                            outputs,
                            Vec::new(),
                            plugin.display_schema(),
                            plugin.ui_schema(),
                        )
                    }
                    "live_plotter" => {
                        let plugin = LivePlotterPlugin::new(0);
                        let inputs: Vec<String> =
                            plugin.inputs().iter().map(|p| p.id.0.clone()).collect();
                        let outputs: Vec<String> =
                            plugin.outputs().iter().map(|p| p.id.0.clone()).collect();
                        (
                            inputs,
                            outputs,
                            Vec::new(),
                            plugin.display_schema(),
                            plugin.ui_schema(),
                        )
                    }
                    #[cfg(feature = "comedi")]
                    "comedi_daq" => {
                        let plugin = ComediDaqPlugin::new(0);
                        let inputs: Vec<String> =
                            plugin.inputs().iter().map(|p| p.id.0.clone()).collect();
                        let outputs: Vec<String> =
                            plugin.outputs().iter().map(|p| p.id.0.clone()).collect();
                        (
                            inputs,
                            outputs,
                            Vec::new(),
                            plugin.display_schema(),
                            plugin.ui_schema(),
                        )
                    }
                    _ => (Vec::new(), Vec::new(), Vec::new(), None, None),
                };

            self.installed_plugins.push(InstalledPlugin {
                manifest: PluginManifest {
                    kind: kind.to_string(),
                    name: name.to_string(),
                    description: Some(desc.to_string()),
                    version: Some("1.0.0".to_string()),
                    library: None,
                },
                path: PathBuf::new(),
                library_path: None,
                removable: false,
                metadata_inputs,
                metadata_outputs,
                metadata_variables,
                display_schema,
                ui_schema,
            });
        }
    }

    pub fn persist_installed_plugins(&self) {
        let removable: Vec<_> = self
            .installed_plugins
            .iter()
            .filter(|p| p.removable)
            .cloned()
            .collect();

        if let Ok(data) = serde_json::to_vec_pretty(&removable) {
            if let Some(parent) = self.install_db_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&self.install_db_path, data);
        }
    }

    pub fn refresh_library_paths(&mut self) {
        for plugin in &mut self.installed_plugins {
            if plugin.removable {
                plugin.library_path = Self::resolve_library_path(&plugin.manifest, &plugin.path);
            }
        }
    }

    pub fn resolve_library_path(manifest: &PluginManifest, folder: &Path) -> Option<PathBuf> {
        let lib_name = manifest.kind.replace('-', "_");
        let candidates = [
            folder
                .join("target/release")
                .join(format!("lib{}.so", lib_name)),
            folder
                .join("target/release")
                .join(format!("lib{}.dylib", lib_name)),
            folder
                .join("target/release")
                .join(format!("{}.dll", lib_name)),
            folder
                .join("target/debug")
                .join(format!("lib{}.so", lib_name)),
            folder
                .join("target/debug")
                .join(format!("lib{}.dylib", lib_name)),
            folder
                .join("target/debug")
                .join(format!("{}.dll", lib_name)),
        ];
        candidates.into_iter().find(|p| p.exists())
    }

    pub fn build_plugin(folder: &Path) -> bool {
        Command::new("cargo")
            .arg("build")
            .arg("--release")
            .current_dir(folder)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    pub fn workspace_root() -> Option<PathBuf> {
        std::env::current_dir().ok()
    }

    pub fn plugin_api_source_path() -> Option<PathBuf> {
        Self::workspace_root().map(|root| {
            root.join("..")
                .join("rtsyn-plugin")
                .join("src")
                .join("lib.rs")
        })
    }

    pub fn library_is_outdated(library_path: &Path) -> bool {
        let Ok(lib_meta) = std::fs::metadata(library_path) else {
            return true;
        };
        let Ok(lib_mtime) = lib_meta.modified() else {
            return false;
        };
        let Some(api_path) = Self::plugin_api_source_path() else {
            return false;
        };
        let Ok(api_meta) = std::fs::metadata(api_path) else {
            return false;
        };
        let Ok(api_mtime) = api_meta.modified() else {
            return false;
        };
        api_mtime > lib_mtime
    }

    pub fn scan_detected_plugins(&mut self) {
        self.detected_plugins.clear();

        if let Some(workspace_root) = Self::workspace_root() {
            let plugins_dir = workspace_root.join("rtsyn-plugins");
            if let Ok(entries) = fs::read_dir(&plugins_dir) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        let manifest_path = entry.path().join("plugin.toml");
                        if manifest_path.exists() {
                            if let Ok(data) = fs::read_to_string(&manifest_path) {
                                if let Ok(manifest) = toml::from_str::<PluginManifest>(&data) {
                                    self.detected_plugins.push(DetectedPlugin {
                                        manifest,
                                        path: entry.path(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn display_kind(kind: &str) -> String {
        kind.split('_')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().chain(chars).collect(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn refresh_installed_plugin(
        &mut self,
        kind: &str,
        path: &Path,
        metadata: &impl PluginMetadataSource,
    ) -> Result<(), String> {
        self.plugin_behaviors.remove(kind);

        if path.as_os_str().is_empty() {
            return Ok(());
        }

        let manifest_path = path.join("plugin.toml");
        let data = std::fs::read_to_string(&manifest_path)
            .map_err(|err| format!("Failed to read plugin.toml: {err}"))?;
        let manifest: PluginManifest =
            toml::from_str(&data).map_err(|err| format!("Failed to parse plugin.toml: {err}"))?;

        let library_path = PluginManager::resolve_library_path(&manifest, path);

        if let Some(installed) = self
            .installed_plugins
            .iter_mut()
            .find(|plugin| plugin.manifest.kind == kind)
        {
            installed.manifest = manifest;
            if let Some(ref lib_path) = library_path {
                let lib_path_str = lib_path.to_string_lossy();
                if let Some((inputs, outputs, vars, display_schema, ui_schema)) =
                    metadata.query_plugin_metadata(lib_path_str.as_ref(), Duration::from_secs(2))
                {
                    installed.metadata_inputs = inputs;
                    installed.metadata_outputs = outputs;
                    installed.metadata_variables = vars;
                    installed.display_schema = display_schema;
                    installed.ui_schema = ui_schema;
                }
            }
            if installed.display_schema.is_none()
                && (!installed.metadata_outputs.is_empty()
                    || !installed.metadata_variables.is_empty())
            {
                installed.display_schema = Some(DisplaySchema {
                    outputs: installed.metadata_outputs.clone(),
                    inputs: Vec::new(),
                    variables: installed
                        .metadata_variables
                        .iter()
                        .map(|(name, _)| name.clone())
                        .collect(),
                });
            }
            installed.path = path.to_path_buf();
            installed.library_path = library_path;
        } else {
            let (
                metadata_inputs,
                metadata_outputs,
                metadata_variables,
                mut display_schema,
                ui_schema,
            ) = if let Some(ref lib_path) = library_path {
                let lib_path_str = lib_path.to_string_lossy();
                match metadata.query_plugin_metadata(lib_path_str.as_ref(), Duration::from_secs(2))
                {
                    Some((inputs, outputs, vars, display_schema, ui_schema)) => {
                        (inputs, outputs, vars, display_schema, ui_schema)
                    }
                    None => (vec![], vec![], vec![], None, None),
                }
            } else {
                (vec![], vec![], vec![], None, None)
            };
            if display_schema.is_none()
                && (!metadata_outputs.is_empty() || !metadata_variables.is_empty())
            {
                display_schema = Some(DisplaySchema {
                    outputs: metadata_outputs.clone(),
                    inputs: Vec::new(),
                    variables: metadata_variables
                        .iter()
                        .map(|(name, _)| name.clone())
                        .collect(),
                });
            }
            self.installed_plugins.push(InstalledPlugin {
                manifest,
                path: path.to_path_buf(),
                library_path,
                removable: false,
                metadata_inputs,
                metadata_outputs,
                metadata_variables,
                display_schema,
                ui_schema,
            });
        }

        self.persist_installed_plugins();
        Ok(())
    }

    pub fn scan_detected_plugins_in(&mut self, bases: &[&str]) {
        let mut detected = Vec::new();
        for base in bases {
            let base = PathBuf::from(base);
            if let Ok(entries) = fs::read_dir(&base) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_dir() {
                        continue;
                    }
                    let folder_name = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or_default();
                    if folder_name.eq_ignore_ascii_case("template") {
                        continue;
                    }
                    let manifest_path = path.join("plugin.toml");
                    if !manifest_path.is_file() {
                        continue;
                    }
                    let data = match fs::read_to_string(&manifest_path) {
                        Ok(content) => content,
                        Err(_) => continue,
                    };
                    let manifest: PluginManifest = match toml::from_str(&data) {
                        Ok(parsed) => parsed,
                        Err(_) => continue,
                    };
                    if manifest.kind == "comedi_daq" && !cfg!(feature = "comedi") {
                        continue;
                    }
                    detected.push(DetectedPlugin { manifest, path });
                }
            }
        }
        let mut detected_kinds: HashSet<String> =
            detected.iter().map(|p| p.manifest.kind.clone()).collect();
        for installed in &self.installed_plugins {
            if detected_kinds.contains(&installed.manifest.kind) {
                continue;
            }
            detected.push(DetectedPlugin {
                manifest: installed.manifest.clone(),
                path: installed.path.clone(),
            });
            detected_kinds.insert(installed.manifest.kind.clone());
        }
        self.detected_plugins = detected;
    }

    pub fn install_plugin_from_folder(
        &mut self,
        folder: &Path,
        removable: bool,
        persist: bool,
        metadata: &impl PluginMetadataSource,
    ) -> Result<(), String> {
        let manifest_path = folder.join("plugin.toml");
        let data = fs::read_to_string(&manifest_path)
            .map_err(|err| format!("Failed to read plugin.toml: {err}"))?;

        let manifest: PluginManifest =
            toml::from_str(&data).map_err(|err| format!("Invalid plugin.toml: {err}"))?;
        if manifest.kind == "comedi_daq" && !cfg!(feature = "comedi") {
            return Err("comedi_daq is not available without the comedi feature".to_string());
        }

        let library_path = PluginManager::resolve_library_path(&manifest, folder);
        let (metadata_inputs, metadata_outputs, metadata_variables, mut display_schema, ui_schema) =
            if let Some(ref lib_path) = library_path {
                let lib_path_str = lib_path.to_string_lossy();
                match metadata.query_plugin_metadata(lib_path_str.as_ref(), Duration::from_secs(2))
                {
                    Some((inputs, outputs, vars, schema, ui_sch)) => {
                        (inputs, outputs, vars, schema, ui_sch)
                    }
                    None => (vec![], vec![], vec![], None, None),
                }
            } else {
                (vec![], vec![], vec![], None, None)
            };
        if display_schema.is_none()
            && (!metadata_outputs.is_empty() || !metadata_variables.is_empty())
        {
            display_schema = Some(DisplaySchema {
                outputs: metadata_outputs.clone(),
                inputs: Vec::new(),
                variables: metadata_variables
                    .iter()
                    .map(|(name, _)| name.clone())
                    .collect(),
            });
        }

        if self
            .installed_plugins
            .iter()
            .any(|p| p.manifest.kind == manifest.kind)
        {
            return Err(format!("Plugin '{}' is already installed", manifest.kind));
        }

        self.installed_plugins.push(InstalledPlugin {
            manifest,
            path: folder.to_path_buf(),
            library_path,
            removable,
            metadata_inputs,
            metadata_outputs,
            metadata_variables,
            display_schema,
            ui_schema,
        });
        if persist {
            self.persist_installed_plugins();
        }
        Ok(())
    }

    pub fn uninstall_plugin_by_index(&mut self, index: usize) -> Result<InstalledPlugin, String> {
        let plugin = self
            .installed_plugins
            .get(index)
            .cloned()
            .ok_or_else(|| "Invalid installed plugin".to_string())?;

        if !plugin.removable {
            return Err("Plugin is bundled and cannot be uninstalled".to_string());
        }
        self.installed_plugins.remove(index);
        self.persist_installed_plugins();
        Ok(plugin)
    }

    pub fn refresh_installed_library_paths(&mut self) -> bool {
        let mut changed = false;
        for installed in &mut self.installed_plugins {
            let needs_update = installed
                .library_path
                .as_ref()
                .map(|path| !path.is_file())
                .unwrap_or(true);
            if needs_update {
                installed.library_path =
                    PluginManager::resolve_library_path(&installed.manifest, &installed.path);
                changed = true;
            }
        }
        if changed {
            self.persist_installed_plugins();
        }
        changed
    }

    pub fn inject_library_paths_into_workspace(&self, workspace: &mut WorkspaceDefinition) {
        let mut paths_by_kind: HashMap<String, String> = HashMap::new();
        for installed in &self.installed_plugins {
            if let Some(path) = installed.library_path.as_ref() {
                if path.is_file() {
                    paths_by_kind.insert(
                        installed.manifest.kind.clone(),
                        path.to_string_lossy().to_string(),
                    );
                }
            }
        }
        if paths_by_kind.is_empty() {
            return;
        }
        for plugin in &mut workspace.plugins {
            if let Some(path) = paths_by_kind.get(&plugin.kind) {
                if let Value::Object(ref mut map) = plugin.config {
                    let needs_update = match map.get("library_path") {
                        Some(Value::String(existing)) => {
                            existing.is_empty() || !Path::new(existing).is_file()
                        }
                        _ => true,
                    };
                    if needs_update {
                        map.insert("library_path".to_string(), Value::String(path.to_string()));
                    }
                }
            }
        }
    }

    pub fn add_installed_plugin_to_workspace(
        &mut self,
        installed_index: usize,
        workspace: &mut WorkspaceDefinition,
        metadata: &impl PluginMetadataSource,
    ) -> Result<u64, String> {
        let installed = self
            .installed_plugins
            .get(installed_index)
            .cloned()
            .ok_or_else(|| "Invalid installed plugin".to_string())?;

        let mut config_map = serde_json::Map::new();
        for (name, value) in &installed.metadata_variables {
            config_map.insert(name.clone(), Value::from(*value));
        }
        if let Some(library_path) = &installed.library_path {
            if installed.removable && PluginManager::library_is_outdated(library_path) {
                return Err(
                    "Plugin library is out of date. Rebuild or reinstall the plugin.".to_string(),
                );
            }
            let lib_path_str = library_path.to_string_lossy();
            if let Some((_, _, variables, _, _)) =
                metadata.query_plugin_metadata(lib_path_str.as_ref(), Duration::from_secs(2))
            {
                for (name, value) in variables {
                    config_map.insert(name, Value::from(value));
                }
            }
        }
        if let Some(schema) = installed.ui_schema.as_ref() {
            for field in &schema.fields {
                if config_map.contains_key(&field.key) {
                    continue;
                }
                if let Some(default) = field.default.as_ref() {
                    config_map.insert(field.key.clone(), default.clone());
                }
            }
        }
        if is_extendable_inputs(&installed.manifest.kind) {
            config_map
                .entry("input_count".to_string())
                .or_insert_with(|| Value::from(0));
        }
        if installed.manifest.kind == "csv_recorder" {
            config_map
                .entry("columns".to_string())
                .or_insert_with(|| Value::Array(Vec::new()));
            config_map
                .entry("path".to_string())
                .or_insert_with(|| Value::from(""));
            config_map
                .entry("path_autogen".to_string())
                .or_insert_with(|| Value::from(true));
        } else if installed.manifest.kind == "comedi_daq" {
            config_map
                .entry("scan_nonce".to_string())
                .or_insert_with(|| Value::from(0));
        }
        if let Some(library_path) = installed.library_path.as_ref() {
            config_map.insert(
                "library_path".to_string(),
                Value::String(library_path.to_string_lossy().to_string()),
            );
        }

        let loads_started = self
            .plugin_behaviors
            .get(&installed.manifest.kind)
            .map(|b| b.loads_started)
            .unwrap_or(false);

        let id = self.available_plugin_ids.pop().unwrap_or_else(|| {
            let id = self.next_plugin_id;
            self.next_plugin_id += 1;
            id
        });

        let plugin = PluginDefinition {
            id,
            kind: installed.manifest.kind.clone(),
            config: Value::Object(config_map),
            priority: 99,
            running: loads_started,
        };

        workspace.plugins.push(plugin);
        Ok(id)
    }

    pub fn duplicate_plugin_in_workspace(
        &mut self,
        workspace: &mut WorkspaceDefinition,
        plugin_id: u64,
    ) -> Result<u64, String> {
        let source = workspace
            .plugins
            .iter()
            .find(|p| p.id == plugin_id)
            .cloned()
            .ok_or_else(|| "Invalid plugin".to_string())?;
        let id = self.available_plugin_ids.pop().unwrap_or_else(|| {
            let id = self.next_plugin_id;
            self.next_plugin_id += 1;
            id
        });
        let plugin = PluginDefinition {
            id,
            kind: source.kind,
            config: source.config,
            priority: source.priority,
            running: source.running,
        };
        workspace.plugins.push(plugin);
        Ok(id)
    }

    pub fn remove_plugin_from_workspace(
        &mut self,
        workspace: &mut WorkspaceDefinition,
        plugin_id: u64,
    ) -> Result<(), String> {
        let index = workspace
            .plugins
            .iter()
            .position(|p| p.id == plugin_id)
            .ok_or_else(|| "Plugin not found".to_string())?;
        workspace.plugins.remove(index);
        workspace
            .connections
            .retain(|conn| conn.from_plugin != plugin_id && conn.to_plugin != plugin_id);
        self.available_plugin_ids.push(plugin_id);
        Ok(())
    }

    pub fn remove_plugins_by_kind_from_workspace(
        &mut self,
        workspace: &mut WorkspaceDefinition,
        kind: &str,
    ) -> Vec<u64> {
        let removed_ids: Vec<u64> = workspace
            .plugins
            .iter()
            .filter(|p| p.kind == kind)
            .map(|p| p.id)
            .collect();
        if removed_ids.is_empty() {
            return removed_ids;
        }
        workspace.plugins.retain(|p| p.kind != kind);
        workspace.connections.retain(|conn| {
            !removed_ids.contains(&conn.from_plugin) && !removed_ids.contains(&conn.to_plugin)
        });
        self.available_plugin_ids
            .extend(removed_ids.iter().copied());
        removed_ids
    }
}

pub struct PluginCatalog {
    pub manager: PluginManager,
}

impl PluginCatalog {
    pub fn new(install_db_path: PathBuf) -> Self {
        Self {
            manager: PluginManager::new(install_db_path),
        }
    }

    pub fn scan_detected_plugins(&mut self) {
        let mut detected = Vec::new();
        for base in ["plugins", "app_plugins", "rtsyn-plugins"] {
            let base = PathBuf::from(base);
            if let Ok(entries) = fs::read_dir(&base) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_dir() {
                        continue;
                    }
                    let folder_name = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or_default();
                    if folder_name.eq_ignore_ascii_case("template") {
                        continue;
                    }
                    let manifest_path = path.join("plugin.toml");
                    if !manifest_path.is_file() {
                        continue;
                    }
                    let data = match fs::read_to_string(&manifest_path) {
                        Ok(content) => content,
                        Err(_) => continue,
                    };
                    let manifest: PluginManifest = match toml::from_str(&data) {
                        Ok(parsed) => parsed,
                        Err(_) => continue,
                    };
                    if manifest.kind == "comedi_daq" && !cfg!(feature = "comedi") {
                        continue;
                    }
                    detected.push(DetectedPlugin { manifest, path });
                }
            }
        }
        let mut detected_kinds: HashSet<String> =
            detected.iter().map(|p| p.manifest.kind.clone()).collect();
        for installed in &self.manager.installed_plugins {
            if detected_kinds.contains(&installed.manifest.kind) {
                continue;
            }
            detected.push(DetectedPlugin {
                manifest: installed.manifest.clone(),
                path: installed.path.clone(),
            });
            detected_kinds.insert(installed.manifest.kind.clone());
        }
        self.manager.detected_plugins = detected;
    }

    pub fn install_plugin_from_folder<P: AsRef<Path>>(
        &mut self,
        folder: P,
        removable: bool,
        persist: bool,
        metadata: &impl PluginMetadataSource,
    ) -> Result<(), String> {
        let manifest_path = folder.as_ref().join("plugin.toml");
        let data = fs::read_to_string(&manifest_path)
            .map_err(|err| format!("Failed to read plugin.toml: {err}"))?;

        let manifest: PluginManifest =
            toml::from_str(&data).map_err(|err| format!("Invalid plugin.toml: {err}"))?;
        if manifest.kind == "comedi_daq" && !cfg!(feature = "comedi") {
            return Err("comedi_daq is not available without the comedi feature".to_string());
        }

        let library_path = PluginManager::resolve_library_path(&manifest, folder.as_ref());
        let (metadata_inputs, metadata_outputs, metadata_variables, mut display_schema, ui_schema) =
            if let Some(ref lib_path) = library_path {
                let lib_path_str = lib_path.to_string_lossy();
                match metadata.query_plugin_metadata(lib_path_str.as_ref(), Duration::from_secs(2))
                {
                    Some((inputs, outputs, vars, schema, ui_sch)) => {
                        (inputs, outputs, vars, schema, ui_sch)
                    }
                    None => (vec![], vec![], vec![], None, None),
                }
            } else {
                (vec![], vec![], vec![], None, None)
            };
        if display_schema.is_none()
            && (!metadata_outputs.is_empty() || !metadata_variables.is_empty())
        {
            display_schema = Some(DisplaySchema {
                outputs: metadata_outputs.clone(),
                inputs: Vec::new(),
                variables: metadata_variables
                    .iter()
                    .map(|(name, _)| name.clone())
                    .collect(),
            });
        }

        if self
            .manager
            .installed_plugins
            .iter()
            .any(|p| p.manifest.kind == manifest.kind)
        {
            return Err(format!("Plugin '{}' is already installed", manifest.kind));
        }

        self.manager.installed_plugins.push(InstalledPlugin {
            manifest,
            path: folder.as_ref().to_path_buf(),
            library_path,
            removable,
            metadata_inputs,
            metadata_outputs,
            metadata_variables,
            display_schema,
            ui_schema,
        });
        if persist {
            self.manager.persist_installed_plugins();
        }
        Ok(())
    }

    pub fn uninstall_plugin_by_kind(
        &mut self,
        kind_or_name: &str,
    ) -> Result<InstalledPlugin, String> {
        let index = self
            .manager
            .installed_plugins
            .iter()
            .position(|p| {
                p.manifest.kind == kind_or_name
                    || p.manifest.name.eq_ignore_ascii_case(kind_or_name)
            })
            .ok_or_else(|| "Plugin not installed".to_string())?;
        let plugin = self.manager.installed_plugins[index].clone();
        if !plugin.removable {
            return Err("Plugin is bundled and cannot be uninstalled".to_string());
        }
        self.manager.installed_plugins.remove(index);
        self.manager.persist_installed_plugins();
        Ok(plugin)
    }

    pub fn reinstall_plugin_by_kind(
        &mut self,
        kind_or_name: &str,
        metadata: &impl PluginMetadataSource,
    ) -> Result<(), String> {
        let index = self
            .manager
            .installed_plugins
            .iter()
            .position(|p| {
                p.manifest.kind == kind_or_name
                    || p.manifest.name.eq_ignore_ascii_case(kind_or_name)
            })
            .ok_or_else(|| "Plugin not installed".to_string())?;
        let path = self.manager.installed_plugins[index].path.clone();
        if path.as_os_str().is_empty() {
            return Err("Plugin path is not set".to_string());
        }
        let kind = self.manager.installed_plugins[index].manifest.kind.clone();
        if self.manager.installed_plugins[index].removable {
            if !PluginManager::build_plugin(&path) {
                return Err("Plugin rebuild failed".to_string());
            }
        }
        self.manager
            .refresh_installed_plugin(&kind, &path, metadata)
    }

    pub fn rebuild_plugin_by_kind(&mut self, kind_or_name: &str) -> Result<(), String> {
        let index = self
            .manager
            .installed_plugins
            .iter()
            .position(|p| {
                p.manifest.kind == kind_or_name
                    || p.manifest.name.eq_ignore_ascii_case(kind_or_name)
            })
            .ok_or_else(|| "Plugin not installed".to_string())?;
        let path = self.manager.installed_plugins[index].path.clone();
        if path.as_os_str().is_empty() {
            return Err("Plugin path is not set".to_string());
        }
        if !self.manager.installed_plugins[index].removable {
            return Err("Bundled plugins cannot be rebuilt".to_string());
        }
        if !PluginManager::build_plugin(&path) {
            return Err("Plugin rebuild failed".to_string());
        }
        Ok(())
    }

    pub fn remove_plugins_by_kind_from_workspace(
        &mut self,
        kind: &str,
        workspace: &mut WorkspaceDefinition,
    ) -> Vec<u64> {
        let removed_ids: Vec<u64> = workspace
            .plugins
            .iter()
            .filter(|p| p.kind == kind)
            .map(|p| p.id)
            .collect();
        if removed_ids.is_empty() {
            return removed_ids;
        }
        workspace.plugins.retain(|p| p.kind != kind);
        workspace.connections.retain(|conn| {
            !removed_ids.contains(&conn.from_plugin) && !removed_ids.contains(&conn.to_plugin)
        });
        self.manager
            .available_plugin_ids
            .extend(removed_ids.iter().copied());
        removed_ids
    }

    pub fn add_installed_plugin_to_workspace(
        &mut self,
        kind_or_name: &str,
        workspace: &mut WorkspaceDefinition,
        metadata: &impl PluginMetadataSource,
    ) -> Result<u64, String> {
        let installed = self
            .manager
            .installed_plugins
            .iter()
            .find(|p| {
                p.manifest.kind == kind_or_name
                    || p.manifest.name.eq_ignore_ascii_case(kind_or_name)
            })
            .cloned()
            .ok_or_else(|| "Plugin is not installed".to_string())?;

        let mut config_map = serde_json::Map::new();
        for (name, value) in &installed.metadata_variables {
            config_map.insert(name.clone(), Value::from(*value));
        }
        if let Some(library_path) = &installed.library_path {
            if installed.removable && PluginManager::library_is_outdated(library_path) {
                return Err(
                    "Plugin library is out of date. Rebuild or reinstall the plugin.".to_string(),
                );
            }
            let lib_path_str = library_path.to_string_lossy();
            if let Some((_, _, variables, _, _)) =
                metadata.query_plugin_metadata(lib_path_str.as_ref(), Duration::from_secs(2))
            {
                for (name, value) in variables {
                    config_map.insert(name, Value::from(value));
                }
            }
        }
        if let Some(schema) = installed.ui_schema.as_ref() {
            for field in &schema.fields {
                if config_map.contains_key(&field.key) {
                    continue;
                }
                if let Some(default) = field.default.as_ref() {
                    config_map.insert(field.key.clone(), default.clone());
                }
            }
        }
        if is_extendable_inputs(&installed.manifest.kind) {
            config_map
                .entry("input_count".to_string())
                .or_insert_with(|| Value::from(0));
        }
        if installed.manifest.kind == "csv_recorder" {
            config_map
                .entry("columns".to_string())
                .or_insert_with(|| Value::Array(Vec::new()));
            config_map
                .entry("path".to_string())
                .or_insert_with(|| Value::from(""));
            config_map
                .entry("path_autogen".to_string())
                .or_insert_with(|| Value::from(true));
        } else if installed.manifest.kind == "comedi_daq" {
            config_map
                .entry("scan_nonce".to_string())
                .or_insert_with(|| Value::from(0));
        }
        if let Some(library_path) = installed.library_path.as_ref() {
            config_map.insert(
                "library_path".to_string(),
                Value::String(library_path.to_string_lossy().to_string()),
            );
        }

        let loads_started = metadata
            .query_plugin_behavior(
                &installed.manifest.kind,
                installed
                    .library_path
                    .as_ref()
                    .map(|p| p.to_string_lossy())
                    .as_deref(),
                Duration::from_secs(2),
            )
            .map(|b| b.loads_started)
            .unwrap_or(false);

        let id = self.manager.available_plugin_ids.pop().unwrap_or_else(|| {
            let id = self.manager.next_plugin_id;
            self.manager.next_plugin_id += 1;
            id
        });

        let plugin = PluginDefinition {
            id,
            kind: installed.manifest.kind.clone(),
            config: Value::Object(config_map),
            priority: 99,
            running: loads_started,
        };

        workspace.plugins.push(plugin);
        Ok(id)
    }

    pub fn remove_plugin_from_workspace(
        &mut self,
        plugin_id: u64,
        workspace: &mut WorkspaceDefinition,
    ) -> Result<(), String> {
        let index = workspace
            .plugins
            .iter()
            .position(|p| p.id == plugin_id)
            .ok_or_else(|| "Plugin not found".to_string())?;
        workspace.plugins.remove(index);
        workspace
            .connections
            .retain(|conn| conn.from_plugin != plugin_id && conn.to_plugin != plugin_id);
        self.manager.available_plugin_ids.push(plugin_id);
        Ok(())
    }

    pub fn sync_ids_from_workspace(&mut self, workspace: &WorkspaceDefinition) {
        let max_id = workspace.plugins.iter().map(|p| p.id).max();
        self.manager.sync_next_plugin_id(max_id);
    }

    pub fn list_installed(&self) -> &[InstalledPlugin] {
        &self.manager.installed_plugins
    }

    pub fn refresh_library_paths(&mut self) {
        self.manager.refresh_library_paths();
    }
}

pub fn is_extendable_inputs(kind: &str) -> bool {
    matches!(kind, "csv_recorder" | "live_plotter")
}

pub fn plugin_display_name(
    installed: &[InstalledPlugin],
    workspace: &WorkspaceDefinition,
    plugin_id: u64,
) -> String {
    let name_by_kind: HashMap<String, String> = installed
        .iter()
        .map(|plugin| (plugin.manifest.kind.clone(), plugin.manifest.name.clone()))
        .collect();
    workspace
        .plugins
        .iter()
        .find(|plugin| plugin.id == plugin_id)
        .map(|plugin| {
            name_by_kind
                .get(&plugin.kind)
                .cloned()
                .unwrap_or_else(|| PluginManager::display_kind(&plugin.kind))
        })
        .unwrap_or_else(|| "plugin".to_string())
}

pub fn empty_workspace() -> WorkspaceDefinition {
    WorkspaceDefinition {
        name: "cli".to_string(),
        description: "CLI workspace".to_string(),
        target_hz: 1000,
        plugins: Vec::new(),
        connections: Vec::new(),
        settings: workspace::WorkspaceSettings::default(),
    }
}

impl PluginCatalog {
    pub fn inject_library_paths_into_workspace(&self, workspace: &mut WorkspaceDefinition) {
        let mut paths_by_kind: HashMap<String, String> = HashMap::new();
        for installed in &self.manager.installed_plugins {
            if let Some(path) = installed.library_path.as_ref() {
                if path.is_file() {
                    paths_by_kind.insert(
                        installed.manifest.kind.clone(),
                        path.to_string_lossy().to_string(),
                    );
                }
            }
        }
        if paths_by_kind.is_empty() {
            return;
        }
        for plugin in &mut workspace.plugins {
            if let Some(path) = paths_by_kind.get(&plugin.kind) {
                if let Value::Object(ref mut map) = plugin.config {
                    let needs_update = match map.get("library_path") {
                        Some(Value::String(existing)) => {
                            existing.is_empty() || !Path::new(existing).is_file()
                        }
                        _ => true,
                    };
                    if needs_update {
                        map.insert("library_path".to_string(), Value::String(path.to_string()));
                    }
                }
            }
        }
    }
}
