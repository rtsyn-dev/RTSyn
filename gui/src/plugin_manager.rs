use crate::state::{InstalledPlugin, PluginManifest, DetectedPlugin};
use rtsyn_plugin::ui::PluginBehavior;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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
            ("live_plotter", "Live Plotter", "Real-time data visualization"),
            ("performance_monitor", "Performance Monitor", "Monitors system performance"),
            #[cfg(feature = "comedi")]
            ("comedi_daq", "Comedi DAQ", "Data acquisition via Comedi"),
        ];

        for (kind, name, desc) in bundled {
            let (metadata_outputs, metadata_variables, display_schema) = match kind {
                "performance_monitor" => (
                    vec![
                        "period_us".to_string(),
                        "latency_us".to_string(),
                        "jitter_us".to_string(),
                        "realtime_violation".to_string(),
                    ],
                    vec![("max_latency_us".to_string(), 1000.0)],
                    Some(rtsyn_plugin::ui::DisplaySchema {
                        outputs: vec![
                            "period_us".to_string(),
                            "latency_us".to_string(),
                            "jitter_us".to_string(),
                            "realtime_violation".to_string(),
                        ],
                        inputs: Vec::new(),
                        variables: Vec::new(),
                    }),
                ),
                "csv_recorder" | "live_plotter" => (
                    Vec::new(),
                    Vec::new(),
                    Some(rtsyn_plugin::ui::DisplaySchema {
                        outputs: Vec::new(),
                        inputs: Vec::new(),
                        variables: vec!["input_count".to_string(), "running".to_string()],
                    }),
                ),
                _ => (Vec::new(), Vec::new(), None),
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
                metadata_inputs: Vec::new(),
                metadata_outputs,
                metadata_variables,
                display_schema,
            });
        }
    }

    pub fn persist_installed_plugins(&self) {
        let removable: Vec<_> = self.installed_plugins.iter()
            .filter(|p| p.removable)
            .cloned()
            .collect();
        
        if let Ok(data) = serde_json::to_vec_pretty(&removable) {
            let _ = fs::create_dir_all(self.install_db_path.parent().unwrap());
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
            folder.join("target/release").join(format!("lib{}.so", lib_name)),
            folder.join("target/release").join(format!("lib{}.dylib", lib_name)),
            folder.join("target/release").join(format!("{}.dll", lib_name)),
            folder.join("target/debug").join(format!("lib{}.so", lib_name)),
            folder.join("target/debug").join(format!("lib{}.dylib", lib_name)),
            folder.join("target/debug").join(format!("{}.dll", lib_name)),
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
}
