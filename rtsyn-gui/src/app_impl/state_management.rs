use crate::state::{FrequencyUnit, PeriodUnit, WorkspaceTimingTab};
use crate::GuiApp;
use workspace::WorkspaceSettings;

impl GuiApp {
    /// - Plugin management operations that might affect ID sequences
    pub(crate) fn sync_next_plugin_id(&mut self) {
        let max_id = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .map(|p| p.id)
            .max();
        self.plugin_manager.sync_next_plugin_id(max_id);
    }

    /// - Triggers workspace synchronization in next update cycle
    pub(crate) fn mark_workspace_dirty(&mut self) {
        self.workspace_manager.mark_dirty();
    }

    /// - Handles mismatched core counts between workspace and system
    pub(crate) fn apply_workspace_settings(&mut self) {
        let settings = self.workspace_manager.workspace.settings.clone();
        self.workspace_settings.tab = WorkspaceTimingTab::Frequency;
        self.frequency_value = settings.frequency_value;
        self.frequency_unit = match settings.frequency_unit.as_str() {
            "khz" => FrequencyUnit::KHz,
            "mhz" => FrequencyUnit::MHz,
            _ => FrequencyUnit::Hz,
        };
        self.period_value = settings.period_value;
        self.period_unit = match settings.period_unit.as_str() {
            "ns" => PeriodUnit::Ns,
            "us" => PeriodUnit::Us,
            "s" => PeriodUnit::S,
            _ => PeriodUnit::Ms,
        };
        self.selected_cores = (0..self.available_cores)
            .map(|idx| settings.selected_cores.contains(&idx))
            .collect();
        if !self.selected_cores.iter().any(|v| *v) && self.available_cores > 0 {
            self.selected_cores[0] = true;
        }

        self.send_logic_settings();
    }

    pub(crate) fn apply_loads_started_on_load(&mut self) {
        let plugin_infos: Vec<(u64, String, Option<std::path::PathBuf>)> = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .map(|plugin| {
                let library_path = plugin
                    .config
                    .get("library_path")
                    .and_then(|v| v.as_str())
                    .map(|v| std::path::PathBuf::from(v));
                (plugin.id, plugin.kind.clone(), library_path)
            })
            .collect();

        for (plugin_id, kind, library_path) in &plugin_infos {
            self.ensure_plugin_behavior_cached_with_path(kind, library_path.as_ref());
            let loads_started = self
                .behavior_manager
                .cached_behaviors
                .get(kind)
                .map(|b| b.loads_started)
                .unwrap_or(false);
            if let Some(plugin) = self
                .workspace_manager
                .workspace
                .plugins
                .iter_mut()
                .find(|p| p.id == *plugin_id)
            {
                plugin.running = loads_started;
            }
        }
    }

    pub(crate) fn current_workspace_settings(&self) -> WorkspaceSettings {
        let frequency_unit = match self.frequency_unit {
            FrequencyUnit::Hz => "hz",
            FrequencyUnit::KHz => "khz",
            FrequencyUnit::MHz => "mhz",
        };
        let period_unit = match self.period_unit {
            PeriodUnit::Ns => "ns",
            PeriodUnit::Us => "us",
            PeriodUnit::Ms => "ms",
            PeriodUnit::S => "s",
        };
        let selected_cores: Vec<usize> = self
            .selected_cores
            .iter()
            .enumerate()
            .filter_map(|(idx, enabled)| if *enabled { Some(idx) } else { None })
            .collect();
        WorkspaceSettings {
            frequency_value: self.frequency_value,
            frequency_unit: frequency_unit.to_string(),
            period_value: self.period_value,
            period_unit: period_unit.to_string(),
            selected_cores,
        }
    }
}
