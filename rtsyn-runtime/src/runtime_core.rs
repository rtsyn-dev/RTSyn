use csv_recorder_plugin::CsvRecorderedPlugin;
use live_plotter_plugin::LivePlotterPlugin;
use performance_monitor_plugin::PerformanceMonitorPlugin;
use rtsyn_plugin::{DeviceDriver, Plugin, PluginContext};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};
use workspace::{order_plugins_for_execution, WorkspaceDefinition};

use crate::connection_cache::{build_connection_cache, RuntimeConnectionCache};
use crate::message_handler::{LogicMessage, LogicSettings, LogicState};
use crate::plugin_manager::{
    runtime_plugin_loads_started, set_dynamic_config_patch, DynamicPluginInstance, RuntimePlugin,
};
#[cfg(feature = "comedi")]
use crate::plugin_processors::process_comedi_daq;
use crate::plugin_processors::{
    process_csv_recorder, process_dynamic_plugin, process_live_plotter, process_performance_monitor,
};
use crate::rt_thread::ActiveRtBackend;

pub fn run_runtime_loop(
    logic_rx: Receiver<LogicMessage>,
    logic_state_tx: Sender<LogicState>,
) -> Result<(), String> {
    let mut settings = LogicSettings {
        cores: vec![0],
        period_seconds: 0.001,
        time_scale: 1000.0,
        time_label: "time_ms".to_string(),
        ui_hz: 60.0,
        max_integration_steps: 10, // Reasonable default for real-time performance
    };
    let mut period_duration = Duration::from_secs_f64(settings.period_seconds.max(0.0));
    let mut sleep_deadline = ActiveRtBackend::init_sleep(period_duration);
    let mut workspace: Option<WorkspaceDefinition> = None;
    let mut plugin_instances: HashMap<u64, RuntimePlugin> = HashMap::new();
    let mut plugin_running: HashMap<u64, bool> = HashMap::new();
    let mut plugin_ctx = PluginContext {
        period_seconds: settings.period_seconds,
        ..Default::default()
    };
    let mut outputs: HashMap<(u64, String), f64> = HashMap::new();
    let mut input_values: HashMap<(u64, String), f64> = HashMap::new();
    let mut internal_variable_values: HashMap<(u64, String), serde_json::Value> = HashMap::new();
    let mut viewer_values: HashMap<u64, f64> = HashMap::new();
    let mut plotter_samples: HashMap<u64, Vec<(u64, Vec<f64>)>> = HashMap::new();
    let mut connection_cache = RuntimeConnectionCache::default();
    let mut last_state = Instant::now();

    loop {
        let mut disconnected = false;
        loop {
            match logic_rx.try_recv() {
                Ok(message) => match message {
                    LogicMessage::UpdateSettings(new_settings) => {
                        settings = new_settings;
                        period_duration = Duration::from_secs_f64(settings.period_seconds.max(0.0));
                        sleep_deadline = ActiveRtBackend::init_sleep(period_duration);
                        plugin_ctx.period_seconds = settings.period_seconds;
                    }
                    LogicMessage::UpdateWorkspace(new_workspace) => {
                        let mut new_ids: HashSet<u64> = HashSet::new();
                        for plugin in &new_workspace.plugins {
                            new_ids.insert(plugin.id);
                            if let std::collections::hash_map::Entry::Vacant(e) =
                                plugin_instances.entry(plugin.id)
                            {
                                let instance = match plugin.kind.as_str() {
                                    "csv_recorder" => RuntimePlugin::CsvRecorder(
                                        CsvRecorderedPlugin::new(plugin.id),
                                    ),
                                    "live_plotter" => RuntimePlugin::LivePlotter(
                                        LivePlotterPlugin::new(plugin.id),
                                    ),
                                    "performance_monitor" => RuntimePlugin::PerformanceMonitor(
                                        PerformanceMonitorPlugin::new(plugin.id),
                                    ),
                                    #[cfg(feature = "comedi")]
                                    "comedi_daq" => RuntimePlugin::ComediDaq(
                                        comedi_daq_plugin::ComediDaqPlugin::new(plugin.id),
                                    ),
                                    _ => {
                                        let library_path = plugin
                                            .config
                                            .get("library_path")
                                            .and_then(|v| v.as_str());
                                        if let Some(path) = library_path {
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
                            viewer_values.remove(&id);
                            outputs.retain(|(pid, _), _| *pid != id);
                            input_values.retain(|(pid, _), _| *pid != id);
                            internal_variable_values.retain(|(pid, _), _| *pid != id);
                            plotter_samples.remove(&id);
                        }
                        connection_cache = build_connection_cache(&new_workspace);
                        workspace = Some(new_workspace);
                    }
                    LogicMessage::SetPluginRunning(plugin_id, running) => {
                        plugin_running.insert(plugin_id, running);
                    }
                    LogicMessage::QueryPluginBehavior(kind, library_path, response_tx) => {
                        let behavior = match kind.as_str() {
                            "csv_recorder" => Some(CsvRecorderedPlugin::new(0).behavior()),
                            "live_plotter" => Some(LivePlotterPlugin::new(0).behavior()),
                            "performance_monitor" => {
                                Some(PerformanceMonitorPlugin::new(0).behavior())
                            }
                            #[cfg(feature = "comedi")]
                            "comedi_daq" => {
                                Some(comedi_daq_plugin::ComediDaqPlugin::new(0).behavior())
                            }

                            _ => {
                                // Try to load behavior from dynamic plugin
                                if let Some(path) = library_path.as_ref() {
                                    if let Some(dynamic) =
                                        unsafe { DynamicPluginInstance::load(path, 0) }
                                    {
                                        if let Some(behavior_json_fn) =
                                            unsafe { (*dynamic.api).behavior_json }
                                        {
                                            let json_str = behavior_json_fn(dynamic.handle);
                                            if !json_str.ptr.is_null() && json_str.len > 0 {
                                                let json = unsafe { json_str.into_string() };
                                                if let Ok(behavior) = serde_json::from_str(&json) {
                                                    unsafe {
                                                        ((*dynamic.api).destroy)(dynamic.handle);
                                                    }
                                                    let _ = response_tx.send(Some(behavior));
                                                    continue;
                                                }
                                            }
                                            unsafe {
                                                ((*dynamic.api).destroy)(dynamic.handle);
                                            }
                                        } else {
                                            unsafe {
                                                ((*dynamic.api).destroy)(dynamic.handle);
                                            }
                                        }
                                    }
                                }
                                None
                            }
                        };
                        let _ = response_tx.send(behavior);
                    }
                    LogicMessage::QueryPluginMetadata(library_path, response_tx) => {
                        let metadata = if let Some(dynamic) =
                            unsafe { DynamicPluginInstance::load(&library_path, 0) }
                        {
                            let inputs_str =
                                unsafe { ((*dynamic.api).inputs_json)(dynamic.handle) };
                            let outputs_str =
                                unsafe { ((*dynamic.api).outputs_json)(dynamic.handle) };
                            let meta_str = unsafe { ((*dynamic.api).meta_json)(dynamic.handle) };
                            let inputs: Vec<String> =
                                if inputs_str.ptr.is_null() || inputs_str.len == 0 {
                                    Vec::new()
                                } else {
                                    let json = unsafe { inputs_str.into_string() };
                                    serde_json::from_str(&json).unwrap_or_default()
                                };
                            let outputs: Vec<String> =
                                if outputs_str.ptr.is_null() || outputs_str.len == 0 {
                                    Vec::new()
                                } else {
                                    let json = unsafe { outputs_str.into_string() };
                                    serde_json::from_str(&json).unwrap_or_default()
                                };
                            let meta: serde_json::Value =
                                if meta_str.ptr.is_null() || meta_str.len == 0 {
                                    serde_json::json!({})
                                } else {
                                    let json = unsafe { meta_str.into_string() };
                                    serde_json::from_str(&json).unwrap_or(serde_json::json!({}))
                                };
                            let variables: Vec<(String, f64)> = meta
                                .get("default_vars")
                                .and_then(|v| v.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|item| {
                                            if let Some(arr) = item.as_array() {
                                                if arr.len() == 2 {
                                                    let name = arr[0].as_str()?.to_string();
                                                    let value = arr[1].as_f64()?;
                                                    return Some((name, value));
                                                }
                                            }
                                            None
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();
                            let display_schema = if let Some(schema_fn) =
                                unsafe { (*dynamic.api).display_schema_json }
                            {
                                let schema_str = schema_fn(dynamic.handle);
                                if schema_str.ptr.is_null() || schema_str.len == 0 {
                                    None
                                } else {
                                    let json = unsafe { schema_str.into_string() };
                                    serde_json::from_str(&json).ok()
                                }
                            } else {
                                None
                            };
                            let ui_schema: Option<rtsyn_plugin::ui::UISchema> = if let Some(
                                ui_schema_fn,
                            ) =
                                unsafe { (*dynamic.api).ui_schema_json }
                            {
                                let schema_str = ui_schema_fn(dynamic.handle);
                                if schema_str.ptr.is_null() || schema_str.len == 0 {
                                    None
                                } else {
                                    let json = unsafe { schema_str.into_string() };
                                    serde_json::from_str(&json).ok()
                                }
                            } else {
                                None
                            };
                            unsafe {
                                ((*dynamic.api).destroy)(dynamic.handle);
                            }
                            Some((inputs, outputs, variables, display_schema, ui_schema))
                        } else {
                            None
                        };
                        let _ = response_tx.send(metadata);
                    }
                    LogicMessage::RestartPlugin(plugin_id) => {
                        let Some(ws) = workspace.as_ref() else {
                            continue;
                        };
                        let Some(plugin) = ws.plugins.iter().find(|p| p.id == plugin_id) else {
                            continue;
                        };
                        if let Some(instance) = plugin_instances.get_mut(&plugin_id) {
                            #[cfg(feature = "comedi")]
                            if let RuntimePlugin::ComediDaq(plugin_instance) = instance {
                                let device_path = plugin
                                    .config
                                    .get("device_path")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("/dev/comedi0")
                                    .to_string();
                                let scan_devices = plugin
                                    .config
                                    .get("scan_devices")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false);
                                let scan_nonce = plugin
                                    .config
                                    .get("scan_nonce")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0);
                                plugin_instance.set_config(device_path, scan_devices, scan_nonce);
                                let _ = plugin_instance.close();
                                let _ = plugin_instance.open();
                                continue;
                            }
                        }
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
                            "comedi_daq" => RuntimePlugin::ComediDaq(
                                comedi_daq_plugin::ComediDaqPlugin::new(plugin.id),
                            ),
                            _ => {
                                let library_path =
                                    plugin.config.get("library_path").and_then(|v| v.as_str());
                                if let Some(path) = library_path {
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
                        plugin_instances.insert(plugin.id, instance);
                        viewer_values.remove(&plugin.id);
                        outputs.retain(|(pid, _), _| *pid != plugin.id);
                        input_values.retain(|(pid, _), _| *pid != plugin.id);
                        internal_variable_values.retain(|(pid, _), _| *pid != plugin.id);
                    }
                    LogicMessage::GetPluginVariable(plugin_id, var_name, response_tx) => {
                        let value =
                            plugin_instances
                                .get(&plugin_id)
                                .and_then(|instance| match instance {
                                    RuntimePlugin::CsvRecorder(p) => p.get_variable(&var_name),
                                    RuntimePlugin::LivePlotter(p) => p.get_variable(&var_name),
                                    RuntimePlugin::PerformanceMonitor(p) => {
                                        p.get_variable(&var_name)
                                    }
                                    #[cfg(feature = "comedi")]
                                    RuntimePlugin::ComediDaq(p) => p.get_variable(&var_name),
                                    RuntimePlugin::Dynamic(_) => None,
                                });
                        let _ = response_tx.send(value);
                    }
                    LogicMessage::SetPluginVariable(plugin_id, var_name, value) => {
                        if let Some(instance) = plugin_instances.get_mut(&plugin_id) {
                            let _ = match instance {
                                RuntimePlugin::CsvRecorder(p) => p.set_variable(&var_name, value),
                                RuntimePlugin::LivePlotter(p) => p.set_variable(&var_name, value),
                                RuntimePlugin::PerformanceMonitor(p) => {
                                    p.set_variable(&var_name, value)
                                }
                                #[cfg(feature = "comedi")]
                                RuntimePlugin::ComediDaq(p) => p.set_variable(&var_name, value),
                                RuntimePlugin::Dynamic(plugin_instance) => {
                                    set_dynamic_config_patch(
                                        plugin_instance,
                                        &var_name,
                                        value.clone(),
                                    );
                                    Ok(())
                                }
                            };
                        }
                    }
                },
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        if disconnected {
            break;
        }

        if let Some(ws) = workspace.as_ref() {
            let plugins = order_plugins_for_execution(&ws.plugins, &ws.connections);

            // Increment tick BEFORE processing plugins so they get the current tick
            plugin_ctx.tick = plugin_ctx.tick.wrapping_add(1);

            for plugin in plugins {
                let is_running = plugin_running.get(&plugin.id).copied().unwrap_or(false);
                let instance = match plugin_instances.get_mut(&plugin.id) {
                    Some(instance) => instance,
                    None => continue,
                };
                match instance {
                    RuntimePlugin::Dynamic(plugin_instance) => {
                        process_dynamic_plugin(
                            plugin_instance,
                            &plugin,
                            &connection_cache,
                            &mut outputs,
                            &mut input_values,
                            &mut internal_variable_values,
                            is_running,
                            &settings,
                            &mut plugin_ctx,
                        );
                    }
                    RuntimePlugin::CsvRecorder(plugin_instance) => {
                        process_csv_recorder(
                            plugin_instance,
                            &plugin,
                            &connection_cache,
                            &outputs,
                            &mut input_values,
                            &mut internal_variable_values,
                            is_running,
                            &settings,
                            &mut plugin_ctx,
                        );
                    }
                    #[cfg(feature = "comedi")]
                    RuntimePlugin::ComediDaq(plugin_instance) => {
                        process_comedi_daq(
                            plugin_instance,
                            &plugin,
                            &connection_cache,
                            &mut outputs,
                            &mut input_values,
                            &mut plugin_ctx,
                            is_running,
                        );
                    }
                    RuntimePlugin::LivePlotter(plugin_instance) => {
                        process_live_plotter(
                            plugin_instance,
                            &plugin,
                            &connection_cache,
                            &outputs,
                            &mut input_values,
                            &mut internal_variable_values,
                            is_running,
                            &mut plotter_samples,
                            &plugin_ctx,
                        );
                    }
                    RuntimePlugin::PerformanceMonitor(plugin_instance) => {
                        process_performance_monitor(
                            plugin_instance,
                            &plugin,
                            &mut outputs,
                            &settings,
                            &mut plugin_ctx,
                        );
                    }
                }
            }
            let ui_interval = if settings.ui_hz > 0.0 {
                Duration::from_secs_f64(1.0 / settings.ui_hz)
            } else {
                Duration::from_secs(1)
            };
            if last_state.elapsed() >= ui_interval {
                // Limit plotter samples to prevent memory issues
                let max_samples_per_plugin = ((ui_interval.as_secs_f64()
                    / settings.period_seconds.max(1e-9))
                .ceil() as usize)
                    .saturating_mul(2)
                    .clamp(128, 20_000);
                let mut limited_plotter_samples = HashMap::new();
                for (plugin_id, samples) in &plotter_samples {
                    let mut limited_samples = samples.clone();
                    if limited_samples.len() > max_samples_per_plugin {
                        limited_samples.drain(0..limited_samples.len() - max_samples_per_plugin);
                    }
                    limited_plotter_samples.insert(*plugin_id, limited_samples);
                }

                let _ = logic_state_tx.send(LogicState {
                    outputs: outputs.clone(),
                    input_values: input_values.clone(),
                    internal_variable_values: internal_variable_values.clone(),
                    viewer_values: viewer_values.clone(),
                    tick: plugin_ctx.tick,
                    plotter_samples: limited_plotter_samples,
                });
                plotter_samples.clear();
                last_state = Instant::now();
            }
        }
        let _ = settings.cores.len();
        ActiveRtBackend::sleep(period_duration, &mut sleep_deadline);
    }

    Ok(())
}
