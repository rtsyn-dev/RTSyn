use connection::{Connection, ConnectionConfig, ConnectionFactory, ConnectionKind};
use std::collections::{HashMap, HashSet};
use workspace::WorkspaceDefinition;

#[derive(Default)]
pub struct RuntimeConnectionCache {
    pub incoming_by_target_port: HashMap<u64, HashMap<String, Vec<usize>>>,
    pub incoming_edges_by_plugin: HashMap<u64, Vec<usize>>,
    pub incoming_ports_by_plugin: HashMap<u64, HashSet<String>>,
    pub outgoing_ports_by_plugin: HashMap<u64, HashSet<String>>,
    pub outgoing_by_source_port: HashMap<u64, HashMap<String, Vec<usize>>>,
    edges: Vec<ConnectionEdge>,
}

struct ConnectionEdge {
    transport: Box<dyn Connection<f64>>,
    last_value: f64,
}

pub fn build_connection_cache(ws: &WorkspaceDefinition) -> RuntimeConnectionCache {
    let mut cache = RuntimeConnectionCache::default();
    for conn in &ws.connections {
        let edge_idx = cache.edges.len();
        cache.edges.push(ConnectionEdge {
            transport: ConnectionFactory::create(&ConnectionConfig {
                kind: parse_connection_kind(&conn.kind),
                queue_capacity: 64,
            }),
            last_value: 0.0,
        });
        cache
            .incoming_by_target_port
            .entry(conn.to_plugin)
            .or_default()
            .entry(conn.to_port.clone())
            .or_default()
            .push(edge_idx);
        cache
            .incoming_edges_by_plugin
            .entry(conn.to_plugin)
            .or_default()
            .push(edge_idx);
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
        cache
            .outgoing_by_source_port
            .entry(conn.from_plugin)
            .or_default()
            .entry(conn.from_port.clone())
            .or_default()
            .push(edge_idx);
    }
    cache
}

fn parse_connection_kind(kind: &str) -> ConnectionKind {
    match kind {
        "shared_memory" => ConnectionKind::SharedMemory,
        "pipe" => ConnectionKind::Pipe,
        "in_process" => ConnectionKind::InProcess,
        _ => ConnectionKind::InProcess,
    }
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
    plugin_id: u64,
    port: &str,
) -> f64 {
    let Some(edge_indices) = cache
        .incoming_by_target_port
        .get(&plugin_id)
        .and_then(|ports| ports.get(port))
    else {
        return 0.0;
    };
    let mut total = 0.0;
    for edge_idx in edge_indices {
        if let Some(edge) = cache.edges.get(*edge_idx) {
            total += edge.last_value;
        }
    }
    sanitize_signal(total)
}

impl RuntimeConnectionCache {
    pub fn refresh_plugin_inputs(&mut self, plugin_id: u64) {
        let Some(indices) = self.incoming_edges_by_plugin.get(&plugin_id) else {
            return;
        };
        for edge_idx in indices {
            let Some(edge) = self.edges.get_mut(*edge_idx) else {
                continue;
            };
            loop {
                match edge.transport.try_recv() {
                    Ok(Some(value)) => {
                        edge.last_value = sanitize_signal(value);
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        }
    }

    pub fn publish_output(&mut self, plugin_id: u64, port: &str, value: f64) {
        let Some(port_map) = self.outgoing_by_source_port.get(&plugin_id) else {
            return;
        };
        let Some(indices) = port_map.get(port) else {
            return;
        };
        for edge_idx in indices {
            let Some(edge) = self.edges.get_mut(*edge_idx) else {
                continue;
            };
            let value = sanitize_signal(value);
            if edge.transport.send(value).is_err() {
                loop {
                    match edge.transport.try_recv() {
                        Ok(Some(dropped)) => {
                            edge.last_value = sanitize_signal(dropped);
                        }
                        Ok(None) => break,
                        Err(_) => break,
                    }
                }
                let _ = edge.transport.send(value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{build_connection_cache, input_sum_cached};
    use workspace::{ConnectionDefinition, PluginDefinition, WorkspaceDefinition, WorkspaceSettings};

    fn test_workspace(connections: Vec<ConnectionDefinition>) -> WorkspaceDefinition {
        WorkspaceDefinition {
            name: "test".to_string(),
            description: "test".to_string(),
            target_hz: 1000,
            plugins: vec![
                PluginDefinition {
                    id: 1,
                    kind: "mock".to_string(),
                    config: serde_json::json!({}),
                    priority: 0,
                    running: true,
                },
                PluginDefinition {
                    id: 2,
                    kind: "mock".to_string(),
                    config: serde_json::json!({}),
                    priority: 0,
                    running: true,
                },
            ],
            connections,
            settings: WorkspaceSettings::default(),
        }
    }

    #[test]
    fn routes_values_through_pipe_and_shared_memory() {
        let ws = test_workspace(vec![
            ConnectionDefinition {
                from_plugin: 1,
                from_port: "out".to_string(),
                to_plugin: 2,
                to_port: "in".to_string(),
                kind: "shared_memory".to_string(),
            },
            ConnectionDefinition {
                from_plugin: 1,
                from_port: "out".to_string(),
                to_plugin: 2,
                to_port: "in".to_string(),
                kind: "pipe".to_string(),
            },
        ]);
        let mut cache = build_connection_cache(&ws);
        cache.publish_output(1, "out", 2.5);
        cache.refresh_plugin_inputs(2);
        assert_eq!(input_sum_cached(&cache, 2, "in"), 5.0);

        cache.refresh_plugin_inputs(2);
        assert_eq!(input_sum_cached(&cache, 2, "in"), 5.0);
    }

    #[test]
    fn unknown_connection_kind_falls_back_to_in_process() {
        let ws = test_workspace(vec![ConnectionDefinition {
            from_plugin: 1,
            from_port: "out".to_string(),
            to_plugin: 2,
            to_port: "in".to_string(),
            kind: "custom_transport".to_string(),
        }]);
        let mut cache = build_connection_cache(&ws);
        cache.publish_output(1, "out", 3.0);
        cache.refresh_plugin_inputs(2);
        assert_eq!(input_sum_cached(&cache, 2, "in"), 3.0);
    }
}
