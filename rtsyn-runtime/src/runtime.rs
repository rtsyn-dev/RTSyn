use csv_recorder_plugin::{normalize_path, CsvRecorderedPlugin};
use live_plotter_plugin::LivePlotterPlugin;
use performance_monitor_plugin::PerformanceMonitorPlugin;
use rtsyn_plugin::{Plugin, PluginContext};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};
use workspace::{order_plugins_for_execution, WorkspaceDefinition};

use crate::connection_cache::{build_connection_cache, input_sum_cached, sanitize_signal, RuntimeConnectionCache};
use crate::message_handler::{LogicMessage, LogicSettings, LogicState};
use crate::plugin_manager::{
    runtime_plugin_loads_started, set_dynamic_config_patch, set_dynamic_config_if_needed, DynamicPluginInstance, RuntimePlugin,
};
use crate::plugin_processors::{
    process_csv_recorder, process_dynamic_plugin, process_live_plotter, process_performance_monitor,
};
#[cfg(feature = "comedi")]
use crate::plugin_processors::process_comedi_daq;
use crate::rt_thread::{ActiveRtBackend, RuntimeThread};



pub fn spawn_runtime() -> Result<(Sender<LogicMessage>, Receiver<LogicState>), String> {
    let (logic_tx, logic_rx) = mpsc::channel::<LogicMessage>();
    let (logic_state_tx, logic_state_rx) = mpsc::channel::<LogicState>();

    RuntimeThread::spawn(move || {
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
        let mut internal_variable_values: HashMap<(u64, String), serde_json::Value> =
            HashMap::new();
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
                            period_duration =
                                Duration::from_secs_f64(settings.period_seconds.max(0.0));
                            sleep_deadline = ActiveRtBackend::init_sleep(period_duration);
                            plugin_ctx.period_seconds = settings.period_seconds;
                        }
                        LogicMessage::UpdateWorkspace(new_workspace) => {
                            let mut new_ids: HashSet<u64> = HashSet::new();
                            for plugin in &new_workspace.plugins {
                                new_ids.insert(plugin.id);
                                if !plugin_instances.contains_key(&plugin.id) {
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
                                    plugin_instances.insert(plugin.id, instance);
                                }
                                if !plugin_running.contains_key(&plugin.id) {
                                    if let Some(instance) = plugin_instances.get(&plugin.id) {
                                        plugin_running.insert(
                                            plugin.id,
                                            runtime_plugin_loads_started(instance),
                                        );
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
                                                    if let Ok(behavior) =
                                                        serde_json::from_str(&json)
                                                    {
                                                        unsafe {
                                                            ((*dynamic.api).destroy)(
                                                                dynamic.handle,
                                                            );
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
                                let meta_str =
                                    unsafe { ((*dynamic.api).meta_json)(dynamic.handle) };
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
                                let ui_schema: Option<rtsyn_plugin::ui::UISchema> =
                                    if let Some(ui_schema_fn) =
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
                                plugin_instances.get(&plugin_id).and_then(
                                    |instance| match instance {
                                        RuntimePlugin::CsvRecorder(p) => p.get_variable(&var_name),
                                        RuntimePlugin::LivePlotter(p) => p.get_variable(&var_name),
                                        RuntimePlugin::PerformanceMonitor(p) => {
                                            p.get_variable(&var_name)
                                        }
                                        #[cfg(feature = "comedi")]
                                        RuntimePlugin::ComediDaq(p) => p.get_variable(&var_name),
                                        RuntimePlugin::Dynamic(_) => None,
                                    },
                                );
                            let _ = response_tx.send(value);
                        }
                        LogicMessage::SetPluginVariable(plugin_id, var_name, value) => {
                            if let Some(instance) = plugin_instances.get_mut(&plugin_id) {
                                let _ = match instance {
                                    RuntimePlugin::CsvRecorder(p) => {
                                        p.set_variable(&var_name, value)
                                    }
                                    RuntimePlugin::LivePlotter(p) => {
                                        p.set_variable(&var_name, value)
                                    }
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
                return;
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
                            let api = unsafe { &*plugin_instance.api };
                            set_dynamic_config_if_needed(
                                plugin_instance,
                                &plugin.config,
                                settings.period_seconds,
                                settings.max_integration_steps,
                            );
                            let connected_ports =
                                connection_cache.incoming_ports_by_plugin.get(&plugin.id);
                            for (idx, input_name) in plugin_instance.inputs.iter().enumerate() {
                                let is_connected = connected_ports
                                    .map(|ports| ports.contains(input_name))
                                    .unwrap_or(false);
                                let value = if is_connected {
                                    input_sum_cached(
                                        &connection_cache,
                                        &outputs,
                                        plugin.id,
                                        input_name,
                                    )
                                } else {
                                    0.0
                                };
                                input_values.insert((plugin.id, input_name.clone()), value);
                                let bits = value.to_bits();
                                if plugin_instance.last_inputs[idx] != bits {
                                    plugin_instance.last_inputs[idx] = bits;
                                    if let (Some(indices), Some(set_fn)) =
                                        (&plugin_instance.input_indices, api.set_input_by_index)
                                    {
                                        let idx_value = indices.get(idx).copied().unwrap_or(-1);
                                        if idx_value >= 0 {
                                            set_fn(
                                                plugin_instance.handle,
                                                idx_value as usize,
                                                value,
                                            );
                                            continue;
                                        }
                                    }
                                    let bytes = &plugin_instance.input_bytes[idx];
                                    (api.set_input)(
                                        plugin_instance.handle,
                                        bytes.as_ptr(),
                                        bytes.len(),
                                        value,
                                    );
                                }
                            }
                            if is_running {
                                (api.process)(
                                    plugin_instance.handle,
                                    plugin_ctx.tick,
                                    plugin_ctx.period_seconds,
                                );
                                for (idx, output_name) in plugin_instance.outputs.iter().enumerate()
                                {
                                    let value = if let (Some(indices), Some(get_fn)) =
                                        (&plugin_instance.output_indices, api.get_output_by_index)
                                    {
                                        let idx_value = indices.get(idx).copied().unwrap_or(-1);
                                        if idx_value >= 0 {
                                            get_fn(plugin_instance.handle, idx_value as usize)
                                        } else {
                                            let bytes = &plugin_instance.output_bytes[idx];
                                            (api.get_output)(
                                                plugin_instance.handle,
                                                bytes.as_ptr(),
                                                bytes.len(),
                                            )
                                        }
                                    } else {
                                        let bytes = &plugin_instance.output_bytes[idx];
                                        (api.get_output)(
                                            plugin_instance.handle,
                                            bytes.as_ptr(),
                                            bytes.len(),
                                        )
                                    };
                                    let value = sanitize_signal(value);
                                    outputs.insert((plugin.id, output_name.clone()), value);
                                }
                            } else {
                                for output_name in &plugin_instance.outputs {
                                    outputs.insert((plugin.id, output_name.clone()), 0.0);
                                }
                            }
                            for (idx, var_name) in
                                plugin_instance.internal_variables.iter().enumerate()
                            {
                                let bytes = &plugin_instance.internal_variable_bytes[idx];
                                let value = (api.get_output)(
                                    plugin_instance.handle,
                                    bytes.as_ptr(),
                                    bytes.len(),
                                );
                                let value = sanitize_signal(value);
                                internal_variable_values.insert(
                                    (plugin.id, var_name.clone()),
                                    serde_json::Value::from(value),
                                );
                            }
                        }
                        RuntimePlugin::CsvRecorder(plugin_instance) => {
                            let config_input_count = plugin
                                .config
                                .get("input_count")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0)
                                as usize;
                            let separator = plugin
                                .config
                                .get("separator")
                                .and_then(|v| v.as_str())
                                .unwrap_or(",");
                            let path = plugin
                                .config
                                .get("path")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let include_time = plugin
                                .config
                                .get("include_time")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(true);
                            let mut columns: Vec<String> = plugin
                                .config
                                .get("columns")
                                .and_then(|v| v.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .map(|value| value.as_str().unwrap_or("").to_string())
                                        .collect()
                                })
                                .unwrap_or_default();
                            let mut input_count = columns.len();
                            if input_count < config_input_count {
                                columns.resize(config_input_count, String::new());
                                input_count = config_input_count;
                            }
                            for idx in 0..input_count {
                                if columns.get(idx).map(|v| v.is_empty()).unwrap_or(true) {
                                    columns[idx] = "empty".to_string();
                                }
                            }
                            let mut inputs = Vec::with_capacity(input_count);
                            for idx in 0..input_count {
                                let port = format!("in_{idx}");
                                let value = if idx == 0 {
                                    input_sum_cached(&connection_cache, &outputs, plugin.id, &port)
                                        + input_sum_cached(
                                            &connection_cache,
                                            &outputs,
                                            plugin.id,
                                            "in",
                                        )
                                } else {
                                    input_sum_cached(&connection_cache, &outputs, plugin.id, &port)
                                };
                                input_values.insert((plugin.id, port), value);
                                inputs.push(value);
                            }
                            plugin_instance.set_config(
                                input_count,
                                separator.to_string(),
                                columns,
                                normalize_path(path),
                                is_running,
                                include_time,
                                settings.time_scale,
                                settings.time_label.clone(),
                                settings.period_seconds,
                            );
                            internal_variable_values.insert(
                                (plugin.id, "input_count".to_string()),
                                serde_json::Value::from(input_count as i64),
                            );
                            internal_variable_values.insert(
                                (plugin.id, "running".to_string()),
                                serde_json::Value::from(is_running),
                            );
                            plugin_instance.set_inputs(inputs);
                            let _ = plugin_instance.process(&mut plugin_ctx);
                        }
                        #[cfg(feature = "comedi")]
                        RuntimePlugin::ComediDaq(plugin_instance) => {
                            let device_path = plugin
                                .config
                                .get("device_path")
                                .and_then(|v| v.as_str())
                                .unwrap_or("/dev/comedi0");
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

                            let active_inputs = connection_cache
                                .incoming_ports_by_plugin
                                .get(&plugin.id)
                                .cloned()
                                .unwrap_or_default();
                            let active_outputs = connection_cache
                                .outgoing_ports_by_plugin
                                .get(&plugin.id)
                                .cloned()
                                .unwrap_or_default();

                            plugin_instance.set_active_ports(&active_inputs, &active_outputs);
                            plugin_instance.set_config(
                                device_path.to_string(),
                                scan_devices,
                                scan_nonce,
                            );

                            let has_active =
                                !active_inputs.is_empty() || !active_outputs.is_empty();
                            if has_active && !plugin_instance.is_open() {
                                let _ = plugin_instance.open();
                            } else if !has_active && plugin_instance.is_open() {
                                let _ = plugin_instance.close();
                            }

                            let input_port_len = plugin_instance.input_port_names().len();
                            for idx in 0..input_port_len {
                                let port = plugin_instance.input_port_names()[idx].clone();
                                let value =
                                    input_sum_cached(&connection_cache, &outputs, plugin.id, &port);
                                input_values.insert((plugin.id, port.clone()), value);
                                plugin_instance.set_input(&port, value);
                            }

                            let _ = plugin_instance.process(&mut plugin_ctx);

                            let output_port_len = plugin_instance.output_port_names().len();
                            for idx in 0..output_port_len {
                                let port = plugin_instance.output_port_names()[idx].clone();
                                let value = plugin_instance.get_output(&port);
                                outputs.insert((plugin.id, port), value);
                            }
                        }
                        RuntimePlugin::LivePlotter(plugin_instance) => {
                            let config_input_count = plugin
                                .config
                                .get("input_count")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0)
                                as usize;
                            let input_count = config_input_count;
                            plugin_instance.set_config(input_count, is_running);
                            internal_variable_values.insert(
                                (plugin.id, "input_count".to_string()),
                                serde_json::Value::from(input_count as i64),
                            );
                            internal_variable_values.insert(
                                (plugin.id, "running".to_string()),
                                serde_json::Value::from(is_running),
                            );
                            let mut inputs = Vec::with_capacity(input_count);
                            for idx in 0..input_count {
                                let port = format!("in_{idx}");
                                let value = if idx == 0 {
                                    input_sum_cached(&connection_cache, &outputs, plugin.id, &port)
                                        + input_sum_cached(
                                            &connection_cache,
                                            &outputs,
                                            plugin.id,
                                            "in",
                                        )
                                } else {
                                    input_sum_cached(&connection_cache, &outputs, plugin.id, &port)
                                };
                                input_values.insert((plugin.id, port), value);
                                inputs.push(value);
                            }
                            plugin_instance.set_inputs(inputs.clone());
                            if plugin_instance.is_running() {
                                plotter_samples
                                    .entry(plugin.id)
                                    .or_default()
                                    .push((plugin_ctx.tick, inputs));
                            }
                        }
                        RuntimePlugin::PerformanceMonitor(plugin_instance) => {
                            let period_unit = plugin
                                .config
                                .get("units")
                                .and_then(|v| v.as_str())
                                .or_else(|| {
                                    plugin.config.get("period_unit").and_then(|v| v.as_str())
                                })
                                .unwrap_or("us");

                            let latency_us_from_unit = |value: f64, unit: &str| -> f64 {
                                match unit {
                                    "ns" => value / 1_000.0,
                                    "us" => value,
                                    "ms" => value * 1_000.0,
                                    "s" => value * 1_000_000.0,
                                    _ => value,
                                }
                            };
                            let max_latency_us = plugin
                                .config
                                .get("latency")
                                .and_then(|v| v.as_f64())
                                .map(|v| latency_us_from_unit(v, period_unit))
                                .or_else(|| {
                                    plugin.config.get("max_latency_us").and_then(|v| v.as_f64())
                                })
                                .unwrap_or(1000.0);
                            let workspace_period_us = settings.period_seconds * 1_000_000.0;
                            plugin_instance.set_config(
                                max_latency_us,
                                workspace_period_us,
                                period_unit,
                            );
                            let _ = plugin_instance.process(&mut plugin_ctx);

                            for (idx, output_name) in plugin_instance
                                .outputs()
                                .iter()
                                .map(|port| port.id.0.as_str())
                                .enumerate()
                            {
                                let value = plugin_instance.get_output_values()[idx];
                                outputs.insert((plugin.id, output_name.to_string()), value);
                            }
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
                            limited_samples
                                .drain(0..limited_samples.len() - max_samples_per_plugin);
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
    })?;

    Ok((logic_tx, logic_state_rx))
}

pub fn run_runtime_current(
    logic_rx: Receiver<LogicMessage>,
    logic_state_tx: Sender<LogicState>,
) -> Result<(), String> {
    ActiveRtBackend::prepare()?;
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
                            if !plugin_instances.contains_key(&plugin.id) {
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
                                plugin_instances.insert(plugin.id, instance);
                            }
                            if !plugin_running.contains_key(&plugin.id) {
                                if let Some(instance) = plugin_instances.get(&plugin.id) {
                                    plugin_running
                                        .insert(plugin.id, runtime_plugin_loads_started(instance));
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
