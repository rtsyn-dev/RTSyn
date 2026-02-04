use rtsyn_plugin::{Plugin, PluginContext, PluginError, PluginId, PluginMeta, Port, PortId};
use serde_json::Value;
use std::time::Instant;

pub struct PerformanceMonitorPlugin {
    id: PluginId,
    meta: PluginMeta,
    inputs: Vec<Port>,
    outputs: Vec<Port>,
    last_trigger_time: Option<Instant>,
    max_latency_us: f64,
    output_values: Vec<f64>,
    workspace_period_us: f64,
    period_history: Vec<f64>,
}

impl PerformanceMonitorPlugin {
    pub fn new(id: u64) -> Self {
        Self {
            id: PluginId(id),
            meta: PluginMeta {
                name: "Performance Monitor".to_string(),
                fixed_vars: Vec::new(),
                default_vars: vec![
                    ("max_latency_us".to_string(), Value::from(1000.0)),
                    ("input_count".to_string(), Value::from(0)),
                ],
            },
            inputs: Vec::new(),
            outputs: vec![
                Port { id: PortId("period_us".to_string()) },
                Port { id: PortId("latency_us".to_string()) },
                Port { id: PortId("jitter_us".to_string()) },
                Port { id: PortId("realtime_violation".to_string()) },
            ],
            last_trigger_time: None,
            max_latency_us: 1000.0,
            output_values: vec![0.0; 4],
            workspace_period_us: 1000.0,
            period_history: Vec::with_capacity(10),
        }
    }

    pub fn get_output_values(&self) -> &[f64] {
        &self.output_values
    }

    pub fn get_workspace_period_us(&self) -> f64 {
        self.workspace_period_us
    }

    pub fn set_config(&mut self, max_latency_us: f64, workspace_period_us: f64) {
        self.max_latency_us = max_latency_us;
        self.workspace_period_us = workspace_period_us;
    }
}

impl Plugin for PerformanceMonitorPlugin {
    fn id(&self) -> PluginId {
        self.id
    }

    fn meta(&self) -> &PluginMeta {
        &self.meta
    }

    fn inputs(&self) -> &[Port] {
        &self.inputs
    }

    fn outputs(&self) -> &[Port] {
        &self.outputs
    }

    fn process(&mut self, _ctx: &mut PluginContext) -> Result<(), PluginError> {
        let process_start = Instant::now();
        
        if let Some(last_time) = self.last_trigger_time {
            let actual_period_us = process_start.duration_since(last_time).as_micros() as f64;
            
            // Add to history for jitter calculation
            self.period_history.push(actual_period_us);
            if self.period_history.len() > 10 {
                self.period_history.remove(0);
            }
            
            // Latency: how much we're late (only positive delays count)
            let latency_us = if actual_period_us > self.workspace_period_us {
                actual_period_us - self.workspace_period_us
            } else {
                0.0
            };
            
            // Jitter: standard deviation of recent periods (timing variation)
            let jitter_us = if self.period_history.len() >= 2 {
                let mean = self.period_history.iter().sum::<f64>() / self.period_history.len() as f64;
                let variance = self.period_history.iter()
                    .map(|x| (x - mean).powi(2))
                    .sum::<f64>() / self.period_history.len() as f64;
                variance.sqrt()
            } else {
                0.0
            };
            
            // Real-time violation if latency exceeds threshold
            let violation = if latency_us > self.max_latency_us { 1.0 } else { 0.0 };
            
            self.output_values[0] = actual_period_us;        // period_us
            self.output_values[1] = latency_us;              // latency_us
            self.output_values[2] = jitter_us;               // jitter_us  
            self.output_values[3] = violation;               // realtime_violation
        }
        
        self.last_trigger_time = Some(process_start);
        
        Ok(())
    }
}
