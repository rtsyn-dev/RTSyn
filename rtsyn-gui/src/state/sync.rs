use rtsyn_runtime::{LogicMessage, LogicState};
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

pub struct StateSync {
    pub logic_tx: Sender<LogicMessage>,
    pub logic_state_rx: Receiver<LogicState>,
    pub computed_outputs: HashMap<(u64, String), f64>,
    pub input_values: HashMap<(u64, String), f64>,
    pub internal_variable_values: HashMap<(u64, String), serde_json::Value>,
    pub viewer_values: HashMap<u64, f64>,
    pub last_output_update: Instant,
    pub logic_period_seconds: f64,
    pub logic_time_scale: f64,
    pub logic_time_label: String,
    pub logic_ui_hz: f64,
}

impl StateSync {
    pub fn new(logic_tx: Sender<LogicMessage>, logic_state_rx: Receiver<LogicState>) -> Self {
        Self {
            logic_tx,
            logic_state_rx,
            computed_outputs: HashMap::new(),
            input_values: HashMap::new(),
            internal_variable_values: HashMap::new(),
            viewer_values: HashMap::new(),
            last_output_update: Instant::now(),
            logic_period_seconds: 0.001,
            logic_time_scale: 1000.0,
            logic_time_label: "time_ms".to_string(),
            logic_ui_hz: 60.0,
        }
    }
}
