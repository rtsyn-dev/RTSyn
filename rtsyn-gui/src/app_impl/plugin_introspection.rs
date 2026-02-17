use crate::GuiApp;
use rtsyn_core::plugin::plugin_display_name as core_plugin_display_name;
use std::path::PathBuf;

const DEDICATED_PLOTTER_VIEW_KINDS: &[&str] = &["live_plotter"];

impl GuiApp {
    /// ```
    pub(crate) fn ports_for_kind(&self, kind: &str, inputs: bool) -> Vec<String> {
        self.plugin_manager
            .installed_plugins
            .iter()
            .find(|plugin| plugin.manifest.kind == kind)
            .map(|plugin| {
                if inputs {
                    plugin.metadata_inputs.clone()
                } else {
                    plugin.metadata_outputs.clone()
                }
            })
            .unwrap_or_default()
    }

    pub(crate) fn ports_for_plugin(&self, plugin_id: u64, inputs: bool) -> Vec<String> {
        let Some(plugin) = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .find(|p| p.id == plugin_id)
        else {
            return Vec::new();
        };
        let extendable_inputs = self.is_extendable_inputs(&plugin.kind);
        if extendable_inputs && inputs {
            let columns_len = plugin
                .config
                .get("columns")
                .and_then(|v| v.as_array())
                .map(|arr| arr.len())
                .unwrap_or(0);
            let input_count = if columns_len > 0 {
                columns_len
            } else {
                plugin
                    .config
                    .get("input_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize
            };
            let mut ports = Vec::new();
            ports.push("in".to_string());
            ports.extend((0..input_count).map(|idx| format!("in_{idx}")));
            return ports;
        }
        self.ports_for_kind(&plugin.kind, inputs)
    }

    pub(crate) fn is_extendable_inputs(&self, kind: &str) -> bool {
        if let Some(cached) = self.behavior_manager.cached_behaviors.get(kind) {
            return matches!(
                cached.extendable_inputs,
                rtsyn_plugin::ui::ExtendableInputs::Auto { .. }
                    | rtsyn_plugin::ui::ExtendableInputs::Manual
            );
        }
        rtsyn_core::plugin::is_extendable_inputs(kind)
    }

    pub(crate) fn plugin_uses_external_window(&self, kind: &str) -> bool {
        self.behavior_manager
            .cached_behaviors
            .get(kind)
            .map(|b| b.external_window)
            .unwrap_or(false)
    }

    pub(crate) fn plugin_uses_plotter_viewport(&self, kind: &str) -> bool {
        self.plugin_uses_external_window(kind) && DEDICATED_PLOTTER_VIEW_KINDS.contains(&kind)
    }

    pub(crate) fn plugin_uses_external_config_viewport(&self, kind: &str) -> bool {
        self.plugin_uses_external_window(kind) && !self.plugin_uses_plotter_viewport(kind)
    }

    pub(crate) fn auto_extend_inputs(&self, kind: &str) -> Vec<String> {
        if let Some(cached) = self.behavior_manager.cached_behaviors.get(kind) {
            if matches!(
                cached.extendable_inputs,
                rtsyn_plugin::ui::ExtendableInputs::Auto { .. }
            ) {
                return (1..=10).map(|i| format!("in_{}", i)).collect();
            }
        }
        if matches!(kind, "csv_recorder" | "live_plotter") {
            (1..=10).map(|i| format!("in_{}", i)).collect()
        } else {
            Vec::new()
        }
    }

    pub(crate) fn ensure_plugin_behavior_cached(&mut self, kind: &str) {
        self.behavior_manager.ensure_behavior_cached(
            kind,
            None,
            &self.state_sync.logic_tx,
            &self.plugin_manager,
        );
    }

    pub(crate) fn ensure_plugin_behavior_cached_with_path(
        &mut self,
        kind: &str,
        library_path: Option<&PathBuf>,
    ) {
        let path_str = library_path.map(|p| p.to_string_lossy().to_string());
        self.behavior_manager.ensure_behavior_cached(
            kind,
            path_str.as_deref(),
            &self.state_sync.logic_tx,
            &self.plugin_manager,
        );
    }

    pub(crate) fn plugin_display_name(&self, plugin_id: u64) -> String {
        core_plugin_display_name(
            &self.plugin_manager.installed_plugins,
            &self.workspace_manager.workspace,
            plugin_id,
        )
    }
}
