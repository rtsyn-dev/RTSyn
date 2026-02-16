use std::collections::{HashMap, HashSet};
use workspace::WorkspaceDefinition;

#[derive(Default, Clone)]
pub struct RuntimeConnectionCache {
    pub incoming_by_target_port: HashMap<u64, HashMap<String, Vec<(u64, String)>>>,
    pub incoming_ports_by_plugin: HashMap<u64, HashSet<String>>,
    pub outgoing_ports_by_plugin: HashMap<u64, HashSet<String>>,
}

pub fn build_connection_cache(ws: &WorkspaceDefinition) -> RuntimeConnectionCache {
    let mut cache = RuntimeConnectionCache::default();
    for conn in &ws.connections {
        cache
            .incoming_by_target_port
            .entry(conn.to_plugin)
            .or_default()
            .entry(conn.to_port.clone())
            .or_default()
            .push((conn.from_plugin, conn.from_port.clone()));
        cache
            .incoming_ports_by_plugin
            .entry(conn.to_plugin)
            .or_default()
            .insert(conn.to_port.clone());
        cache
            .outgoing_ports_by_plugin
            .entry(conn.from_plugin)
            .or_default()
            .insert(conn.from_port.clone());
    }
    cache
}

#[inline]
pub fn sanitize_signal(value: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

pub fn input_sum_cached(
    cache: &RuntimeConnectionCache,
    outputs: &HashMap<(u64, String), f64>,
    plugin_id: u64,
    port: &str,
) -> f64 {
    let Some(sources) = cache
        .incoming_by_target_port
        .get(&plugin_id)
        .and_then(|ports| ports.get(port))
    else {
        return 0.0;
    };
    let mut total = 0.0;
    for (from_plugin, from_port) in sources {
        if let Some(value) = outputs.get(&(*from_plugin, from_port.clone())) {
            total += *value;
        }
    }
    sanitize_signal(total)
}