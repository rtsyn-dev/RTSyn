use rtsyn_plugin::{Plugin, PluginContext, PluginError, PluginId, PluginMeta, Port, PortId};
use serde_json::Value;

pub struct LivePlotterPlugin {
    id: PluginId,
    meta: PluginMeta,
    inputs: Vec<Port>,
    input_values: Vec<f64>,
    running: bool,
}

impl LivePlotterPlugin {
    pub fn new(id: u64) -> Self {
        Self {
            id: PluginId(id),
            meta: PluginMeta {
                name: "Live Plotter".to_string(),
                fixed_vars: Vec::new(),
                default_vars: vec![
                    ("input_count".to_string(), Value::from(0)),
                    ("refresh_hz".to_string(), Value::from(60.0)),
                    ("window_multiplier".to_string(), Value::from(1000)),
                    ("window_value".to_string(), Value::from(10)),
                    ("amplitude".to_string(), Value::from(0.0)),
                ],
            },
            inputs: Vec::new(),
            input_values: Vec::new(),
            running: false,
        }
    }

    pub fn set_config(&mut self, input_count: usize, running: bool) {
        if self.inputs.len() != input_count {
            self.inputs = (0..input_count)
                .map(|idx| Port {
                    id: PortId(format!("in_{idx}")),
                })
                .collect();
        }
        if self.input_values.len() != input_count {
            self.input_values.resize(input_count, 0.0);
        }
        self.running = running;
    }

    pub fn set_inputs(&mut self, values: Vec<f64>) {
        self.input_values = values;
    }

    pub fn inputs_values(&self) -> &[f64] {
        &self.input_values
    }

    pub fn is_running(&self) -> bool {
        self.running
    }
}

impl Plugin for LivePlotterPlugin {
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
        &[]
    }

    fn process(&mut self, _ctx: &mut PluginContext) -> Result<(), PluginError> {
        Ok(())
    }
}
