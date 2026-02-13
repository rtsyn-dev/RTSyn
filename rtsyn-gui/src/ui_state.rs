// UI state management structs

use crate::state::{
    ConfirmAction, ConnectionEditMode, ConnectionEditTab, HelpTopic, WorkspaceDialogMode,
    WorkspaceTimingTab,
};
use crate::WorkspaceSettingsDraft;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

#[derive(Clone)]
pub struct PlotterPreviewState {
    pub open: bool,
    pub target: Option<u64>,
    pub show_axes: bool,
    pub show_legend: bool,
    pub show_grid: bool,
    pub series_names: Vec<String>,
    pub series_scales: Vec<f64>,
    pub series_offsets: Vec<f64>,
    pub selected_series_tab: usize,
    pub series_tab_start: usize,
    pub colors: Vec<egui::Color32>,
    pub title: String,
    pub dark_theme: bool,
    pub x_axis_name: String,
    pub y_axis_name: String,
    pub window_ms: f64,
    pub timebase_divisions: u32,
    pub high_quality: bool,
    pub export_svg: bool,
    pub width: u32,
    pub height: u32,
}

impl Default for PlotterPreviewState {
    fn default() -> Self {
        Self {
            open: false,
            target: None,
            show_axes: true,
            show_legend: true,
            show_grid: true,
            series_names: Vec::new(),
            series_scales: Vec::new(),
            series_offsets: Vec::new(),
            selected_series_tab: 0,
            series_tab_start: 0,
            colors: Vec::new(),
            title: String::new(),
            dark_theme: false,
            x_axis_name: "Time".to_string(),
            y_axis_name: "Value".to_string(),
            window_ms: 10_000.0,
            timebase_divisions: 10,
            high_quality: false,
            export_svg: false,
            width: 1920,
            height: 1080,
        }
    }
}

pub struct ConnectionEditorState {
    pub from_idx: usize,
    pub to_idx: usize,
    pub from_port: String,
    pub to_port: String,
    pub kind: String,
    pub kind_options: Vec<String>,
    pub open: bool,
    pub mode: ConnectionEditMode,
    pub tab: ConnectionEditTab,
    pub plugin_id: Option<u64>,
    pub selected_idx: Option<usize>,
    pub from_port_idx: usize,
    pub to_port_idx: usize,
    pub last_selected: Option<u64>,
    pub last_tab: Option<ConnectionEditTab>,
}

impl Default for ConnectionEditorState {
    fn default() -> Self {
        Self {
            from_idx: 0,
            to_idx: 0,
            from_port: String::new(),
            to_port: String::new(),
            kind: "shared_memory".to_string(),
            kind_options: vec![
                "shared_memory".to_string(),
                "pipe".to_string(),
                "in_process".to_string(),
            ],
            open: false,
            mode: ConnectionEditMode::Add,
            tab: ConnectionEditTab::Outputs,
            plugin_id: None,
            selected_idx: None,
            from_port_idx: 0,
            to_port_idx: 0,
            last_selected: None,
            last_tab: None,
        }
    }
}

pub struct WorkspaceDialogState {
    pub open: bool,
    pub mode: WorkspaceDialogMode,
    pub name_input: String,
    pub description_input: String,
    pub edit_path: Option<PathBuf>,
}

impl Default for WorkspaceDialogState {
    fn default() -> Self {
        Self {
            open: false,
            mode: WorkspaceDialogMode::New,
            name_input: String::new(),
            description_input: String::new(),
            edit_path: None,
        }
    }
}

pub struct BuildDialogState {
    pub open: bool,
    pub in_progress: bool,
    pub message: String,
    pub title: String,
    pub rx: Option<Receiver<super::BuildResult>>,
}

impl Default for BuildDialogState {
    fn default() -> Self {
        Self {
            open: false,
            in_progress: false,
            message: String::new(),
            title: String::new(),
            rx: None,
        }
    }
}

pub struct ConfirmDialogState {
    pub open: bool,
    pub title: String,
    pub message: String,
    pub action_label: String,
    pub action: Option<ConfirmAction>,
}

impl Default for ConfirmDialogState {
    fn default() -> Self {
        Self {
            open: false,
            title: String::new(),
            message: String::new(),
            action_label: String::new(),
            action: None,
        }
    }
}

pub struct WorkspaceSettingsState {
    pub open: bool,
    pub draft: Option<WorkspaceSettingsDraft>,
    pub tab: WorkspaceTimingTab,
}

impl Default for WorkspaceSettingsState {
    fn default() -> Self {
        Self {
            open: false,
            draft: None,
            tab: WorkspaceTimingTab::Frequency,
        }
    }
}

pub struct HelpState {
    pub open: bool,
    pub topic: HelpTopic,
}

impl Default for HelpState {
    fn default() -> Self {
        Self {
            open: false,
            topic: HelpTopic::RTSyn,
        }
    }
}

pub struct WindowState {
    pub manage_workspace_open: bool,
    pub load_workspace_open: bool,
    pub manage_workspace_selected_index: Option<usize>,
    pub load_workspace_selected_index: Option<usize>,
    pub manage_plugins_open: bool,
    pub install_plugins_open: bool,
    pub uninstall_plugins_open: bool,
    pub install_plugin_search: String,
    pub uninstall_plugin_search: String,
    pub manage_plugin_search: String,
    pub load_workspace_search: String,
    pub manage_workspace_search: String,
    pub install_selected_index: Option<usize>,
    pub uninstall_selected_index: Option<usize>,
    pub manage_plugin_selected_index: Option<usize>,
    pub plugins_open: bool,
    pub new_plugin_open: bool,
    pub plugin_search: String,
    pub plugin_selected_index: Option<usize>,
    pub manage_connections_open: bool,
    pub uml_diagram_open: bool,
    pub plugin_config_open: bool,
    pub plugin_config_id: Option<u64>,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            manage_workspace_open: false,
            load_workspace_open: false,
            manage_workspace_selected_index: None,
            load_workspace_selected_index: None,
            manage_plugins_open: false,
            install_plugins_open: false,
            uninstall_plugins_open: false,
            install_plugin_search: String::new(),
            uninstall_plugin_search: String::new(),
            manage_plugin_search: String::new(),
            load_workspace_search: String::new(),
            manage_workspace_search: String::new(),
            install_selected_index: None,
            uninstall_selected_index: None,
            manage_plugin_selected_index: None,
            plugins_open: false,
            new_plugin_open: false,
            plugin_search: String::new(),
            plugin_selected_index: None,
            manage_connections_open: false,
            uml_diagram_open: false,
            plugin_config_open: false,
            plugin_config_id: None,
        }
    }
}
