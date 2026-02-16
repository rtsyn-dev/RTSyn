use crate::GuiApp;
use crate::plotter::LivePlotter;
use rtsyn_core::plotter_view::{live_plotter_config, live_plotter_series_names};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use workspace::{input_sum, input_sum_any};


impl GuiApp {
    /// - Updates UI refresh rate based on actual plotter requirements
    pub(crate) fn update_plotters(
            &mut self,
            tick: u64,
            outputs: &HashMap<(u64, String), f64>,
            samples: &HashMap<u64, Vec<(u64, Vec<f64>)>>,
        ) {
            let mut max_refresh = 1.0;
            let mut live_plotter_ids: HashSet<u64> = HashSet::new();

            for plugin in &self.workspace_manager.workspace.plugins {
                if plugin.kind != "live_plotter" {
                    continue;
                }
                live_plotter_ids.insert(plugin.id);
                let fallback_sample = samples
                    .get(&plugin.id)
                    .and_then(|rows| rows.last())
                    .map(|(_, values)| values.as_slice());
                let (input_count, refresh_hz, config_window_ms) =
                    live_plotter_config(&plugin.config, fallback_sample);
                let preview_window_ms = self
                    .plotter_manager
                    .plotter_preview_settings
                    .get(&plugin.id)
                    .map(|settings| settings.11);
                let effective_window_ms = preview_window_ms.unwrap_or(config_window_ms).max(1.0);
                let series_names = live_plotter_series_names(
                    &self.workspace_manager.workspace,
                    &self.plugin_manager.installed_plugins,
                    plugin.id,
                    input_count,
                );
                let is_open = self
                    .plotter_manager
                    .plotters
                    .get(&plugin.id)
                    .and_then(|plotter| plotter.lock().ok().map(|plotter| plotter.open))
                    .unwrap_or(false);
                let values = if is_open {
                    self.plotter_input_values(plugin.id, input_count, outputs)
                } else {
                    Vec::new()
                };
                let plotter = self
                    .plotter_manager
                    .plotters
                    .entry(plugin.id)
                    .or_insert_with(|| Arc::new(Mutex::new(LivePlotter::new(plugin.id))));
                if let Ok(mut plotter) = plotter.lock() {
                    // Window size must be set before update_config, because decimation
                    // parameters are derived from current window_ms.
                    plotter.set_window_ms(effective_window_ms);
                    plotter.update_config(
                        input_count,
                        refresh_hz,
                        self.state_sync.logic_period_seconds,
                    );
                    plotter.set_series_names(series_names);
                    if plugin.running {
                        if let Some(samples) = samples.get(&plugin.id) {
                            let sample_budget = if plotter.open { 8192 } else { 1024 };
                            let mut selected_indices: Vec<usize> = if samples.len() <= sample_budget {
                                (0..samples.len()).collect()
                            } else {
                                // Preserve first-channel extrema per chunk to avoid cutting spikes.
                                let chunk = (samples.len() + sample_budget - 1) / sample_budget;
                                let mut idxs = Vec::with_capacity(sample_budget * 2);
                                let mut start = 0usize;
                                while start < samples.len() {
                                    let end = (start + chunk).min(samples.len());
                                    idxs.push(start);
                                    if end - start > 2 {
                                        let mut min_i = start;
                                        let mut max_i = start;
                                        let mut min_v =
                                            samples[start].1.first().copied().unwrap_or(0.0);
                                        let mut max_v = min_v;
                                        for (i, (_, values)) in
                                            samples.iter().enumerate().take(end).skip(start + 1)
                                        {
                                            let v = values.first().copied().unwrap_or(0.0);
                                            if v < min_v {
                                                min_v = v;
                                                min_i = i;
                                            }
                                            if v > max_v {
                                                max_v = v;
                                                max_i = i;
                                            }
                                        }
                                        idxs.push(min_i);
                                        idxs.push(max_i);
                                    }
                                    idxs.push(end - 1);
                                    start = end;
                                }
                                idxs.sort_unstable();
                                idxs.dedup();
                                idxs
                            };
                            if selected_indices.len() > sample_budget * 2 {
                                let step = (selected_indices.len() / (sample_budget * 2)).max(1);
                                selected_indices = selected_indices.into_iter().step_by(step).collect();
                            }
                            for idx in selected_indices {
                                let (sample_tick, values) = &samples[idx];
                                plotter.push_sample_from_tick(
                                    *sample_tick,
                                    self.state_sync.logic_period_seconds,
                                    self.state_sync.logic_time_scale,
                                    values,
                                );
                            }
                        } else if plotter.open {
                            plotter.push_sample_from_tick(
                                tick,
                                self.state_sync.logic_period_seconds,
                                self.state_sync.logic_time_scale,
                                &values,
                            );
                        }
                        if plotter.open && refresh_hz > max_refresh {
                            max_refresh = refresh_hz;
                        }
                    }
                }
            }

            self.plotter_manager
                .plotters
                .retain(|plugin_id, _| live_plotter_ids.contains(plugin_id));
            self.refresh_logic_ui_hz(max_refresh);
        }

    pub(crate) fn plotter_input_values(
            &self,
            plotter_id: u64,
            input_count: usize,
            outputs: &HashMap<(u64, String), f64>,
        ) -> Vec<f64> {
            let mut values = Vec::with_capacity(input_count);
            for idx in 0..input_count {
                let port = format!("in_{idx}");
                let value = if idx == 0 {
                    let ports = vec![port.clone(), "in".to_string()];
                    input_sum_any(
                        &self.workspace_manager.workspace.connections,
                        outputs,
                        plotter_id,
                        &ports,
                    )
                } else {
                    input_sum(
                        &self.workspace_manager.workspace.connections,
                        outputs,
                        plotter_id,
                        &port,
                    )
                };
                values.push(value);
            }
            values
        }

    pub(crate) fn open_running_plotters(&mut self) {
            let mut recompute = false;
            for plugin in &self.workspace_manager.workspace.plugins {
                if !self.plugin_uses_plotter_viewport(&plugin.kind) {
                    continue;
                }
                let should_open = plugin.running || self.plugin_uses_external_window(&plugin.kind);
                if !should_open {
                    continue;
                }
                let plotter = self
                    .plotter_manager
                    .plotters
                    .entry(plugin.id)
                    .or_insert_with(|| Arc::new(Mutex::new(LivePlotter::new(plugin.id))));
                if let Ok(mut plotter) = plotter.lock() {
                    if !plotter.open {
                        plotter.open = true;
                        recompute = true;
                    }
                }
            }
            if recompute {
                self.recompute_plotter_ui_hz();
            }
        }

}