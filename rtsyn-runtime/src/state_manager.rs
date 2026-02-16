use std::collections::HashMap;

pub struct RuntimeState {
    pub outputs: HashMap<(u64, String), f64>,
    pub input_values: HashMap<(u64, String), f64>,
    pub internal_variable_values: HashMap<(u64, String), serde_json::Value>,
    pub viewer_values: HashMap<(u64, String), serde_json::Value>,
    pub tick_count: u64,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeState {
    pub fn new() -> Self {
        Self {
            outputs: HashMap::new(),
            input_values: HashMap::new(),
            internal_variable_values: HashMap::new(),
            viewer_values: HashMap::new(),
            tick_count: 0,
        }
    }

    pub fn clear(&mut self) {
        self.outputs.clear();
        self.input_values.clear();
        self.internal_variable_values.clear();
        self.viewer_values.clear();
        self.tick_count = 0;
    }

    pub fn update_tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);
    }
}