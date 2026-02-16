use crate::GuiApp;
use rtsyn_runtime::{LogicMessage, LogicSettings, LogicState};
use std::collections::HashMap;
use std::time::{Duration, Instant};


impl GuiApp {
    /// runtime operations.
    pub(crate) fn poll_logic_state(&mut self) {
            let mut latest: Option<LogicState> = None;
            let mut merged_samples: HashMap<u64, Vec<(u64, Vec<f64>)>> = HashMap::new();
            while let Ok(state) = self.state_sync.logic_state_rx.try_recv() {
                for (plugin_id, samples) in &state.plotter_samples {
                    let entry = merged_samples.entry(*plugin_id).or_default();
                    entry.extend(samples.iter().cloned());
                }
                latest = Some(state);
            }
            if let Some(state) = latest {
                let outputs = state.outputs;
                let input_values = state.input_values;
                let internal_variable_values = state.internal_variable_values;
                let viewer_values = state.viewer_values;
                let tick = state.tick;
                self.update_plotters(tick, &outputs, &merged_samples);
                let output_interval = if self.output_refresh_hz > 0.0 {
                    Duration::from_secs_f64(1.0 / self.output_refresh_hz)
                } else {
                    Duration::from_secs(1)
                };
                if self.state_sync.last_output_update.elapsed() >= output_interval {
                    // Filter out outputs from stopped plugins
                    let running_plugins: std::collections::HashSet<u64> = self
                        .workspace_manager
                        .workspace
                        .plugins
                        .iter()
                        .filter(|p| p.running)
                        .map(|p| p.id)
                        .collect();

                    let filtered_outputs: HashMap<(u64, String), f64> = outputs
                        .into_iter()
                        .filter(|((id, _), _)| running_plugins.contains(id))
                        .collect();
                    let filtered_inputs: HashMap<(u64, String), f64> = input_values
                        .into_iter()
                        .filter(|((id, _), _)| running_plugins.contains(id))
                        .collect();
                    let filtered_internals: HashMap<(u64, String), serde_json::Value> =
                        internal_variable_values
                            .into_iter()
                            .filter(|((id, _), _)| running_plugins.contains(id))
                            .collect();

                    self.state_sync.computed_outputs = filtered_outputs;
                    self.state_sync.input_values = filtered_inputs;
                    self.state_sync.internal_variable_values = filtered_internals;
                    self.state_sync.viewer_values = viewer_values;
                    self.state_sync.last_output_update = Instant::now();
                }
            }
        }

    /// while the actual restart happens in the background runtime.
    pub(crate) fn restart_plugin(&mut self, plugin_id: u64) {
            let _ = self
                .state_sync
                .logic_tx
                .send(LogicMessage::RestartPlugin(plugin_id));
        }

    /// - UI refresh rate balances responsiveness with performance
    pub(crate) fn send_logic_settings(&mut self) {
            let period_seconds = self.compute_period_seconds();
            let (_unit, time_scale, time_label) = Self::time_settings_from_selection(
                self.workspace_settings.tab,
                self.frequency_unit,
                self.period_unit,
            );
            let cores: Vec<usize> = self
                .selected_cores
                .iter()
                .enumerate()
                .filter_map(|(idx, enabled)| if *enabled { Some(idx) } else { None })
                .collect();
            self.state_sync.logic_period_seconds = period_seconds;
            self.state_sync.logic_time_scale = time_scale;
            self.state_sync.logic_time_label = time_label.clone();
            let _ = self
                .state_sync
                .logic_tx
                .send(LogicMessage::UpdateSettings(LogicSettings {
                    cores,
                    period_seconds,
                    time_scale,
                    time_label,
                    ui_hz: self.state_sync.logic_ui_hz,
                    max_integration_steps: 10, // Default reasonable limit for real-time performance
                }));
        }

    pub(crate) fn refresh_logic_ui_hz(&mut self, max_refresh: f64) {
            let target_hz = if max_refresh > 0.0 { max_refresh } else { 1.0 };
            if (self.state_sync.logic_ui_hz - target_hz).abs() > f64::EPSILON {
                self.state_sync.logic_ui_hz = target_hz;
                self.send_logic_settings();
            }
        }

    pub(crate) fn recompute_plotter_ui_hz(&mut self) {
            let mut max_refresh = 1.0;
            for plotter in self.plotter_manager.plotters.values() {
                if let Ok(plotter) = plotter.lock() {
                    if plotter.open && plotter.refresh_hz > max_refresh {
                        max_refresh = plotter.refresh_hz;
                    }
                }
            }
            self.refresh_logic_ui_hz(max_refresh);
        }

}