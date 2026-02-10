use rtsyn_plugin::prelude::*;
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

    fn ui_schema(&self) -> Option<UISchema> {
        Some(
            UISchema::new()
                .field(
                    ConfigField::float("refresh_hz", "Refresh Hz")
                        .min_f(1.0)
                        .max_f(120.0)
                        .default_value(Value::from(60.0))
                        .hint("Plot refresh rate"),
                )
                .field(
                    ConfigField::integer("window_multiplier", "Window multiplier")
                        .min(1)
                        .step(100)
                        .default_value(Value::from(1000)),
                )
                .field(
                    ConfigField::integer("window_value", "Window value")
                        .min(1)
                        .default_value(Value::from(10)),
                )
                .field(
                    ConfigField::float("amplitude", "Amplitude")
                        .min_f(0.0)
                        .step_f(0.1)
                        .default_value(Value::from(0.0))
                        .hint("Y-axis amplitude (0 = auto)"),
                ),
        )
    }

    fn behavior(&self) -> PluginBehavior {
        PluginBehavior {
            supports_start_stop: true,
            supports_restart: false,
            extendable_inputs: ExtendableInputs::Auto {
                pattern: "in_{}".to_string(),
            },
            loads_started: false,
        }
    }

    fn connection_behavior(&self) -> ConnectionBehavior {
        ConnectionBehavior { dependent: true }
    }

    fn display_schema(&self) -> Option<DisplaySchema> {
        Some(DisplaySchema {
            outputs: vec![],
            inputs: self.inputs.iter().map(|p| p.id.0.clone()).collect(),
            variables: Vec::new(),
        })
    }

    fn get_variable(&self, name: &str) -> Option<Value> {
        self.meta
            .default_vars
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    }

    fn set_variable(&mut self, name: &str, value: Value) -> Result<(), PluginError> {
        if let Some(var) = self.meta.default_vars.iter_mut().find(|(k, _)| k == name) {
            var.1 = value;
        }
        Ok(())
    }

    fn on_input_added(&mut self, port: &str) -> Result<(), PluginError> {
        if let Some(idx) = port
            .strip_prefix("in_")
            .and_then(|s| s.parse::<usize>().ok())
        {
            while self.inputs.len() <= idx {
                let i = self.inputs.len();
                self.inputs.push(Port {
                    id: PortId(format!("in_{}", i)),
                });
                self.input_values.push(0.0);
            }
        }
        Ok(())
    }

    fn on_input_removed(&mut self, port: &str) -> Result<(), PluginError> {
        if let Some(idx) = port
            .strip_prefix("in_")
            .and_then(|s| s.parse::<usize>().ok())
        {
            if idx < self.inputs.len() {
                self.inputs.remove(idx);
                if idx < self.input_values.len() {
                    self.input_values.remove(idx);
                }
                // Reindex remaining
                for (i, input) in self.inputs.iter_mut().enumerate() {
                    input.id = PortId(format!("in_{}", i));
                }
            }
        }
        Ok(())
    }
}
