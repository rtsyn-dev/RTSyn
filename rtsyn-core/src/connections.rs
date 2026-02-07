use crate::plugins::{is_extendable_inputs, plugin_display_name, InstalledPlugin};
use serde_json::Value;
use workspace::{ConnectionDefinition, ConnectionRuleError, WorkspaceDefinition};

pub fn extendable_input_index(port: &str) -> Option<usize> {
    if port == "in" {
        Some(0)
    } else {
        port.strip_prefix("in_")
            .and_then(|value| value.parse::<usize>().ok())
    }
}

pub fn next_available_extendable_input_index(workspace: &WorkspaceDefinition, plugin_id: u64) -> usize {
    let mut used = std::collections::HashSet::new();
    for connection in &workspace.connections {
        if connection.to_plugin == plugin_id {
            if let Some(idx) = extendable_input_index(&connection.to_port) {
                used.insert(idx);
            }
        }
    }
    let mut idx = 0;
    while used.contains(&idx) {
        idx += 1;
    }
    idx
}

pub fn ensure_extendable_input_count(
    workspace: &mut WorkspaceDefinition,
    plugin_id: u64,
    required_count: usize,
) {
    let kind = workspace
        .plugins
        .iter()
        .find(|p| p.id == plugin_id)
        .map(|p| p.kind.clone());
    let Some(kind) = kind else {
        return;
    };
    if !is_extendable_inputs(&kind) {
        return;
    }
    let Some(plugin) = workspace.plugins.iter_mut().find(|p| p.id == plugin_id) else {
        return;
    };
    let map = match plugin.config {
        Value::Object(ref mut map) => map,
        _ => {
            plugin.config = Value::Object(serde_json::Map::new());
            match plugin.config {
                Value::Object(ref mut map) => map,
                _ => return,
            }
        }
    };
    let mut input_count = map
        .get("input_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    if input_count < required_count {
        input_count = required_count;
        map.insert("input_count".to_string(), Value::from(input_count as u64));
    }

    if plugin.kind == "csv_recorder" {
        let mut columns: Vec<String> = map
            .get("columns")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|v| v.as_str().unwrap_or("").to_string())
                    .collect()
            })
            .unwrap_or_default();
        if columns.len() < input_count {
            columns.resize(input_count, String::new());
            map.insert(
                "columns".to_string(),
                Value::Array(columns.into_iter().map(Value::from).collect()),
            );
        }
    }
}

pub fn sync_extendable_input_count(workspace: &mut WorkspaceDefinition, plugin_id: u64) {
    let kind = workspace
        .plugins
        .iter()
        .find(|p| p.id == plugin_id)
        .map(|p| p.kind.clone());
    let Some(kind) = kind else {
        return;
    };
    if !is_extendable_inputs(&kind) {
        return;
    }
    let Some(plugin) = workspace.plugins.iter_mut().find(|p| p.id == plugin_id) else {
        return;
    };
    let mut max_idx: Option<usize> = None;
    for conn in &workspace.connections {
        if conn.to_plugin != plugin_id {
            continue;
        }
        if let Some(idx) = conn.to_port.strip_prefix("in_").and_then(|v| v.parse().ok()) {
            max_idx = Some(max_idx.map(|v| v.max(idx)).unwrap_or(idx));
        }
    }
    let required_count = max_idx.map(|v| v + 1).unwrap_or(0);
    let map = match plugin.config {
        Value::Object(ref mut map) => map,
        _ => {
            plugin.config = Value::Object(serde_json::Map::new());
            match plugin.config {
                Value::Object(ref mut map) => map,
                _ => return,
            }
        }
    };
    map.insert("input_count".to_string(), Value::from(required_count as u64));
    if plugin.kind == "csv_recorder" {
        let mut columns: Vec<String> = map
            .get("columns")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|v| v.as_str().unwrap_or("").to_string())
                    .collect()
            })
            .unwrap_or_default();
        if columns.len() > required_count {
            columns.truncate(required_count);
        } else if columns.len() < required_count {
            columns.resize(required_count, String::new());
        }
        map.insert(
            "columns".to_string(),
            Value::Array(columns.into_iter().map(Value::from).collect()),
        );
    }
}

pub fn default_csv_column(
    workspace: &WorkspaceDefinition,
    installed: &[InstalledPlugin],
    recorder_id: u64,
    input_idx: usize,
) -> String {
    let port = format!("in_{input_idx}");
    if let Some(conn) = workspace
        .connections
        .iter()
        .find(|conn| conn.to_plugin == recorder_id && conn.to_port == port)
    {
        let source_name = plugin_display_name(installed, workspace, conn.from_plugin)
            .replace(' ', "_")
            .to_lowercase();
        let port = conn.from_port.to_lowercase();
        return format!("{source_name}_{}_{}", conn.from_plugin, port);
    }
    let recorder_name = plugin_display_name(installed, workspace, recorder_id)
        .replace(' ', "_")
        .to_lowercase();
    format!("{recorder_name}_{}_{}", recorder_id, port.to_lowercase())
}

pub fn add_connection(
    workspace: &mut WorkspaceDefinition,
    installed: &[InstalledPlugin],
    from_plugin: u64,
    from_port: &str,
    to_plugin: u64,
    to_port: &str,
    kind: &str,
) -> Result<(), ConnectionRuleError> {
    if from_plugin == to_plugin {
        return Err(ConnectionRuleError::SelfConnection);
    }

    let mut to_port_string = to_port.to_string();
    if let Some(target) = workspace.plugins.iter().find(|p| p.id == to_plugin) {
        if is_extendable_inputs(&target.kind) && to_port_string == "in" {
            let next_idx = next_available_extendable_input_index(workspace, to_plugin);
            to_port_string = format!("in_{next_idx}");
        }
    }
    let input_idx = to_port_string
        .strip_prefix("in_")
        .and_then(|v| v.parse::<usize>().ok());
    let default_column = input_idx.map(|idx| default_csv_column(workspace, installed, to_plugin, idx));

    let connection = ConnectionDefinition {
        from_plugin,
        from_port: from_port.to_string(),
        to_plugin,
        to_port: to_port_string,
        kind: kind.to_string(),
    };
    workspace::add_connection(&mut workspace.connections, connection, 1)?;

    if let Some(idx) = input_idx {
        if let Some(target) = workspace.plugins.iter().find(|p| p.id == to_plugin) {
            if is_extendable_inputs(&target.kind) {
                ensure_extendable_input_count(workspace, to_plugin, idx + 1);
            }
        }
    }
    if let (Some(idx), Some(default_name)) = (input_idx, default_column) {
        if let Some(plugin) = workspace.plugins.iter_mut().find(|p| p.id == to_plugin) {
            if plugin.kind == "csv_recorder" {
                if let Value::Object(ref mut map) = plugin.config {
                    let input_count = map
                        .get("input_count")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize;
                    let mut columns: Vec<String> = map
                        .get("columns")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .map(|v| v.as_str().unwrap_or("").to_string())
                                .collect()
                        })
                        .unwrap_or_default();
                    if columns.len() < input_count {
                        for _ in columns.len()..input_count {
                            columns.push(String::new());
                        }
                    }
                    if idx < columns.len() && columns[idx].is_empty() {
                        columns[idx] = default_name;
                        map.insert(
                            "columns".to_string(),
                            Value::Array(columns.into_iter().map(Value::from).collect()),
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
