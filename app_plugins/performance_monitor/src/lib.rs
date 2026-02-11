use rtsyn_plugin::prelude::*;
use serde_json::Value;
use std::time::Instant;

pub struct PerformanceMonitorPlugin {
    id: PluginId,
    meta: PluginMeta,
    inputs: Vec<Port>,
    outputs: Vec<Port>,
    last_trigger_time: Option<Instant>,
    max_latency_us: f64,
    max_period_us: f64,
    period_unit: String,
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
                    ("latency".to_string(), Value::from(1000.0)),
                    ("units".to_string(), Value::from("us")),
                    ("input_count".to_string(), Value::from(0)),
                ],
            },
            inputs: Vec::new(),
            outputs: vec![
                Port {
                    id: PortId("period".to_string()),
                },
                Port {
                    id: PortId("latency".to_string()),
                },
                Port {
                    id: PortId("jitter".to_string()),
                },
                Port {
                    id: PortId("realtime_violation".to_string()),
                },
                Port {
                    id: PortId("max_period".to_string()),
                },
            ],
            last_trigger_time: None,
            max_latency_us: 1000.0,
            max_period_us: 0.0,
            period_unit: "us".to_string(),
            output_values: vec![0.0; 5],
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

    pub fn set_config(&mut self, max_latency_us: f64, workspace_period_us: f64, period_unit: &str) {
        self.max_latency_us = max_latency_us;
        self.workspace_period_us = workspace_period_us;
        self.set_period_unit(period_unit);
    }

    fn set_period_unit(&mut self, unit: &str) {
        let normalized = match unit {
            "ns" | "us" | "ms" | "s" => unit,
            _ => "us",
        };
        self.period_unit = normalized.to_string();
    }

    fn convert_us_to_selected_unit(&self, value_us: f64) -> f64 {
        match self.period_unit.as_str() {
            "ns" => value_us * 1_000.0,
            "us" => value_us,
            "ms" => value_us / 1_000.0,
            "s" => value_us / 1_000_000.0,
            _ => value_us,
        }
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
            if actual_period_us > self.max_period_us {
                self.max_period_us = actual_period_us;
            }

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
                let mean =
                    self.period_history.iter().sum::<f64>() / self.period_history.len() as f64;
                let variance = self
                    .period_history
                    .iter()
                    .map(|x| (x - mean).powi(2))
                    .sum::<f64>()
                    / self.period_history.len() as f64;
                variance.sqrt()
            } else {
                0.0
            };

            // Real-time violation if latency exceeds threshold
            let violation = if latency_us > self.max_latency_us {
                1.0
            } else {
                0.0
            };

            self.output_values[0] = self.convert_us_to_selected_unit(actual_period_us); // period
            self.output_values[1] = self.convert_us_to_selected_unit(latency_us); // latency
            self.output_values[2] = self.convert_us_to_selected_unit(jitter_us); // jitter
            self.output_values[3] = violation; // realtime_violation
            self.output_values[4] = self.convert_us_to_selected_unit(self.max_period_us); // max_period
        }

        self.last_trigger_time = Some(process_start);

        Ok(())
    }

    fn ui_schema(&self) -> Option<UISchema> {
        Some(
            UISchema::new().field(
                ConfigField::float("latency", "Latency")
                    .min_f(0.0)
                    .step_f(100.0)
                    .default_value(Value::from(1000.0))
                    .hint("Maximum allowed latency before violation"),
            )
            .field(
                ConfigField::new(
                    "units",
                    "Units",
                    rtsyn_plugin::ui::FieldType::Choice {
                        options: vec![
                            "ns".to_string(),
                            "us".to_string(),
                            "ms".to_string(),
                            "s".to_string(),
                        ],
                    },
                )
                    .default_value(Value::from("us"))
                    .hint("Units used by max_period output"),
            ),
        )
    }

    fn display_schema(&self) -> Option<DisplaySchema> {
        Some(DisplaySchema {
            outputs: vec![
                "period".to_string(),
                "latency".to_string(),
                "jitter".to_string(),
                "realtime_violation".to_string(),
                "max_period".to_string(),
            ],
            inputs: Vec::new(),
            variables: Vec::new(),
        })
    }

    fn get_variable(&self, name: &str) -> Option<Value> {
        match name {
            "latency" | "max_latency_us" => Some(Value::from(self.max_latency_us)),
            "units" | "period_unit" => Some(Value::from(self.period_unit.clone())),
            _ => None,
        }
    }

    fn set_variable(&mut self, name: &str, value: Value) -> Result<(), PluginError> {
        match name {
            "latency" | "max_latency_us" => {
                if let Some(v) = value.as_f64() {
                    self.max_latency_us = v;
                }
            }
            "units" | "period_unit" => {
                if let Some(v) = value.as_str() {
                    self.set_period_unit(v);
                }
            }
            _ => {}
        }
        Ok(())
    }
}
