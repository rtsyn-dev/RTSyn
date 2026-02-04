use crate::{ConnectionDefinition, PluginDefinition};
use std::collections::{HashMap, HashSet};

pub fn input_sum(
    connections: &[ConnectionDefinition],
    outputs: &HashMap<(u64, String), f64>,
    plugin_id: u64,
    port: &str,
) -> f64 {
    let mut value = 0.0;
    for connection in connections {
        if connection.to_plugin == plugin_id && connection.to_port == port {
            if let Some(output) =
                outputs.get(&(connection.from_plugin, connection.from_port.clone()))
            {
                value += output;
            }
        }
    }
    value
}

pub fn input_sum_any(
    connections: &[ConnectionDefinition],
    outputs: &HashMap<(u64, String), f64>,
    plugin_id: u64,
    ports: &[String],
) -> f64 {
    let mut total = 0.0;
    for port in ports {
        total += input_sum(connections, outputs, plugin_id, port);
    }
    total
}

pub fn order_plugins_for_execution(
    plugins: &[PluginDefinition],
    connections: &[ConnectionDefinition],
) -> Vec<PluginDefinition> {
    let mut by_priority: HashMap<i32, Vec<PluginDefinition>> = HashMap::new();
    for plugin in plugins {
        by_priority
            .entry(plugin.priority)
            .or_default()
            .push(plugin.clone());
    }

    let mut priorities: Vec<i32> = by_priority.keys().copied().collect();
    priorities.sort();

    let mut ordered = Vec::new();
    for priority in priorities {
        let mut group = by_priority.remove(&priority).unwrap_or_default();
        group.sort_by_key(|p| p.id);

        let ids: HashSet<u64> = group.iter().map(|p| p.id).collect();
        let mut indegree: HashMap<u64, usize> = ids.iter().map(|id| (*id, 0)).collect();
        let mut edges: HashMap<u64, Vec<u64>> = HashMap::new();

        for conn in connections {
            if ids.contains(&conn.from_plugin) && ids.contains(&conn.to_plugin) {
                edges
                    .entry(conn.from_plugin)
                    .or_default()
                    .push(conn.to_plugin);
                if let Some(count) = indegree.get_mut(&conn.to_plugin) {
                    *count += 1;
                }
            }
        }

        let mut ready: Vec<u64> = indegree
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(id, _)| *id)
            .collect();
        ready.sort();

        let mut ordered_ids: Vec<u64> = Vec::new();
        while let Some(id) = ready.first().copied() {
            ready.remove(0);
            ordered_ids.push(id);
            if let Some(children) = edges.get(&id) {
                for child in children {
                    if let Some(count) = indegree.get_mut(child) {
                        if *count > 0 {
                            *count -= 1;
                            if *count == 0 {
                                ready.push(*child);
                            }
                        }
                    }
                }
                ready.sort();
            }
        }

        let mut remaining: Vec<u64> = ids
            .iter()
            .filter(|id| !ordered_ids.contains(id))
            .copied()
            .collect();
        let mut out_degree: HashMap<u64, usize> = HashMap::new();
        for id in &remaining {
            let count = edges.get(id).map(|v| v.len()).unwrap_or(0);
            out_degree.insert(*id, count);
        }
        remaining.sort_by(|a, b| {
            let a_sink = out_degree.get(a).copied().unwrap_or(0) == 0;
            let b_sink = out_degree.get(b).copied().unwrap_or(0) == 0;
            a_sink.cmp(&b_sink).then_with(|| a.cmp(b))
        });
        ordered_ids.extend(remaining);

        for id in ordered_ids {
            if let Some(plugin) = group.iter().find(|p| p.id == id) {
                ordered.push(plugin.clone());
            }
        }
    }

    ordered
}
