#![allow(dead_code)]
use rtsyn_core::plugin::PluginManager;
use rtsyn_plugin::ui::PluginBehavior;
use rtsyn_runtime::LogicMessage;
use std::collections::HashMap;
use std::sync::mpsc;
use std::time::Duration;

/// Manages plugin behavior caching and metadata for the GUI application.
/// 
/// This manager handles caching of plugin behaviors and port information to avoid
/// repeated queries to the logic thread. It provides efficient access to plugin
/// metadata including input/output ports and behavioral characteristics.
pub struct PluginBehaviorManager {
    /// Cache of plugin behaviors indexed by plugin kind
    pub cached_behaviors: HashMap<String, PluginBehavior>,
    /// Cache of plugin port information (inputs, outputs) indexed by plugin kind
    cached_ports: HashMap<String, (Vec<String>, Vec<String>)>,
}

impl PluginBehaviorManager {
    /// Creates a new PluginBehaviorManager with empty caches.
    /// 
    /// # Returns
    /// A new instance with initialized but empty behavior and port caches.
    pub fn new() -> Self {
        Self {
            cached_behaviors: HashMap::new(),
            cached_ports: HashMap::new(),
        }
    }

    /// Ensures a plugin behavior is cached and returns a reference to it.
    /// 
    /// This function checks if the behavior for a given plugin kind is already cached.
    /// If not, it queries the logic thread for the behavior and caches both the behavior
    /// and port information for future use.
    /// 
    /// # Parameters
    /// - `kind`: The plugin kind/type to query
    /// - `library_path`: Optional path to the plugin library
    /// - `logic_tx`: Sender channel to communicate with the logic thread
    /// - `plugin_manager`: Reference to the plugin manager for metadata access
    /// 
    /// # Returns
    /// - `Some(&PluginBehavior)`: Reference to the cached behavior if available
    /// - `None`: If the behavior query failed or timed out
    /// 
    /// # Behavior
    /// - Uses a 500ms timeout for behavior queries to prevent UI blocking
    /// - Caches both behavior and port information from plugin metadata
    /// - Populates port cache from plugin manager's installed plugins
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

    /// Retrieves the input or output ports for a specific plugin kind.
    /// 
    /// This function returns the list of ports (inputs or outputs) for a given plugin
    /// kind, with support for auto-extending inputs for certain plugin types.
    /// 
    /// # Parameters
    /// - `kind`: The plugin kind/type to query
    /// - `inputs`: If true, returns input ports; if false, returns output ports
    /// - `plugin_manager`: Reference to the plugin manager for metadata fallback
    /// 
    /// # Returns
    /// A vector of port names for the specified plugin kind and direction.
    /// 
    /// # Behavior
    /// - First attempts to use cached port information
    /// - Falls back to plugin manager metadata if not cached
    /// - Auto-extends input ports for extendable plugin types (e.g., "input_sum")
    /// - Returns empty vector if plugin kind is not found
    /// 
    /// # Auto-Extension
    /// For plugins with extendable inputs, automatically adds numbered ports (in_1 to in_10).
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

    /// Determines if a plugin kind supports auto-extending input ports.
    /// 
    /// This function checks if the specified plugin kind supports dynamic input
    /// port extension, allowing for additional numbered input ports beyond the
    /// base configuration.
    /// 
    /// # Parameters
    /// - `kind`: The plugin kind/type to check
    /// 
    /// # Returns
    /// - `true`: If the plugin supports auto-extending inputs
    /// - `false`: If the plugin has a fixed set of input ports
    /// 
    /// # Supported Types
    /// Currently supports auto-extension for:
    /// - "input_sum": Summation plugins that can accept multiple inputs
    /// - "input_sum_any": Flexible summation plugins
    pub fn is_extendable_inputs(&self, kind: &str) -> bool {
        matches!(kind, "input_sum" | "input_sum_any")
    }

    /// Generates auto-extended input port names for extendable plugin types.
    /// 
    /// This function creates additional numbered input ports for plugins that
    /// support dynamic input extension, providing a standardized naming scheme.
    /// 
    /// # Parameters
    /// - `kind`: The plugin kind/type to generate ports for
    /// 
    /// # Returns
    /// A vector of auto-generated port names, or empty vector if not extendable.
    /// 
    /// # Port Naming
    /// - Generates ports named "in_1" through "in_10"
    /// - Only generates ports for plugin kinds that support extension
    /// - Returns empty vector for non-extendable plugin types
    pub fn auto_extend_inputs(&self, kind: &str) -> Vec<String> {
        if self.is_extendable_inputs(kind) {
            (1..=10).map(|i| format!("in_{}", i)).collect()
        } else {
            Vec::new()
        }
    }

    /// Determines if a plugin uses an external window for its interface.
    /// 
    /// This function checks whether a plugin kind requires or uses an external
    /// window for its user interface, rather than being embedded in the main GUI.
    /// 
    /// # Parameters
    /// - `kind`: The plugin kind/type to check
    /// 
    /// # Returns
    /// - `true`: If the plugin uses an external window
    /// - `false`: If the plugin is embedded in the main GUI
    /// 
    /// # Behavior
    /// - First checks cached behavior information if available
    /// - Falls back to hardcoded knowledge for known plugin types
    /// - Currently recognizes "live_plotter" as using external windows
    /// - Defaults to embedded behavior for unknown plugin types
    pub fn plugin_uses_external_window(&self, kind: &str) -> bool {
        if let Some(behavior) = self.cached_behaviors.get(kind) {
            behavior.external_window
        } else {
            kind == "live_plotter"
        }
    }
}