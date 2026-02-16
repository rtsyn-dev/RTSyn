use crate::validation::Validator;
use std::path::Path;
use workspace::WorkspaceSettings;

#[derive(Debug, Clone)]
pub struct RuntimeSettings {
    pub cores: Vec<usize>,
    pub period_seconds: f64,
    pub time_scale: f64,
    pub time_label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeSettingsSaveTarget {
    Defaults,
    Workspace,
}

#[derive(Debug, Clone)]
pub struct RuntimeSettingsOptions {
    pub frequency_units: Vec<&'static str>,
    pub period_units: Vec<&'static str>,
    pub min_frequency_value: f64,
    pub min_period_value: f64,
    pub max_integration_steps_min: usize,
    pub max_integration_steps_max: usize,
}

pub const RUNTIME_FREQUENCY_UNITS: [&str; 3] = ["hz", "khz", "mhz"];
pub const RUNTIME_PERIOD_UNITS: [&str; 4] = ["ns", "us", "ms", "s"];
pub const RUNTIME_MIN_FREQUENCY_VALUE: f64 = 1.0;
pub const RUNTIME_MIN_PERIOD_VALUE: f64 = 1.0;
pub const RUNTIME_MAX_INTEGRATION_STEPS_MIN: usize = 1;
pub const RUNTIME_MAX_INTEGRATION_STEPS_MAX: usize = 100;

pub fn runtime_settings_options() -> RuntimeSettingsOptions {
    RuntimeSettingsOptions {
        frequency_units: RUNTIME_FREQUENCY_UNITS.to_vec(),
        period_units: RUNTIME_PERIOD_UNITS.to_vec(),
        min_frequency_value: RUNTIME_MIN_FREQUENCY_VALUE,
        min_period_value: RUNTIME_MIN_PERIOD_VALUE,
        max_integration_steps_min: RUNTIME_MAX_INTEGRATION_STEPS_MIN,
        max_integration_steps_max: RUNTIME_MAX_INTEGRATION_STEPS_MAX,
    }
}

pub fn normalize_workspace_settings(
    mut settings: WorkspaceSettings,
) -> Result<WorkspaceSettings, String> {
    settings.frequency_value = settings.frequency_value.max(1.0);
    settings.period_value = settings.period_value.max(1.0);
    normalize_frequency_unit(&settings.frequency_unit)?;
    normalize_period_unit(&settings.period_unit)?;
    Validator::normalize_cores(&mut settings.selected_cores);
    Ok(settings)
}

pub fn load_runtime_settings_file(path: &Path) -> Result<WorkspaceSettings, String> {
    let data = std::fs::read(path).map_err(|e| {
        format!(
            "Failed to read runtime settings file '{}': {e}",
            path.display()
        )
    })?;
    let settings: WorkspaceSettings = serde_json::from_slice(&data).map_err(|e| {
        format!(
            "Failed to parse runtime settings file '{}': {e}",
            path.display()
        )
    })?;
    normalize_workspace_settings(settings)
}

pub fn save_runtime_settings_file(path: &Path, settings: &WorkspaceSettings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let data = serde_json::to_vec_pretty(settings)
        .map_err(|e| format!("Failed to serialize runtime settings: {e}"))?;
    std::fs::write(path, data).map_err(|e| {
        format!(
            "Failed to write runtime settings file '{}': {e}",
            path.display()
        )
    })
}

pub fn normalize_frequency_unit(unit: &str) -> Result<&str, String> {
    if Validator::validate_unit(unit, &["hz", "khz", "mhz"]) {
        Ok(unit)
    } else {
        Err("frequency_unit must be 'hz', 'khz', or 'mhz'".to_string())
    }
}

pub fn normalize_period_unit(unit: &str) -> Result<&str, String> {
    if Validator::validate_unit(unit, &["ns", "us", "ms", "s"]) {
        Ok(unit)
    } else {
        Err("period_unit must be 'ns', 'us', 'ms', or 's'".to_string())
    }
}

pub fn frequency_hz_from(value: f64, unit: &str) -> Result<f64, String> {
    let unit = normalize_frequency_unit(unit)?;
    let multiplier = match unit {
        "hz" => 1.0,
        "khz" => 1_000.0,
        "mhz" => 1_000_000.0,
        _ => 1.0,
    };
    Ok(value * multiplier)
}

pub fn frequency_value_from_hz(hz: f64, unit: &str) -> Result<f64, String> {
    let unit = normalize_frequency_unit(unit)?;
    let divisor = match unit {
        "hz" => 1.0,
        "khz" => 1_000.0,
        "mhz" => 1_000_000.0,
        _ => 1.0,
    };
    Ok(hz / divisor)
}

pub fn period_seconds_from(value: f64, unit: &str) -> Result<f64, String> {
    let unit = normalize_period_unit(unit)?;
    let multiplier = match unit {
        "ns" => 1e-9,
        "us" => 1e-6,
        "ms" => 1e-3,
        "s" => 1.0,
        _ => 1.0,
    };
    Ok(value * multiplier)
}

pub fn period_value_from_seconds(seconds: f64, unit: &str) -> Result<f64, String> {
    let unit = normalize_period_unit(unit)?;
    let divisor = match unit {
        "ns" => 1e-9,
        "us" => 1e-6,
        "ms" => 1e-3,
        "s" => 1.0,
        _ => 1.0,
    };
    Ok(seconds / divisor)
}

pub fn time_scale_and_label(period_unit: &str) -> Result<(f64, String), String> {
    let (scale, label) = match normalize_period_unit(period_unit)? {
        "ns" => (1e9, "time_ns"),
        "us" => (1e6, "time_us"),
        "ms" => (1e3, "time_ms"),
        "s" => (1.0, "time_s"),
        _ => (1.0, "time_s"),
    };
    Ok((scale, label.to_string()))
}