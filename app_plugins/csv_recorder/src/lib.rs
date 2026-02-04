use rtsyn_plugin::{
    EventLogger, Plugin, PluginContext, PluginError, PluginId, PluginMeta, Port, PortId,
};
use serde_json::Value;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct CsvRecorderedPlugin {
    id: PluginId,
    meta: PluginMeta,
    inputs: Vec<Port>,
    separator: String,
    columns: Vec<String>,
    include_time: bool,
    time_scale: f64,
    time_label: String,
    time_seconds: f64,
    time_step: f64,
    path: Option<PathBuf>,
    file: Option<std::fs::File>,
    header_written: bool,
    input_values: Vec<f64>,
    recording: bool,
}

impl CsvRecorderedPlugin {
    pub fn new(id: u64) -> Self {
        Self {
            id: PluginId(id),
            meta: PluginMeta {
                name: "Csv Recorder".to_string(),
                fixed_vars: Vec::new(),
                default_vars: vec![
                    ("separator".to_string(), Value::from(",")),
                    ("path".to_string(), Value::from("")),
                    ("input_count".to_string(), Value::from(0)),
                    ("include_time".to_string(), Value::from(true)),
                ],
            },
            inputs: Vec::new(),
            separator: ",".to_string(),
            columns: Vec::new(),
            include_time: true,
            time_scale: 1000.0,
            time_label: "time_ms".to_string(),
            time_seconds: 0.0,
            time_step: 0.001,
            path: None,
            file: None,
            header_written: false,
            input_values: Vec::new(),
            recording: false,
        }
    }

    pub fn set_inputs(&mut self, values: Vec<f64>) {
        self.input_values = values;
    }

    pub fn set_config(
        &mut self,
        input_count: usize,
        separator: String,
        columns: Vec<String>,
        path: Option<PathBuf>,
        recording: bool,
        include_time: bool,
        time_scale: f64,
        time_label: String,
        time_step: f64,
    ) {
        let changed = self.separator != separator
            || self.columns != columns
            || self.path != path
            || self.recording != recording
            || self.include_time != include_time;
        if !self.recording && recording {
            self.time_seconds = 0.0;
        }
        self.recording = recording;
        if changed {
            self.separator = separator;
            self.columns = columns;
            self.path = path;
            self.include_time = include_time;
            self.reopen_file();
        }
        self.time_scale = time_scale;
        self.time_label = time_label;
        self.time_step = time_step;

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
    }

    fn reopen_file(&mut self) {
        self.file = None;
        self.header_written = false;
        if !self.recording {
            return;
        }
        let Some(path) = self.path.as_ref() else {
            return;
        };
        if path.as_os_str().is_empty() {
            return;
        }
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path);
        if let Ok(file) = file {
            self.file = Some(file);
        }
    }

    fn write_header(&mut self) -> Result<(), PluginError> {
        let Some(file) = self.file.as_mut() else {
            return Ok(());
        };
        if self.header_written {
            return Ok(());
        }
        let header = if self.include_time {
            let mut columns = Vec::with_capacity(self.columns.len() + 1);
            columns.push(self.time_label.clone());
            columns.extend(self.columns.iter().cloned());
            columns.join(&self.separator)
        } else {
            self.columns.join(&self.separator)
        };
        writeln!(file, "{header}").map_err(|_| PluginError::ProcessingFailed)?;
        self.header_written = true;
        Ok(())
    }
}

impl Plugin for CsvRecorderedPlugin {
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
        if !self.recording || self.file.is_none() {
            return Ok(());
        }
        self.write_header()?;
        let Some(file) = self.file.as_mut() else {
            return Ok(());
        };
        let mut values = Vec::with_capacity(self.input_values.len() + 1);
        if self.include_time {
            values.push(format!("{}", self.time_seconds * self.time_scale));
        }
        values.extend(self.input_values.iter().map(|value| value.to_string()));
        let values = values.join(&self.separator);
        writeln!(file, "{values}").map_err(|_| PluginError::ProcessingFailed)?;
        if self.include_time {
            self.time_seconds += self.time_step.max(0.0);
        }
        Ok(())
    }
}

impl EventLogger for CsvRecorderedPlugin {
    fn flush(&mut self) -> Result<(), PluginError> {
        if let Some(file) = self.file.as_mut() {
            file.flush().map_err(|_| PluginError::ProcessingFailed)?;
        }
        Ok(())
    }
}

pub fn default_column_name(plugin_name: &str, plugin_id: u64, port: &str) -> String {
    let safe_name = plugin_name.replace(' ', "_").to_lowercase();
    format!("{}_{}_{}", safe_name, plugin_id, port.to_lowercase())
}

pub fn normalize_path(path: &str) -> Option<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(Path::new(trimmed).to_path_buf())
    }
}
