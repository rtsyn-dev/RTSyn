#![allow(dead_code)]
use rtsyn_core::plugin::PluginManager;
use rtsyn_plugin::ui::PluginBehavior;
use rtsyn_runtime::LogicMessage;
use std::collections::HashMap;
use std::sync::mpsc;
use std::time::Duration;

pub struct PluginBehaviorManager {
    pub cached_behaviors: HashMap<String, PluginBehavior>,
    cached_ports: HashMap<String, (Vec<String>, Vec<String>)>,
}

impl PluginBehaviorManager {
    pub fn new() -> Self {
        Self {
            cached_behaviors: HashMap::new(),
            cached_ports: HashMap::new(),
        }
    }

    pub fn ensure_behavior_cached(
        &mut self,
        kind: &str,
        library_path: Option<&str>,
        logic_tx: &mpsc::Sender<LogicMessage>,
        plugin_manager: &PluginManager,
    ) -> Option<&PluginBehavior> {
        if !self.cached_behaviors.contains_key(kind) {
            let (tx, rx) = mpsc::channel();
            let _ = logic_tx.send(LogicMessage::QueryPluginBehavior(
                kind.to_string(),
                library_path.map(|s| s.to_string()),
                tx,
            ));
            
            if let Ok(Some(behavior)) = rx.recv_timeout(Duration::from_millis(500)) {
                // Populate ports from metadata
                if let Some(plugin) = plugin_manager.installed_plugins.iter().find(|p| p.manifest.kind == kind) {
                    self.cached_ports.insert(
                        kind.to_string(),
                        (plugin.metadata_inputs.clone(), plugin.metadata_outputs.clone())
                    );
                }
                self.cached_behaviors.insert(kind.to_string(), behavior);
            }
        }
        self.cached_behaviors.get(kind)
    }

    pub fn ports_for_kind(
        &self,
        kind: &str,
        inputs: bool,
        plugin_manager: &PluginManager,
    ) -> Vec<String> {
        if let Some((input_ports, output_ports)) = self.cached_ports.get(kind) {
            let mut ports = if inputs { input_ports.clone() } else { output_ports.clone() };
            if inputs && self.is_extendable_inputs(kind) {
                ports.extend(self.auto_extend_inputs(kind));
            }
            ports
        } else if let Some(plugin) = plugin_manager.installed_plugins.iter().find(|p| p.manifest.kind == kind) {
            let mut ports = if inputs { plugin.metadata_inputs.clone() } else { plugin.metadata_outputs.clone() };
            if inputs && self.is_extendable_inputs(kind) {
                ports.extend(self.auto_extend_inputs(kind));
            }
            ports
        } else {
            Vec::new()
        }
    }

    pub fn is_extendable_inputs(&self, kind: &str) -> bool {
        matches!(kind, "input_sum" | "input_sum_any")
    }

    pub fn auto_extend_inputs(&self, kind: &str) -> Vec<String> {
        if self.is_extendable_inputs(kind) {
            (1..=10).map(|i| format!("in_{}", i)).collect()
        } else {
            Vec::new()
        }
    }

    pub fn plugin_uses_external_window(&self, kind: &str) -> bool {
        if let Some(behavior) = self.cached_behaviors.get(kind) {
            behavior.external_window
        } else {
            kind == "live_plotter"
        }
    }
}