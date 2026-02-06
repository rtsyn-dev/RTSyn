use crate::state::{DetectedPlugin, InstalledPlugin, PluginManifest};
use crate::plugin_manager::PluginManager;
use crate::GuiApp;
use rtsyn_runtime::runtime::LogicMessage;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::mpsc;

impl GuiApp {
    pub(crate) fn scan_detected_plugins(&mut self) {
        let mut detected = Vec::new();
        for base in ["plugins", "app_plugins"] {
            let base = std::path::PathBuf::from(base);
            if let Ok(entries) = fs::read_dir(&base) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_dir() {
                        continue;
                    }
                    let folder_name = path.file_name().and_then(|s| s.to_str()).unwrap_or_default();
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
                    let _library_path = PluginManager::resolve_library_path(&manifest, &path);
                    detected.push(DetectedPlugin { manifest, path });
                }
            }
        }
        let mut detected_kinds: HashSet<String> = detected.iter().map(|p| p.manifest.kind.clone()).collect();
        for installed in &self.plugin_manager.installed_plugins {
            if detected_kinds.contains(&installed.manifest.kind) {
                continue;
            }
            detected.push(DetectedPlugin {
                manifest: installed.manifest.clone(),
                path: installed.path.clone(),
            });
            detected_kinds.insert(installed.manifest.kind.clone());
        }
        self.plugin_manager.detected_plugins = detected;
    }

    pub(crate) fn add_installed_plugin(&mut self, installed_index: usize) {
        let installed = match self.plugin_manager.installed_plugins.get(installed_index) {
            Some(plugin) => plugin.clone(),
            None => {
                self.status = "Invalid installed plugin".to_string();
                return;
            }
        };

        let mut config_map = serde_json::Map::new();
        for (name, value) in &installed.metadata_variables {
            config_map.insert(name.clone(), Value::from(*value));
        }
        if let Some(library_path) = &installed.library_path {
            let (tx, rx) = mpsc::channel();
            let _ = self.state_sync.logic_tx.send(LogicMessage::QueryPluginMetadata(library_path.to_string_lossy().to_string(), tx));
            if let Ok(Some((_inputs, _outputs, variables, _display_schema))) = rx.recv() {
                for (name, value) in variables {
                    config_map.insert(name, Value::from(value));
                }
            }
        }
        if installed.manifest.kind == "csv_recorder" {
            config_map.insert("separator".to_string(), Value::from(","));
            config_map.insert("path".to_string(), Value::from(Self::default_csv_path()));
            config_map.insert("input_count".to_string(), Value::from(0));
            config_map.insert("columns".to_string(), Value::Array(Vec::new()));
            config_map.insert("include_time".to_string(), Value::from(true));
            config_map.insert("path_autogen".to_string(), Value::from(true));
        } else if installed.manifest.kind == "live_plotter" {
            config_map.insert("input_count".to_string(), Value::from(0));
            config_map.insert("refresh_hz".to_string(), Value::from(60.0));
            config_map.insert("window_ms".to_string(), Value::from(10000.0));
        } else if installed.manifest.kind == "performance_monitor" {
            config_map.insert("input_count".to_string(), Value::from(0));
        } else if installed.manifest.kind == "comedi_daq" {
            config_map.insert("device_path".to_string(), Value::from("/dev/comedi0"));
            config_map.insert("scan_devices".to_string(), Value::from(false));
            config_map.insert("scan_nonce".to_string(), Value::from(0));
        }
        if let Some(library_path) = installed.library_path.as_ref() {
            config_map.insert("library_path".to_string(), Value::String(library_path.to_string_lossy().to_string()));
        }

        self.ensure_plugin_behavior_cached_with_path(&installed.manifest.kind, installed.library_path.as_ref());
        let loads_started = self.plugin_manager.plugin_behaviors.get(&installed.manifest.kind).map(|b| b.loads_started).unwrap_or(false);

        let plugin = workspace::PluginDefinition {
            id: self.plugin_manager.available_plugin_ids.pop().unwrap_or_else(|| {
                let id = self.plugin_manager.next_plugin_id;
                self.plugin_manager.next_plugin_id += 1;
                id
            }),
            kind: installed.manifest.kind.clone(),
            config: Value::Object(config_map),
            priority: 99,
            running: loads_started,
        };

        self.workspace_manager.workspace.plugins.push(plugin);
        self.status = "Installed plugin added".to_string();
        self.mark_workspace_dirty();
    }

    pub(crate) fn duplicate_plugin(&mut self, plugin_id: u64) {
        let source = match self.workspace_manager.workspace.plugins.iter().find(|p| p.id == plugin_id) {
            Some(plugin) => plugin.clone(),
            None => {
                self.show_info("Plugin", "Invalid plugin");
                return;
            }
        };
        let plugin = workspace::PluginDefinition {
            id: self.plugin_manager.available_plugin_ids.pop().unwrap_or_else(|| {
                let id = self.plugin_manager.next_plugin_id;
                self.plugin_manager.next_plugin_id += 1;
                id
            }),
            kind: source.kind,
            config: source.config,
            priority: source.priority,
            running: source.running,
        };
        let kind = plugin.kind.clone();
        self.workspace_manager.workspace.plugins.push(plugin);
        self.ensure_plugin_behavior_cached(&kind);
        self.status = "Plugin duplicated".to_string();
        self.mark_workspace_dirty();
    }

    pub(crate) fn remove_plugin(&mut self, plugin_index: usize) {
        if plugin_index >= self.workspace_manager.workspace.plugins.len() {
            self.status = "Invalid plugin selection".to_string();
            return;
        }

        let removed_id = self.workspace_manager.workspace.plugins[plugin_index].id;
        self.plugin_manager.available_plugin_ids.push(removed_id);
        
        if self.selected_plugin_id == Some(removed_id) {
            self.selected_plugin_id = None;
        }
        if self.windows.plugin_config_id == Some(removed_id) {
            self.windows.plugin_config_id = None;
            self.windows.plugin_config_open = false;
        }
        self.plotter_manager.plotters.remove(&removed_id);
        
        self.workspace_manager.workspace.plugins.remove(plugin_index);
        self.workspace_manager.workspace.connections.retain(|conn| conn.from_plugin != removed_id && conn.to_plugin != removed_id);
        let ids: Vec<u64> = self.workspace_manager.workspace.plugins.iter().map(|p| p.id).collect();
        for id in ids {
            self.sync_extendable_input_count(id);
        }
        self.recompute_plotter_ui_hz();
        self.enforce_connection_dependent();
        self.status = "Plugin removed".to_string();
        self.mark_workspace_dirty();
    }

    pub(crate) fn uninstall_plugin(&mut self, installed_index: usize) {
        let plugin = match self.plugin_manager.installed_plugins.get(installed_index) {
            Some(plugin) => plugin.clone(),
            None => {
                self.show_info("Plugin", "Invalid installed plugin");
                return;
            }
        };

        if !plugin.removable {
            self.show_info("Plugin", "Plugin is bundled and cannot be uninstalled");
            return;
        }

        let kind = plugin.manifest.kind.clone();
        let plugin_ids: Vec<u64> = self.workspace_manager.workspace.plugins.iter().filter(|p| p.kind == kind).map(|p| p.id).collect();
        
        for id in &plugin_ids {
            if self.selected_plugin_id == Some(*id) {
                self.selected_plugin_id = None;
            }
            if self.windows.plugin_config_id == Some(*id) {
                self.windows.plugin_config_id = None;
                self.windows.plugin_config_open = false;
            }
            self.plotter_manager.plotters.remove(id);
        }
        
        self.workspace_manager.workspace.plugins.retain(|p| p.kind != kind);
        self.workspace_manager.workspace.connections.retain(|conn| !plugin_ids.contains(&conn.from_plugin) && !plugin_ids.contains(&conn.to_plugin));
        
        self.plugin_manager.installed_plugins.remove(installed_index);
        self.scan_detected_plugins();
        self.show_info("Plugin", "Plugin uninstalled");
        self.persist_installed_plugins();
    }

    pub(crate) fn install_plugin_from_folder<P: AsRef<Path>>(&mut self, folder: P, removable: bool, persist: bool) {
        let manifest_path = folder.as_ref().join("plugin.toml");
        let data = match fs::read_to_string(&manifest_path) {
            Ok(content) => content,
            Err(err) => {
                self.status = format!("Failed to read plugin.toml: {err}");
                return;
            }
        };

        let manifest: PluginManifest = match toml::from_str(&data) {
            Ok(parsed) => parsed,
            Err(err) => {
                self.status = format!("Invalid plugin.toml: {err}");
                return;
            }
        };
        if manifest.kind == "comedi_daq" && !cfg!(feature = "comedi") {
            return;
        }

        let library_path = PluginManager::resolve_library_path(&manifest, folder.as_ref());
        let (mut metadata_inputs, mut metadata_outputs, mut metadata_variables, mut display_schema) = if let Some(ref lib_path) = library_path {
            let (tx, rx) = mpsc::channel();
            let _ = self.state_sync.logic_tx.send(LogicMessage::QueryPluginMetadata(lib_path.to_string_lossy().to_string(), tx));
            if let Ok(Some((inputs, outputs, vars, schema))) = rx.recv() {
                (inputs, outputs, vars, schema)
            } else {
                (vec![], vec![], vec![], None)
            }
        } else {
            (vec![], vec![], vec![], None)
        };
        if manifest.kind == "performance_monitor" {
            metadata_inputs = Vec::new();
            metadata_outputs = vec!["period_us".to_string(), "latency_us".to_string(), "jitter_us".to_string(), "realtime_violation".to_string()];
            metadata_variables = vec![("max_latency_us".to_string(), 1000.0)];
            display_schema = Some(rtsyn_plugin::ui::DisplaySchema {
                outputs: metadata_outputs.clone(),
                inputs: Vec::new(),
                variables: Vec::new(),
            });
        } else if matches!(manifest.kind.as_str(), "csv_recorder" | "live_plotter") {
            display_schema = Some(rtsyn_plugin::ui::DisplaySchema {
                outputs: Vec::new(),
                inputs: Vec::new(),
                variables: vec!["input_count".to_string(), "running".to_string()],
            });
        } else if display_schema.is_none() && (!metadata_outputs.is_empty() || !metadata_variables.is_empty()) {
            display_schema = Some(rtsyn_plugin::ui::DisplaySchema {
                outputs: metadata_outputs.clone(),
                inputs: Vec::new(),
                variables: metadata_variables.iter().map(|(name, _)| name.clone()).collect(),
            });
        }
        
        if self.plugin_manager.installed_plugins.iter().any(|p| p.manifest.kind == manifest.kind) {
            self.status = format!("Plugin '{}' is already installed", manifest.kind);
            return;
        }
        
        self.plugin_manager.installed_plugins.push(InstalledPlugin {
            manifest,
            path: folder.as_ref().to_path_buf(),
            library_path,
            removable,
            metadata_inputs,
            metadata_outputs,
            metadata_variables,
            display_schema,
        });
        self.status = "Plugin installed".to_string();
        if persist {
            self.persist_installed_plugins();
        }
    }

    pub(crate) fn refresh_installed_plugin(&mut self, kind: String, path: &Path) {
        self.plugin_manager.plugin_behaviors.remove(&kind);
        let plugin_ids: Vec<u64> = self.workspace_manager.workspace.plugins.iter().filter(|p| p.kind == kind).map(|p| p.id).collect();
        
        for id in &plugin_ids {
            if self.selected_plugin_id == Some(*id) {
                self.selected_plugin_id = None;
            }
            if self.windows.plugin_config_id == Some(*id) {
                self.windows.plugin_config_id = None;
                self.windows.plugin_config_open = false;
            }
            self.plotter_manager.plotters.remove(id);
        }
        
        if path.as_os_str().is_empty() {
            self.status = "Plugin refreshed".to_string();
            return;
        }
        
        let manifest_path = path.join("plugin.toml");
        let data = match fs::read_to_string(&manifest_path) {
            Ok(content) => content,
            Err(err) => {
                self.status = format!("Failed to read plugin.toml: {err}");
                return;
            }
        };

        let manifest: PluginManifest = match toml::from_str(&data) {
            Ok(parsed) => parsed,
            Err(err) => {
                self.status = format!("Failed to parse plugin.toml: {err}");
                return;
            }
        };

        let library_path = PluginManager::resolve_library_path(&manifest, path);
        if let Some(installed) = self.plugin_manager.installed_plugins.iter_mut().find(|plugin| plugin.manifest.kind == kind) {
            installed.manifest = manifest;
            let (tx, rx) = mpsc::channel();
            if let Some(ref lib_path) = library_path {
                let _ = self.state_sync.logic_tx.send(LogicMessage::QueryPluginMetadata(lib_path.to_string_lossy().to_string(), tx));
                if let Ok(Some((inputs, outputs, vars, display_schema))) = rx.recv() {
                    installed.metadata_inputs = inputs;
                    installed.metadata_outputs = outputs;
                    installed.metadata_variables = vars;
                    installed.display_schema = display_schema;
                }
            }
            if installed.manifest.kind == "performance_monitor" {
                installed.metadata_inputs = Vec::new();
                installed.metadata_outputs = vec!["period_us".to_string(), "latency_us".to_string(), "jitter_us".to_string(), "realtime_violation".to_string()];
                installed.metadata_variables = vec![("max_latency_us".to_string(), 1000.0)];
                installed.display_schema = Some(rtsyn_plugin::ui::DisplaySchema {
                    outputs: installed.metadata_outputs.clone(),
                    inputs: Vec::new(),
                    variables: Vec::new(),
                });
            } else if matches!(installed.manifest.kind.as_str(), "csv_recorder" | "live_plotter") {
                installed.display_schema = Some(rtsyn_plugin::ui::DisplaySchema {
                    outputs: Vec::new(),
                    inputs: Vec::new(),
                    variables: vec!["input_count".to_string(), "running".to_string()],
                });
            }
            installed.path = path.to_path_buf();
            installed.library_path = library_path;
        } else {
            let (mut metadata_inputs, mut metadata_outputs, mut metadata_variables, mut display_schema) = if let Some(ref lib_path) = library_path {
                let (tx, rx) = mpsc::channel();
                let _ = self.state_sync.logic_tx.send(LogicMessage::QueryPluginMetadata(lib_path.to_string_lossy().to_string(), tx));
                if let Ok(Some((inputs, outputs, vars, display_schema))) = rx.recv() {
                    (inputs, outputs, vars, display_schema)
                } else {
                    (vec![], vec![], vec![], None)
                }
            } else {
                (vec![], vec![], vec![], None)
            };
            if manifest.kind == "performance_monitor" {
                metadata_inputs = Vec::new();
                metadata_outputs = vec!["period_us".to_string(), "latency_us".to_string(), "jitter_us".to_string(), "realtime_violation".to_string()];
                metadata_variables = vec![("max_latency_us".to_string(), 1000.0)];
                display_schema = Some(rtsyn_plugin::ui::DisplaySchema {
                    outputs: metadata_outputs.clone(),
                    inputs: Vec::new(),
                    variables: Vec::new(),
                });
            } else if matches!(manifest.kind.as_str(), "csv_recorder" | "live_plotter") {
                display_schema = Some(rtsyn_plugin::ui::DisplaySchema {
                    outputs: Vec::new(),
                    inputs: Vec::new(),
                    variables: vec!["input_count".to_string(), "running".to_string()],
                });
            }
            self.plugin_manager.installed_plugins.push(InstalledPlugin {
                manifest,
                path: path.to_path_buf(),
                library_path,
                removable: false,
                metadata_inputs,
                metadata_outputs,
                metadata_variables,
                display_schema,
            });
        }
        self.persist_installed_plugins();
    }

    pub(crate) fn refresh_installed_library_paths(&mut self) {
        let mut changed = false;
        for installed in &mut self.plugin_manager.installed_plugins {
            let needs_update = installed.library_path.as_ref().map(|path| !path.is_file()).unwrap_or(true);
            if needs_update {
                installed.library_path = PluginManager::resolve_library_path(&installed.manifest, &installed.path);
                changed = true;
            }
        }
        if changed {
            self.persist_installed_plugins();
        }
    }

    pub(crate) fn inject_library_paths_into_workspace(&mut self) {
        let mut paths_by_kind: HashMap<String, String> = HashMap::new();
        for installed in &self.plugin_manager.installed_plugins {
            if let Some(path) = installed.library_path.as_ref() {
                if path.is_file() {
                    paths_by_kind.insert(installed.manifest.kind.clone(), path.to_string_lossy().to_string());
                }
            }
        }
        if paths_by_kind.is_empty() {
            return;
        }
        for plugin in &mut self.workspace_manager.workspace.plugins {
            if let Some(path) = paths_by_kind.get(&plugin.kind) {
                if let Value::Object(ref mut map) = plugin.config {
                    let needs_update = match map.get("library_path") {
                        Some(Value::String(existing)) => existing.is_empty() || !Path::new(existing).is_file(),
                        _ => true,
                    };
                    if needs_update {
                        map.insert("library_path".to_string(), Value::String(path.to_string()));
                    }
                }
            }
        }
    }

    pub(crate) fn load_installed_plugins(&mut self) {
        self.plugin_manager.load_installed_plugins();
    }

    pub(crate) fn persist_installed_plugins(&mut self) {
        self.plugin_manager.persist_installed_plugins();
    }
}
