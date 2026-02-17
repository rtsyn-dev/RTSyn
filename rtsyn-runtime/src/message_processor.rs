use std::collections::{HashMap, HashSet};
use std::time::Duration;
use workspace::WorkspaceDefinition;

use crate::connection_cache::{build_connection_cache, RuntimeConnectionCache};
use crate::message_handler::{LogicMessage, LogicSettings};
use crate::plugin_manager::{runtime_plugin_loads_started, DynamicPluginInstance, RuntimePlugin};
#[cfg(feature = "comedi")]
use comedi_daq_plugin::ComediDaqPlugin;
use csv_recorder_plugin::CsvRecorderedPlugin;
use live_plotter_plugin::LivePlotterPlugin;
use performance_monitor_plugin::PerformanceMonitorPlugin;

pub enum MessageAction {
    UpdateSettings(LogicSettings, Duration),
    UpdateWorkspace(WorkspaceDefinition, RuntimeConnectionCache),
    SetPluginRunning(u64, bool),
    RestartPlugin(u64),
    SetPluginVariable(u64, String, serde_json::Value),
}

pub fn process_message(
    message: LogicMessage,
    workspace: &Option<WorkspaceDefinition>,
    plugin_instances: &mut HashMap<u64, RuntimePlugin>,
    plugin_running: &mut HashMap<u64, bool>,
) -> Option<MessageAction> {
    match message {
        LogicMessage::UpdateSettings(new_settings) => {
            let period_duration = Duration::from_secs_f64(new_settings.period_seconds.max(0.0));
            Some(MessageAction::UpdateSettings(new_settings, period_duration))
        }
        LogicMessage::UpdateWorkspace(new_workspace) => {
            let mut new_ids: HashSet<u64> = HashSet::new();
            for plugin in &new_workspace.plugins {
                new_ids.insert(plugin.id);
                if let std::collections::hash_map::Entry::Vacant(e) =
                    plugin_instances.entry(plugin.id)
                {
                    let instance = match plugin.kind.as_str() {
                        "csv_recorder" => {
                            RuntimePlugin::CsvRecorder(CsvRecorderedPlugin::new(plugin.id))
                        }
                        "live_plotter" => {
                            RuntimePlugin::LivePlotter(LivePlotterPlugin::new(plugin.id))
                        }
                        "performance_monitor" => RuntimePlugin::PerformanceMonitor(
                            PerformanceMonitorPlugin::new(plugin.id),
                        ),
                        #[cfg(feature = "comedi")]
                        "comedi_daq" => RuntimePlugin::ComediDaq(ComediDaqPlugin::new(plugin.id)),
                        _ => {
                            if let Some(path) =
                                plugin.config.get("library_path").and_then(|v| v.as_str())
                            {
                                unsafe {
                                    if let Some(dynamic) =
                                        DynamicPluginInstance::load(path, plugin.id)
                                    {
                                        RuntimePlugin::Dynamic(dynamic)
                                    } else {
                                        continue;
                                    }
                                }
                            } else {
                                continue;
                            }
                        }
                    };
                    e.insert(instance);
                }
                if let std::collections::hash_map::Entry::Vacant(e) =
                    plugin_running.entry(plugin.id)
                {
                    if let Some(instance) = plugin_instances.get(&plugin.id) {
                        e.insert(runtime_plugin_loads_started(instance));
                    }
                }
            }

            let removed_ids: Vec<u64> = plugin_instances
                .keys()
                .filter(|id| !new_ids.contains(id))
                .copied()
                .collect();
            for id in removed_ids {
                if let Some(instance) = plugin_instances.remove(&id) {
                    if let RuntimePlugin::Dynamic(dynamic) = instance {
                        (unsafe { &*dynamic.api }.destroy)(dynamic.handle);
                    }
                }
                plugin_running.remove(&id);
            }

            let connection_cache = build_connection_cache(&new_workspace);
            Some(MessageAction::UpdateWorkspace(
                new_workspace,
                connection_cache,
            ))
        }
        LogicMessage::SetPluginRunning(plugin_id, running) => {
            Some(MessageAction::SetPluginRunning(plugin_id, running))
        }
        LogicMessage::RestartPlugin(plugin_id) => {
            if let Some(ws) = workspace.as_ref() {
                if let Some(plugin) = ws.plugins.iter().find(|p| p.id == plugin_id) {
                    let instance = match plugin.kind.as_str() {
                        "csv_recorder" => {
                            RuntimePlugin::CsvRecorder(CsvRecorderedPlugin::new(plugin.id))
                        }
                        "live_plotter" => {
                            RuntimePlugin::LivePlotter(LivePlotterPlugin::new(plugin.id))
                        }
                        "performance_monitor" => RuntimePlugin::PerformanceMonitor(
                            PerformanceMonitorPlugin::new(plugin.id),
                        ),
                        #[cfg(feature = "comedi")]
                        "comedi_daq" => RuntimePlugin::ComediDaq(ComediDaqPlugin::new(plugin.id)),
                        _ => {
                            if let Some(path) =
                                plugin.config.get("library_path").and_then(|v| v.as_str())
                            {
                                unsafe {
                                    if let Some(dynamic) =
                                        DynamicPluginInstance::load(path, plugin.id)
                                    {
                                        RuntimePlugin::Dynamic(dynamic)
                                    } else {
                                        return None;
                                    }
                                }
                            } else {
                                return None;
                            }
                        }
                    };
                    plugin_instances.insert(plugin.id, instance);
                    Some(MessageAction::RestartPlugin(plugin_id))
                } else {
                    None
                }
            } else {
                None
            }
        }
        LogicMessage::SetPluginVariable(plugin_id, var_name, value) => {
            Some(MessageAction::SetPluginVariable(plugin_id, var_name, value))
        }
        LogicMessage::QueryPluginBehavior(_, _, response_tx) => {
            let _ = response_tx.send(None);
            None
        }
        LogicMessage::QueryPluginMetadata(_, response_tx) => {
            let _ = response_tx.send(None);
            None
        }
        LogicMessage::GetPluginVariable(_, _, response_tx) => {
            let _ = response_tx.send(None);
            None
        }
    }
}
