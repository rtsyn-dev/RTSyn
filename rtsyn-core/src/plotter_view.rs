use crate::plugin::{plugin_display_name, InstalledPlugin};
use serde_json::Value;
use workspace::WorkspaceDefinition;

pub fn live_plotter_input_count(config: &Value, fallback_sample: Option<&[f64]>) -> usize {
    let configured = config
        .get("input_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    if configured > 0 {
        configured
    } else {
        fallback_sample.map(|s| s.len()).unwrap_or(0)
    }
}

pub fn live_plotter_refresh_hz(config: &Value) -> f64 {
    config
        .get("refresh_hz")
        .and_then(|v| v.as_f64())
        .unwrap_or(60.0)
}

pub fn live_plotter_window_ms(config: &Value) -> f64 {
    config
        .get("window_ms")
        .and_then(|v| v.as_f64())
        .or_else(|| {
            let div = config.get("timebase_ms_div").and_then(|v| v.as_f64())?;
            let cols = config.get("timebase_divisions").and_then(|v| v.as_f64())?;
            Some(div * cols)
        })
        .or_else(|| {
            let mult = config.get("window_multiplier").and_then(|v| v.as_f64())?;
            let val = config.get("window_value").and_then(|v| v.as_f64())?;
            Some(mult * val)
        })
        .unwrap_or(10_000.0)
        .max(1.0)
}

pub fn live_plotter_config(config: &Value, fallback_sample: Option<&[f64]>) -> (usize, f64, f64) {
    (
        live_plotter_input_count(config, fallback_sample),
        live_plotter_refresh_hz(config),
        live_plotter_window_ms(config),
    )
}

pub fn live_plotter_series_names(
    workspace: &WorkspaceDefinition,
    installed: &[InstalledPlugin],
    plotter_id: u64,
    input_count: usize,
) -> Vec<String> {
    let mut names = Vec::with_capacity(input_count);
    for idx in 0..input_count {
        let port = format!("in_{idx}");
        if let Some(conn) = workspace
            .connections
            .iter()
            .find(|conn| conn.to_plugin == plotter_id && conn.to_port == port)
        {
            let source_name = plugin_display_name(installed, workspace, conn.from_plugin);
            names.push(format!("{source_name}:{}", conn.from_port));
        } else {
            names.push(port);
        }
    }
    names
}
