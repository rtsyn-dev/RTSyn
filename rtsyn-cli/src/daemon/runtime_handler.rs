use crate::protocol::{DaemonResponse, RuntimeSettingsOptions};
use rtsyn_core::workspace::{runtime_settings_options, RuntimeSettingsSaveTarget, WorkspaceManager};
use rtsyn_runtime::{LogicMessage, LogicSettings};
use std::sync::mpsc;

pub fn runtime_settings_show(workspace_manager: &WorkspaceManager) -> DaemonResponse {
    DaemonResponse::RuntimeSettings {
        settings: workspace_manager.workspace.settings.clone(),
    }
}

pub fn runtime_settings_set(
    workspace_manager: &mut WorkspaceManager,
    logic_settings: &mut LogicSettings,
    logic_tx: &mpsc::Sender<LogicMessage>,
    json: String,
) -> DaemonResponse {
    match workspace_manager.apply_runtime_settings_json(&json) {
        Ok(()) => match workspace_manager.runtime_settings() {
            Ok(runtime_settings) => {
                logic_settings.cores = runtime_settings.cores;
                logic_settings.period_seconds = runtime_settings.period_seconds;
                logic_settings.time_scale = runtime_settings.time_scale;
                logic_settings.time_label = runtime_settings.time_label;
                let _ = logic_tx.send(LogicMessage::UpdateSettings(logic_settings.clone()));
                DaemonResponse::Ok {
                    message: "Runtime settings updated".to_string(),
                }
            }
            Err(err) => DaemonResponse::Error { message: err },
        },
        Err(err) => DaemonResponse::Error { message: err },
    }
}

pub fn runtime_settings_save(workspace_manager: &mut WorkspaceManager) -> DaemonResponse {
    match workspace_manager.persist_runtime_settings_current_context() {
        Ok(RuntimeSettingsSaveTarget::Defaults) => DaemonResponse::Ok {
            message: "Default values saved".to_string(),
        },
        Ok(RuntimeSettingsSaveTarget::Workspace) => DaemonResponse::Ok {
            message: "Workspace values saved".to_string(),
        },
        Err(err) => DaemonResponse::Error { message: err },
    }
}

pub fn runtime_settings_restore(
    workspace_manager: &mut WorkspaceManager,
    logic_settings: &mut LogicSettings,
    logic_tx: &mpsc::Sender<LogicMessage>,
) -> DaemonResponse {
    match workspace_manager.restore_runtime_settings_current_context() {
        Ok(_) => match workspace_manager.runtime_settings() {
            Ok(runtime_settings) => {
                logic_settings.cores = runtime_settings.cores;
                logic_settings.period_seconds = runtime_settings.period_seconds;
                logic_settings.time_scale = runtime_settings.time_scale;
                logic_settings.time_label = runtime_settings.time_label;
                let _ = logic_tx.send(LogicMessage::UpdateSettings(logic_settings.clone()));
                DaemonResponse::Ok {
                    message: "Default values restored".to_string(),
                }
            }
            Err(err) => DaemonResponse::Error { message: err },
        },
        Err(err) => DaemonResponse::Error { message: err },
    }
}

pub fn runtime_settings_options() -> DaemonResponse {
    let options = runtime_settings_options();
    DaemonResponse::RuntimeSettingsOptions {
        options: RuntimeSettingsOptions {
            frequency_units: options
                .frequency_units
                .into_iter()
                .map(str::to_string)
                .collect(),
            period_units: options
                .period_units
                .into_iter()
                .map(str::to_string)
                .collect(),
            min_frequency_value: options.min_frequency_value,
            min_period_value: options.min_period_value,
            max_integration_steps_min: options.max_integration_steps_min,
            max_integration_steps_max: options.max_integration_steps_max,
        },
    }
}

pub fn runtime_uml_diagram(workspace_manager: &WorkspaceManager) -> DaemonResponse {
    DaemonResponse::RuntimeUmlDiagram {
        uml: workspace_manager.current_workspace_uml_diagram(),
    }
}