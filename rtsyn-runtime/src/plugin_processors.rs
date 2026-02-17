use csv_recorder_plugin::{normalize_path, CsvRecorderedPlugin};
use live_plotter_plugin::LivePlotterPlugin;
use performance_monitor_plugin::PerformanceMonitorPlugin;
use rtsyn_plugin::{DeviceDriver, Plugin, PluginContext};
use std::collections::HashMap;
use workspace::PluginDefinition;

use crate::connection_cache::{input_sum_cached, sanitize_signal, RuntimeConnectionCache};
use crate::message_handler::LogicSettings;
use crate::plugin_manager::{set_dynamic_config_if_needed, DynamicPluginInstance};

pub fn process_csv_recorder(
    plugin_instance: &mut CsvRecorderedPlugin,
    plugin: &PluginDefinition,
    connection_cache: &RuntimeConnectionCache,
    outputs: &HashMap<(u64, String), f64>,
    input_values: &mut HashMap<(u64, String), f64>,
    internal_variable_values: &mut HashMap<(u64, String), serde_json::Value>,
    is_running: bool,
    settings: &LogicSettings,
    plugin_ctx: &mut PluginContext,
) {
    let config_input_count = plugin
        .config
        .get("input_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
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
            input_sum_cached(connection_cache, outputs, plugin.id, &port)
                + input_sum_cached(connection_cache, outputs, plugin.id, "in")
        } else {
            input_sum_cached(connection_cache, outputs, plugin.id, &port)
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
    let _ = plugin_instance.process(plugin_ctx);
}

pub fn process_live_plotter(
    plugin_instance: &mut LivePlotterPlugin,
    plugin: &PluginDefinition,
    connection_cache: &RuntimeConnectionCache,
    outputs: &HashMap<(u64, String), f64>,
    input_values: &mut HashMap<(u64, String), f64>,
    internal_variable_values: &mut HashMap<(u64, String), serde_json::Value>,
    is_running: bool,
    plotter_samples: &mut HashMap<u64, Vec<(u64, Vec<f64>)>>,
    plugin_ctx: &PluginContext,
) {
    let config_input_count = plugin
        .config
        .get("input_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
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
            input_sum_cached(connection_cache, outputs, plugin.id, &port)
                + input_sum_cached(connection_cache, outputs, plugin.id, "in")
        } else {
            input_sum_cached(connection_cache, outputs, plugin.id, &port)
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

pub fn process_performance_monitor(
    plugin_instance: &mut PerformanceMonitorPlugin,
    plugin: &PluginDefinition,
    outputs: &mut HashMap<(u64, String), f64>,
    settings: &LogicSettings,
    plugin_ctx: &mut PluginContext,
) {
    let period_unit = plugin
        .config
        .get("units")
        .and_then(|v| v.as_str())
        .or_else(|| plugin.config.get("period_unit").and_then(|v| v.as_str()))
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
        .or_else(|| plugin.config.get("max_latency_us").and_then(|v| v.as_f64()))
        .unwrap_or(1000.0);
    let workspace_period_us = settings.period_seconds * 1_000_000.0;
    plugin_instance.set_config(max_latency_us, workspace_period_us, period_unit);
    let _ = plugin_instance.process(plugin_ctx);

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

pub fn process_dynamic_plugin(
    plugin_instance: &mut DynamicPluginInstance,
    plugin: &PluginDefinition,
    connection_cache: &RuntimeConnectionCache,
    outputs: &mut HashMap<(u64, String), f64>,
    input_values: &mut HashMap<(u64, String), f64>,
    internal_variable_values: &mut HashMap<(u64, String), serde_json::Value>,
    is_running: bool,
    settings: &LogicSettings,
    plugin_ctx: &mut PluginContext,
) {
    let api = unsafe { &*plugin_instance.api };
    set_dynamic_config_if_needed(
        plugin_instance,
        &plugin.config,
        settings.period_seconds,
        settings.max_integration_steps,
    );
    let connected_ports = connection_cache.incoming_ports_by_plugin.get(&plugin.id);
    for (idx, input_name) in plugin_instance.inputs.iter().enumerate() {
        let is_connected = connected_ports
            .map(|ports| ports.contains(input_name))
            .unwrap_or(false);
        let value = if is_connected {
            input_sum_cached(connection_cache, outputs, plugin.id, input_name)
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
                    set_fn(plugin_instance.handle, idx_value as usize, value);
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
        for (idx, output_name) in plugin_instance.outputs.iter().enumerate() {
            let value = if let (Some(indices), Some(get_fn)) =
                (&plugin_instance.output_indices, api.get_output_by_index)
            {
                let idx_value = indices.get(idx).copied().unwrap_or(-1);
                if idx_value >= 0 {
                    get_fn(plugin_instance.handle, idx_value as usize)
                } else {
                    let bytes = &plugin_instance.output_bytes[idx];
                    (api.get_output)(plugin_instance.handle, bytes.as_ptr(), bytes.len())
                }
            } else {
                let bytes = &plugin_instance.output_bytes[idx];
                (api.get_output)(plugin_instance.handle, bytes.as_ptr(), bytes.len())
            };
            let value = sanitize_signal(value);
            outputs.insert((plugin.id, output_name.clone()), value);
        }
    } else {
        for output_name in &plugin_instance.outputs {
            outputs.insert((plugin.id, output_name.clone()), 0.0);
        }
    }
    for (idx, var_name) in plugin_instance.internal_variables.iter().enumerate() {
        let bytes = &plugin_instance.internal_variable_bytes[idx];
        let value = (api.get_output)(plugin_instance.handle, bytes.as_ptr(), bytes.len());
        let value = sanitize_signal(value);
        internal_variable_values.insert(
            (plugin.id, var_name.clone()),
            serde_json::Value::from(value),
        );
    }
}

#[cfg(feature = "comedi")]
pub fn process_comedi_daq(
    plugin_instance: &mut comedi_daq_plugin::ComediDaqPlugin,
    plugin: &PluginDefinition,
    connection_cache: &RuntimeConnectionCache,
    outputs: &mut HashMap<(u64, String), f64>,
    input_values: &mut HashMap<(u64, String), f64>,
    plugin_ctx: &mut PluginContext,
) {
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
    plugin_instance.set_config(device_path.to_string(), scan_devices, scan_nonce);

    let has_active_inputs = !active_inputs.is_empty();
    let has_active_outputs = !active_outputs.is_empty();
    let has_active = has_active_inputs || has_active_outputs;
    if has_active && !plugin_instance.is_open() {
        let _ = plugin_instance.open();
    } else if !has_active && plugin_instance.is_open() {
        let _ = plugin_instance.close();
    }

    let input_port_len = plugin_instance.input_port_names().len();
    for idx in 0..input_port_len {
        let port = plugin_instance.input_port_names()[idx].clone();
        let value = input_sum_cached(connection_cache, outputs, plugin.id, &port);
        input_values.insert((plugin.id, port.clone()), value);
        plugin_instance.set_input(&port, value);
    }

    let _ = plugin_instance.process(plugin_ctx);

    if has_active_outputs && plugin_instance.is_open() {
        let output_port_len = plugin_instance.output_port_names().len();
        for idx in 0..output_port_len {
            let port = plugin_instance.output_port_names()[idx].clone();
            if active_outputs.contains(&port) {
                let value = plugin_instance.get_output(&port);
                outputs.insert((plugin.id, port), value);
            } else {
                outputs.insert((plugin.id, port), 0.0);
            }
        }
    } else {
        for port in plugin_instance.output_port_names() {
            outputs.insert((plugin.id, port.clone()), 0.0);
        }
    }
}
