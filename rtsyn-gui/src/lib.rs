use eframe::{egui, egui::RichText};
use rtsyn_runtime::runtime::{LogicMessage, LogicSettings, LogicState};
use rtsyn_runtime::spawn_runtime;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::{self, Command};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use workspace::WorkspaceDefinition;

// Helper to check if running with RT capabilities
fn has_rt_capabilities() -> bool {
    #[cfg(unix)]
    unsafe {
        let policy = libc::sched_getscheduler(0);
        policy == libc::SCHED_FIFO || policy == libc::SCHED_RR
    }
    #[cfg(not(unix))]
    false
}

// External file dialog using zenity
fn zenity_file_dialog(mode: &str, filter: Option<&str>) -> Option<PathBuf> {
    zenity_file_dialog_with_name(mode, filter, None)
}

fn zenity_file_dialog_with_name(
    mode: &str,
    filter: Option<&str>,
    filename: Option<&str>,
) -> Option<PathBuf> {
    let mut cmd = Command::new("zenity");
    cmd.arg("--file-selection");

    match mode {
        "save" => {
            cmd.arg("--save");
        }
        "folder" => {
            cmd.arg("--directory");
        }
        _ => {} // open file is default
    }

    if let Some(f) = filter {
        cmd.arg("--file-filter").arg(f);
    }

    if let Some(name) = filename {
        cmd.arg("--filename").arg(name);
    }

    cmd.output().ok().and_then(|output| {
        if output.status.success() {
            let path_string = String::from_utf8_lossy(&output.stdout);
            let path_str = path_string.trim();
            if !path_str.is_empty() {
                Some(PathBuf::from(path_str))
            } else {
                None
            }
        } else {
            None
        }
    })
}

// Helper function to spawn file dialogs that work with RT
fn spawn_file_dialog_thread<F, T>(f: F) -> std::thread::JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    std::thread::spawn(f)
}
use workspace::{input_sum, input_sum_any, ConnectionDefinition, WorkspaceSettings};

// Operation modules
mod connection_operations;
mod dialog_polling;
mod plugin_operations;
mod workspace_operations;

// Core modules
mod daemon_viewer;
mod file_dialogs;
mod notifications;
mod plotter;
mod plotter_manager;
mod state;
mod state_sync;
mod ui;
mod ui_state;
mod utils;

use file_dialogs::FileDialogManager;
use notifications::Notification;
use plotter::LivePlotter;
use plotter_manager::PlotterManager;
use rtsyn_core::plugin::PluginManager;
use rtsyn_core::workspace::WorkspaceManager;
use state::{
    ConfirmAction, FrequencyUnit, PeriodUnit, TimeUnit, WorkspaceDialogMode, WorkspaceTimingTab,
};
use state_sync::StateSync;

#[derive(Debug, Clone)]
pub struct GuiConfig {
    pub title: String,
    pub width: f32,
    pub height: f32,
}

impl Default for GuiConfig {
    fn default() -> Self {
        Self {
            title: "RTSyn".to_string(),
            width: 1280.0,
            height: 720.0,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum GuiError {
    #[error("gui error: {0}")]
    Gui(String),
}

#[derive(Debug, Clone)]
enum BuildAction {
    Install {
        path: PathBuf,
        removable: bool,
        persist: bool,
    },
    Reinstall {
        kind: String,
        path: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowFocus {
    WorkspaceDialog,
    LoadWorkspaces,
    ManageWorkspaces,
    ManagePlugins,
    InstallPlugins,
    UninstallPlugins,
    Plugins,
    NewPlugin,
    WorkspaceSettings,
    UmlDiagram,
    ManageConnections,
    ConnectionEditorAdd,
    ConnectionEditorRemove,
    PluginConfig,
    Help,
}

#[derive(Debug, Clone)]
struct PluginFieldDraft {
    name: String,
    type_name: String,
}

impl Default for PluginFieldDraft {
    fn default() -> Self {
        Self {
            name: String::new(),
            type_name: "f64".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
struct NewPluginDraft {
    name: String,
    language: String,
    main_characteristics: String,
    autostart: bool,
    supports_start_stop: bool,
    supports_restart: bool,
    external_window: bool,
    variables: Vec<PluginFieldDraft>,
    inputs: Vec<PluginFieldDraft>,
    outputs: Vec<PluginFieldDraft>,
    internal_variables: Vec<PluginFieldDraft>,
}

impl Default for NewPluginDraft {
    fn default() -> Self {
        Self {
            name: String::new(),
            language: "rust".to_string(),
            main_characteristics: String::new(),
            autostart: false,
            supports_start_stop: true,
            supports_restart: true,
            external_window: false,
            variables: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            internal_variables: Vec::new(),
        }
    }
}

#[derive(Debug)]
struct BuildResult {
    success: bool,
    action: BuildAction,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct WorkspaceSettingsDraft {
    frequency_value: f64,
    frequency_unit: FrequencyUnit,
    period_value: f64,
    period_unit: PeriodUnit,
    tab: WorkspaceTimingTab,
    max_integration_steps: usize,
}

pub fn run_gui(config: GuiConfig) -> Result<(), GuiError> {
    if let Ok(id_str) = std::env::var("RTSYN_DAEMON_VIEW_PLUGIN_ID") {
        if let Ok(plugin_id) = id_str.parse::<u64>() {
            let socket_path = std::env::var("RTSYN_DAEMON_SOCKET")
                .unwrap_or_else(|_| "/tmp/rtsyn-daemon.sock".to_string());
            return daemon_viewer::run_daemon_plugin_viewer(config, plugin_id, socket_path);
        }
    }
    let (logic_tx, logic_state_rx) = match spawn_runtime() {
        Ok(tuple) => tuple,
        Err(err) => {
            eprintln!("Failed to start logic runtime: {err}");
            process::exit(1);
        }
    };
    run_gui_with_runtime(config, logic_tx, logic_state_rx)
}

pub fn run_gui_with_runtime(
    config: GuiConfig,
    logic_tx: Sender<LogicMessage>,
    logic_state_rx: Receiver<LogicState>,
) -> Result<(), GuiError> {
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([config.width, config.height]),
        ..Default::default()
    };
    // NOTE: Vsync generates hangs and lag on occluded windows.
    options.vsync = false;

    eframe::run_native(
        &config.title,
        options,
        Box::new(move |cc| {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "fa".to_string(),
                egui::FontData::from_static(include_bytes!("../assets/fonts/fa-solid-900.ttf")),
            );
            let family = fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default();
            if !family.contains(&"fa".to_string()) {
                family.push("fa".to_string());
            }
            cc.egui_ctx.set_fonts(fonts);
            Box::new(GuiApp::new_with_runtime(logic_tx, logic_state_rx))
        }),
    )
    .map_err(|err| GuiError::Gui(err.to_string()))
}

struct GuiApp {
    // Managers
    plugin_manager: PluginManager,
    workspace_manager: WorkspaceManager,
    file_dialogs: FileDialogManager,
    plotter_manager: PlotterManager,
    state_sync: StateSync,

    // UI State Groups
    plotter_preview: ui_state::PlotterPreviewState,
    connection_editor: ui_state::ConnectionEditorState,
    workspace_dialog: ui_state::WorkspaceDialogState,
    build_dialog: ui_state::BuildDialogState,
    confirm_dialog: ui_state::ConfirmDialogState,
    workspace_settings: ui_state::WorkspaceSettingsState,
    help_state: ui_state::HelpState,
    windows: ui_state::WindowState,

    // Remaining UI State
    status: String,
    csv_path_target_plugin_id: Option<u64>,
    plugin_creator_last_path: Option<PathBuf>,
    new_plugin_draft: NewPluginDraft,
    notifications: Vec<Notification>,
    plugin_positions: HashMap<u64, egui::Pos2>,
    plugin_rects: HashMap<u64, egui::Rect>,
    connections_view_enabled: bool,
    available_cores: usize,
    selected_cores: Vec<bool>,
    frequency_value: f64,
    frequency_unit: FrequencyUnit,
    period_value: f64,
    period_unit: PeriodUnit,
    output_refresh_hz: f64,
    plotter_screenshot_target: Option<u64>,
    connection_highlight_plugin_id: Option<u64>,
    selected_plugin_id: Option<u64>,
    plugin_context_menu: Option<(u64, egui::Pos2, u64)>,
    connection_context_menu: Option<(Vec<ConnectionDefinition>, egui::Pos2, u64)>,
    number_edit_buffers: HashMap<(u64, String), String>,
    window_rects: Vec<egui::Rect>,
    pending_window_focus: Option<WindowFocus>,
    uml_preview_texture: Option<egui::TextureHandle>,
    uml_preview_hash: Option<u64>,
    uml_preview_error: Option<String>,
    uml_preview_loading: bool,
    uml_preview_rx: Option<Receiver<(u64, Result<Vec<u8>, String>)>>,
    uml_text_buffer: String,
    uml_export_svg: bool,
    uml_export_width: u32,
    uml_export_height: u32,
    uml_preview_zoom: f32,
}

impl GuiApp {
    fn new_with_runtime(
        logic_tx: Sender<LogicMessage>,
        logic_state_rx: Receiver<LogicState>,
    ) -> Self {
        let install_db_path = PathBuf::from("app_plugins").join("installed_plugins.json");
        let workspace_dir = PathBuf::from("app_workspaces");

        let available_cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);

        let mut plugin_manager = PluginManager::new(install_db_path);
        let mut workspace_manager = WorkspaceManager::new(workspace_dir);
        let file_dialogs = FileDialogManager::new();
        let plotter_manager = PlotterManager::new();
        let state_sync = StateSync::new(logic_tx, logic_state_rx);

        plugin_manager.refresh_library_paths();
        workspace_manager
            .workspace
            .plugins
            .iter_mut()
            .for_each(|p| {
                if let Some(installed) = plugin_manager
                    .installed_plugins
                    .iter()
                    .find(|i| i.manifest.kind == p.kind)
                {
                    if let Some(lib_path) = &installed.library_path {
                        if let Some(config) = p.config.as_object_mut() {
                            config.insert(
                                "library_path".to_string(),
                                serde_json::Value::String(lib_path.to_string_lossy().to_string()),
                            );
                        }
                    }
                }
            });

        let mut app = Self {
            plugin_manager,
            workspace_manager,
            file_dialogs,
            plotter_manager,
            state_sync,
            plotter_preview: ui_state::PlotterPreviewState::default(),
            connection_editor: ui_state::ConnectionEditorState::default(),
            workspace_dialog: ui_state::WorkspaceDialogState::default(),
            build_dialog: ui_state::BuildDialogState::default(),
            confirm_dialog: ui_state::ConfirmDialogState::default(),
            workspace_settings: ui_state::WorkspaceSettingsState::default(),
            help_state: ui_state::HelpState::default(),
            windows: ui_state::WindowState::default(),
            status: String::new(),
            csv_path_target_plugin_id: None,
            plugin_creator_last_path: None,
            new_plugin_draft: NewPluginDraft::default(),
            notifications: Vec::new(),
            plugin_positions: HashMap::new(),
            plugin_rects: HashMap::new(),
            connections_view_enabled: true,
            available_cores,
            selected_cores: (0..available_cores).map(|i| i == 0).collect(),
            frequency_value: 1000.0,
            frequency_unit: FrequencyUnit::Hz,
            period_value: 1.0,
            period_unit: PeriodUnit::Ms,
            output_refresh_hz: 1.0,
            plotter_screenshot_target: None,
            connection_highlight_plugin_id: None,
            selected_plugin_id: None,
            plugin_context_menu: None,
            connection_context_menu: None,
            number_edit_buffers: HashMap::new(),
            window_rects: Vec::new(),
            pending_window_focus: None,
            uml_preview_texture: None,
            uml_preview_hash: None,
            uml_preview_error: None,
            uml_preview_loading: false,
            uml_preview_rx: None,
            uml_text_buffer: String::new(),
            uml_export_svg: false,
            uml_export_width: 1920,
            uml_export_height: 1080,
            uml_preview_zoom: 0.0,
        };
        app.refresh_installed_plugin_metadata_cache();
        app.apply_workspace_settings();
        app
    }

    fn center_window(ctx: &egui::Context, size: egui::Vec2) -> egui::Pos2 {
        let rect = ctx.available_rect();
        let center = rect.center();
        center - size * 0.5
    }

    fn sync_next_plugin_id(&mut self) {
        let max_id = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .map(|p| p.id)
            .max();
        self.plugin_manager.sync_next_plugin_id(max_id);
    }

    fn mark_workspace_dirty(&mut self) {
        self.workspace_manager.mark_dirty();
    }

    fn restart_plugin(&mut self, plugin_id: u64) {
        let _ = self
            .state_sync
            .logic_tx
            .send(LogicMessage::RestartPlugin(plugin_id));
    }

    fn display_kind(kind: &str) -> String {
        PluginManager::display_kind(kind)
    }

    fn show_info(&mut self, title: &str, message: &str) {
        self.push_notification(title, message);
    }

    fn push_notification(&mut self, title: &str, message: &str) {
        let notification = Notification {
            title: title.to_string(),
            message: message.to_string(),
            created_at: Instant::now(),
        };
        self.notifications.push(notification);
    }

    fn show_confirm(
        &mut self,
        title: &str,
        message: &str,
        action_label: &str,
        action: ConfirmAction,
    ) {
        self.confirm_dialog.title = title.to_string();
        self.confirm_dialog.message = message.to_string();
        self.confirm_dialog.action_label = action_label.to_string();
        self.confirm_dialog.action = Some(action);
        self.confirm_dialog.open = true;
    }

    fn perform_confirm_action(&mut self, action: ConfirmAction) {
        match action {
            ConfirmAction::RemovePlugin(plugin_id) => {
                if let Some(index) = self
                    .workspace_manager
                    .workspace
                    .plugins
                    .iter()
                    .position(|plugin| plugin.id == plugin_id)
                {
                    self.remove_plugin(index);
                }
            }
            ConfirmAction::UninstallPlugin(index) => {
                self.uninstall_plugin(index);
            }
            ConfirmAction::DeleteWorkspace(path) => {
                let name = WorkspaceDefinition::load_from_file(&path)
                    .map(|ws| ws.name)
                    .unwrap_or_else(|_| {
                        path.file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("workspace")
                            .replace('_', " ")
                    });
                match self.workspace_manager.delete_workspace(&name) {
                    Ok(()) => {
                        if self.workspace_manager.workspace_path.as_os_str().is_empty() {
                            self.plotter_manager.plotters.clear();
                            self.apply_workspace_settings();
                            self.plugin_positions.clear();
                        }
                        self.scan_workspaces();
                        self.show_info("Workspace", &format!("Workspace '{}' deleted", name));
                    }
                    Err(err) => {
                        self.show_info("Workspace Error", &err);
                    }
                }
            }
        }
    }

    fn poll_logic_state(&mut self) {
        let mut latest: Option<LogicState> = None;
        let mut merged_samples: HashMap<u64, Vec<(u64, Vec<f64>)>> = HashMap::new();
        while let Ok(state) = self.state_sync.logic_state_rx.try_recv() {
            for (plugin_id, samples) in &state.plotter_samples {
                let entry = merged_samples.entry(*plugin_id).or_default();
                entry.extend(samples.iter().cloned());
            }
            latest = Some(state);
        }
        if let Some(state) = latest {
            let outputs = state.outputs;
            let input_values = state.input_values;
            let internal_variable_values = state.internal_variable_values;
            let viewer_values = state.viewer_values;
            let tick = state.tick;
            self.update_plotters(tick, &outputs, &merged_samples);
            let output_interval = if self.output_refresh_hz > 0.0 {
                Duration::from_secs_f64(1.0 / self.output_refresh_hz)
            } else {
                Duration::from_secs(1)
            };
            if self.state_sync.last_output_update.elapsed() >= output_interval {
                // Filter out outputs from stopped plugins
                let running_plugins: std::collections::HashSet<u64> = self
                    .workspace_manager
                    .workspace
                    .plugins
                    .iter()
                    .filter(|p| p.running)
                    .map(|p| p.id)
                    .collect();

                let filtered_outputs: HashMap<(u64, String), f64> = outputs
                    .into_iter()
                    .filter(|((id, _), _)| running_plugins.contains(id))
                    .collect();
                let filtered_inputs: HashMap<(u64, String), f64> = input_values
                    .into_iter()
                    .filter(|((id, _), _)| running_plugins.contains(id))
                    .collect();
                let filtered_internals: HashMap<(u64, String), serde_json::Value> =
                    internal_variable_values
                        .into_iter()
                        .filter(|((id, _), _)| running_plugins.contains(id))
                        .collect();

                self.state_sync.computed_outputs = filtered_outputs;
                self.state_sync.input_values = filtered_inputs;
                self.state_sync.internal_variable_values = filtered_internals;
                self.state_sync.viewer_values = viewer_values;
                self.state_sync.last_output_update = Instant::now();
            }
        }
    }

    fn ports_for_kind(&self, kind: &str, inputs: bool) -> Vec<String> {
        self.plugin_manager
            .installed_plugins
            .iter()
            .find(|plugin| plugin.manifest.kind == kind)
            .map(|plugin| {
                if inputs {
                    plugin.metadata_inputs.clone()
                } else {
                    plugin.metadata_outputs.clone()
                }
            })
            .unwrap_or_default()
    }

    fn is_extendable_inputs(&self, kind: &str) -> bool {
        if let Some(cached) = self.plugin_manager.plugin_behaviors.get(kind) {
            return matches!(
                cached.extendable_inputs,
                rtsyn_plugin::ui::ExtendableInputs::Auto { .. }
                    | rtsyn_plugin::ui::ExtendableInputs::Manual
            );
        }
        rtsyn_core::plugin::is_extendable_inputs(kind)
    }

    fn auto_extend_inputs(&self, kind: &str) -> bool {
        if let Some(cached) = self.plugin_manager.plugin_behaviors.get(kind) {
            return matches!(
                cached.extendable_inputs,
                rtsyn_plugin::ui::ExtendableInputs::Auto { .. }
            );
        }
        matches!(kind, "csv_recorder" | "live_plotter")
    }

    fn ensure_plugin_behavior_cached_with_path(
        &mut self,
        kind: &str,
        library_path: Option<&PathBuf>,
    ) {
        if self.plugin_manager.plugin_behaviors.contains_key(kind) {
            return;
        }

        let (tx, rx) = std::sync::mpsc::channel();
        let path_str = library_path.map(|p| p.to_string_lossy().to_string());
        let _ = self
            .state_sync
            .logic_tx
            .send(LogicMessage::QueryPluginBehavior(
                kind.to_string(),
                path_str,
                tx,
            ));
        if let Ok(Some(behavior)) = rx.recv_timeout(std::time::Duration::from_millis(100)) {
            self.plugin_manager
                .plugin_behaviors
                .insert(kind.to_string(), behavior);
        }
    }

    fn ensure_plugin_behavior_cached(&mut self, kind: &str) {
        if self.plugin_manager.plugin_behaviors.contains_key(kind) {
            return;
        }

        let (tx, rx) = std::sync::mpsc::channel();
        let _ = self
            .state_sync
            .logic_tx
            .send(LogicMessage::QueryPluginBehavior(
                kind.to_string(),
                None,
                tx,
            ));
        if let Ok(Some(behavior)) = rx.recv_timeout(std::time::Duration::from_millis(100)) {
            self.plugin_manager
                .plugin_behaviors
                .insert(kind.to_string(), behavior);
        }
    }

    fn ports_for_plugin(&self, plugin_id: u64, inputs: bool) -> Vec<String> {
        let Some(plugin) = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .find(|p| p.id == plugin_id)
        else {
            return Vec::new();
        };
        let extendable_inputs = self.is_extendable_inputs(&plugin.kind);
        if extendable_inputs && inputs {
            let columns_len = plugin
                .config
                .get("columns")
                .and_then(|v| v.as_array())
                .map(|arr| arr.len())
                .unwrap_or(0);
            let input_count = if columns_len > 0 {
                columns_len
            } else {
                plugin
                    .config
                    .get("input_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize
            };
            let mut ports = Vec::new();
            ports.push("in".to_string());
            ports.extend((0..input_count).map(|idx| format!("in_{idx}")));
            return ports;
        }
        self.ports_for_kind(&plugin.kind, inputs)
    }

    fn plugin_display_name(&self, plugin_id: u64) -> String {
        let name_by_kind: HashMap<String, String> = self
            .plugin_manager
            .installed_plugins
            .iter()
            .map(|plugin| (plugin.manifest.kind.clone(), plugin.manifest.name.clone()))
            .collect();
        self.workspace_manager
            .workspace
            .plugins
            .iter()
            .find(|plugin| plugin.id == plugin_id)
            .map(|plugin| {
                name_by_kind
                    .get(&plugin.kind)
                    .cloned()
                    .unwrap_or_else(|| Self::display_kind(&plugin.kind))
            })
            .unwrap_or_else(|| "plugin".to_string())
    }

    fn default_csv_path() -> String {
        let base = std::env::var("HOME")
            .map(|home| PathBuf::from(home).join("rtsyn-recorded"))
            .unwrap_or_else(|_| PathBuf::from("rtsyn-recorded"));
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let day = now / 86_400;
        let hour = (now % 86_400) / 3_600;
        let minute = (now % 3_600) / 60;
        let second = now % 60;
        let stamp = format!("{day}-{hour:02}-{minute:02}-{second:02}");
        base.join(format!("{stamp}.csv"))
            .to_string_lossy()
            .to_string()
    }

    fn plotter_config_from_value(&self, config: &Value) -> (usize, f64) {
        let input_count = config
            .get("input_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let refresh_hz = config
            .get("refresh_hz")
            .and_then(|v| v.as_f64())
            .unwrap_or(60.0);
        (input_count, refresh_hz)
    }

    fn plotter_series_names(&self, plotter_id: u64, input_count: usize) -> Vec<String> {
        let mut names = Vec::with_capacity(input_count);
        for idx in 0..input_count {
            let port = format!("in_{idx}");
            if let Some(conn) = self
                .workspace_manager
                .workspace
                .connections
                .iter()
                .find(|conn| conn.to_plugin == plotter_id && conn.to_port == port)
            {
                let source_name = self.plugin_display_name(conn.from_plugin);
                names.push(format!("{source_name}:{}", conn.from_port));
            } else {
                names.push(port);
            }
        }
        names
    }

    fn plotter_input_values(
        &self,
        plotter_id: u64,
        input_count: usize,
        outputs: &HashMap<(u64, String), f64>,
    ) -> Vec<f64> {
        let mut values = Vec::with_capacity(input_count);
        for idx in 0..input_count {
            let port = format!("in_{idx}");
            let value = if idx == 0 {
                let ports = vec![port.clone(), "in".to_string()];
                input_sum_any(
                    &self.workspace_manager.workspace.connections,
                    outputs,
                    plotter_id,
                    &ports,
                )
            } else {
                input_sum(
                    &self.workspace_manager.workspace.connections,
                    outputs,
                    plotter_id,
                    &port,
                )
            };
            values.push(value);
        }
        values
    }

    fn update_plotters(
        &mut self,
        tick: u64,
        outputs: &HashMap<(u64, String), f64>,
        samples: &HashMap<u64, Vec<(u64, Vec<f64>)>>,
    ) {
        let mut max_refresh = 1.0;
        let time_s = tick as f64 * self.state_sync.logic_period_seconds.max(0.0);
        let mut live_plotter_ids: HashSet<u64> = HashSet::new();

        for plugin in &self.workspace_manager.workspace.plugins {
            if plugin.kind != "live_plotter" {
                continue;
            }
            live_plotter_ids.insert(plugin.id);
            let (input_count, refresh_hz) = self.plotter_config_from_value(&plugin.config);
            let window_ms = self
                .plotter_manager
                .plotter_preview_settings
                .get(&plugin.id)
                .map(|(_, _, _, _, _, _, _, _, _, _, _, window_ms, _, _)| *window_ms)
                .unwrap_or(10_000.0);
            let series_names = self.plotter_series_names(plugin.id, input_count);
            let is_open = self
                .plotter_manager
                .plotters
                .get(&plugin.id)
                .and_then(|plotter| plotter.lock().ok().map(|plotter| plotter.open))
                .unwrap_or(false);
            let values = if is_open {
                self.plotter_input_values(plugin.id, input_count, outputs)
            } else {
                Vec::new()
            };
            let plotter = self
                .plotter_manager
                .plotters
                .entry(plugin.id)
                .or_insert_with(|| Arc::new(Mutex::new(LivePlotter::new(plugin.id))));
            if let Ok(mut plotter) = plotter.lock() {
                plotter.update_config(
                    input_count,
                    refresh_hz,
                    self.state_sync.logic_period_seconds,
                );
                plotter.set_window_ms(window_ms);
                plotter.set_series_names(series_names);
                if plotter.open && plugin.running {
                    if let Some(samples) = samples.get(&plugin.id) {
                        for (sample_tick, values) in samples {
                            let sample_time_s =
                                *sample_tick as f64 * self.state_sync.logic_period_seconds.max(0.0);
                            plotter.push_sample(
                                *sample_tick,
                                sample_time_s,
                                self.state_sync.logic_time_scale,
                                values,
                            );
                        }
                    } else {
                        plotter.push_sample(
                            tick,
                            time_s,
                            self.state_sync.logic_time_scale,
                            &values,
                        );
                    }
                    if refresh_hz > max_refresh {
                        max_refresh = refresh_hz;
                    }
                }
            }
        }

        self.plotter_manager
            .plotters
            .retain(|plugin_id, _| live_plotter_ids.contains(plugin_id));
        self.refresh_logic_ui_hz(max_refresh);
    }

    fn refresh_logic_ui_hz(&mut self, max_refresh: f64) {
        let target_hz = if max_refresh > 0.0 { max_refresh } else { 1.0 };
        if (self.state_sync.logic_ui_hz - target_hz).abs() > f64::EPSILON {
            self.state_sync.logic_ui_hz = target_hz;
            self.send_logic_settings();
        }
    }

    fn recompute_plotter_ui_hz(&mut self) {
        let mut max_refresh = 1.0;
        for plotter in self.plotter_manager.plotters.values() {
            if let Ok(plotter) = plotter.lock() {
                if plotter.open && plotter.refresh_hz > max_refresh {
                    max_refresh = plotter.refresh_hz;
                }
            }
        }
        self.refresh_logic_ui_hz(max_refresh);
    }

    fn display_connection_kind(kind: &str) -> &str {
        match kind {
            "shared_memory" => "Shared memory",
            "pipe" => "Pipe",
            "in_process" => "In process",
            other => other,
        }
    }

    fn time_settings_from_selection(
        tab: WorkspaceTimingTab,
        frequency_unit: FrequencyUnit,
        period_unit: PeriodUnit,
    ) -> (TimeUnit, f64, String) {
        let unit = match tab {
            WorkspaceTimingTab::Period => match period_unit {
                PeriodUnit::Ns => TimeUnit::Ns,
                PeriodUnit::Us => TimeUnit::Us,
                PeriodUnit::Ms => TimeUnit::Ms,
                PeriodUnit::S => TimeUnit::S,
            },
            WorkspaceTimingTab::Frequency => match frequency_unit {
                FrequencyUnit::Hz => TimeUnit::S,
                FrequencyUnit::KHz => TimeUnit::Ms,
                FrequencyUnit::MHz => TimeUnit::Us,
            },
        };
        let (scale, label) = match unit {
            TimeUnit::Ns => (1e9, "time_ns"),
            TimeUnit::Us => (1e6, "time_us"),
            TimeUnit::Ms => (1e3, "time_ms"),
            TimeUnit::S => (1.0, "time_s"),
        };
        (unit, scale, label.to_string())
    }

    fn compute_period_seconds(&self) -> f64 {
        self.period_seconds_from_fields()
    }

    fn period_seconds_from_fields(&self) -> f64 {
        match self.period_unit {
            PeriodUnit::Ns => self.period_value * 1e-9,
            PeriodUnit::Us => self.period_value * 1e-6,
            PeriodUnit::Ms => self.period_value * 1e-3,
            PeriodUnit::S => self.period_value,
        }
    }

    fn send_logic_settings(&mut self) {
        let period_seconds = self.compute_period_seconds();
        let (_unit, time_scale, time_label) = Self::time_settings_from_selection(
            self.workspace_settings.tab,
            self.frequency_unit,
            self.period_unit,
        );
        let cores: Vec<usize> = self
            .selected_cores
            .iter()
            .enumerate()
            .filter_map(|(idx, enabled)| if *enabled { Some(idx) } else { None })
            .collect();
        self.state_sync.logic_period_seconds = period_seconds;
        self.state_sync.logic_time_scale = time_scale;
        self.state_sync.logic_time_label = time_label.clone();
        let _ = self
            .state_sync
            .logic_tx
            .send(LogicMessage::UpdateSettings(LogicSettings {
                cores,
                period_seconds,
                time_scale,
                time_label,
                ui_hz: self.state_sync.logic_ui_hz,
                max_integration_steps: 10, // Default reasonable limit for real-time performance
            }));
    }

    fn current_workspace_settings(&self) -> WorkspaceSettings {
        let frequency_unit = match self.frequency_unit {
            FrequencyUnit::Hz => "hz",
            FrequencyUnit::KHz => "khz",
            FrequencyUnit::MHz => "mhz",
        };
        let period_unit = match self.period_unit {
            PeriodUnit::Ns => "ns",
            PeriodUnit::Us => "us",
            PeriodUnit::Ms => "ms",
            PeriodUnit::S => "s",
        };
        let selected_cores: Vec<usize> = self
            .selected_cores
            .iter()
            .enumerate()
            .filter_map(|(idx, enabled)| if *enabled { Some(idx) } else { None })
            .collect();
        WorkspaceSettings {
            frequency_value: self.frequency_value,
            frequency_unit: frequency_unit.to_string(),
            period_value: self.period_value,
            period_unit: period_unit.to_string(),
            selected_cores,
        }
    }

    fn apply_workspace_settings(&mut self) {
        let settings = self.workspace_manager.workspace.settings.clone();
        self.workspace_settings.tab = WorkspaceTimingTab::Frequency;
        self.frequency_value = settings.frequency_value;
        self.frequency_unit = match settings.frequency_unit.as_str() {
            "khz" => FrequencyUnit::KHz,
            "mhz" => FrequencyUnit::MHz,
            _ => FrequencyUnit::Hz,
        };
        self.period_value = settings.period_value;
        self.period_unit = match settings.period_unit.as_str() {
            "ns" => PeriodUnit::Ns,
            "us" => PeriodUnit::Us,
            "s" => PeriodUnit::S,
            _ => PeriodUnit::Ms,
        };
        self.selected_cores = (0..self.available_cores)
            .map(|idx| settings.selected_cores.contains(&idx))
            .collect();
        if !self.selected_cores.iter().any(|v| *v) && self.available_cores > 0 {
            self.selected_cores[0] = true;
        }

        self.send_logic_settings();
    }

    fn apply_loads_started_on_load(&mut self) {
        let plugin_infos: Vec<(u64, String, Option<std::path::PathBuf>)> = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .map(|plugin| {
                let library_path = plugin
                    .config
                    .get("library_path")
                    .and_then(|v| v.as_str())
                    .map(|v| std::path::PathBuf::from(v));
                (plugin.id, plugin.kind.clone(), library_path)
            })
            .collect();

        for (plugin_id, kind, library_path) in &plugin_infos {
            self.ensure_plugin_behavior_cached_with_path(kind, library_path.as_ref());
            let loads_started = self
                .plugin_manager
                .plugin_behaviors
                .get(kind)
                .map(|b| b.loads_started)
                .unwrap_or(false);
            if let Some(plugin) = self
                .workspace_manager
                .workspace
                .plugins
                .iter_mut()
                .find(|p| p.id == *plugin_id)
            {
                plugin.running = loads_started;
            }
        }
    }

    fn open_running_plotters(&mut self) {
        let mut recompute = false;
        for plugin in &self.workspace_manager.workspace.plugins {
            if plugin.kind != "live_plotter" || !plugin.running {
                continue;
            }
            let plotter = self
                .plotter_manager
                .plotters
                .entry(plugin.id)
                .or_insert_with(|| Arc::new(Mutex::new(LivePlotter::new(plugin.id))));
            if let Ok(mut plotter) = plotter.lock() {
                if !plotter.open {
                    plotter.open = true;
                    recompute = true;
                }
            }
        }
        if recompute {
            self.recompute_plotter_ui_hz();
        }
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.style_mut(|style| {
            style.interaction.selectable_labels = false;
        });
        self.poll_build_dialog();
        self.poll_install_dialog();
        self.poll_import_dialog();
        self.poll_load_dialog();
        self.poll_export_dialog();
        self.poll_csv_path_dialog();
        self.poll_plugin_creator_dialog();
        self.poll_plotter_screenshot_dialog();
        self.poll_logic_state();
        let mut plotter_refresh = 0.0;
        for plotter in self.plotter_manager.plotters.values() {
            if let Ok(plotter) = plotter.lock() {
                if plotter.open && plotter.refresh_hz > plotter_refresh {
                    plotter_refresh = plotter.refresh_hz;
                }
            }
        }
        if plotter_refresh > 0.0 {
            let hz = plotter_refresh.max(1.0);
            ctx.request_repaint_after(Duration::from_secs_f64(1.0 / hz));
        } else if !ctx.input(|i| i.focused) {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        if self.workspace_manager.workspace_dirty {
            let _ = self.state_sync.logic_tx.send(LogicMessage::UpdateWorkspace(
                self.workspace_manager.workspace.clone(),
            ));
            self.workspace_manager.workspace_dirty = false;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.build_dialog.open && !self.build_dialog.in_progress {
                self.build_dialog.open = false;
            } else if self.confirm_dialog.open {
                self.confirm_dialog.open = false;
                self.confirm_dialog.action = None;
            }
        }
        self.window_rects.clear();

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.scope(|ui| {
                let mut style = ui.style().as_ref().clone();
                style.spacing.button_padding = egui::vec2(10.0, 6.0);
                style
                    .text_styles
                    .insert(egui::TextStyle::Button, egui::FontId::proportional(15.0));
                ui.set_style(style);
                egui::menu::bar(ui, |ui| {
                    ui.menu_button("Workspace", |ui| {
                        let label = if self.workspace_manager.workspace_path.as_os_str().is_empty()
                        {
                            "No Workspace loaded".to_string()
                        } else {
                            self.workspace_manager.workspace.name.clone()
                        };
                        ui.add_enabled(
                            false,
                            egui::Label::new(
                                RichText::new(label)
                                    .color(egui::Color32::from_gray(230))
                                    .size(15.0),
                            ),
                        );
                        ui.separator();
                        if ui.button("New Workspace").clicked() {
                            self.open_workspace_dialog(WorkspaceDialogMode::New);
                            ui.close_menu();
                        }
                        if ui.button("Load Workspace").clicked() {
                            self.open_load_workspaces();
                            ui.close_menu();
                        }
                        if ui.button("Save Workspace").clicked() {
                            self.save_workspace_overwrite_current();
                            ui.close_menu();
                        }
                        let has_workspace =
                            !self.workspace_manager.workspace_path.as_os_str().is_empty();
                        if ui
                            .add_enabled(has_workspace, egui::Button::new("Export Workspace"))
                            .clicked()
                        {
                            self.export_workspace_path(
                                &self.workspace_manager.workspace_path.clone(),
                            );
                            ui.close_menu();
                        }
                        if ui
                            .add_enabled(has_workspace, egui::Button::new("Delete Workspace"))
                            .clicked()
                        {
                            self.show_confirm(
                                "Delete workspace",
                                "Delete current workspace?",
                                "Delete",
                                ConfirmAction::DeleteWorkspace(
                                    self.workspace_manager.workspace_path.clone(),
                                ),
                            );
                            ui.close_menu();
                        }
                        if ui.button("Manage Workspaces").clicked() {
                            self.open_manage_workspaces();
                            ui.close_menu();
                        }
                    });

                    ui.menu_button("Plugins", |ui| {
                        if ui.button("Add plugins").clicked() {
                            self.open_plugins();
                            ui.close_menu();
                        }
                        if ui.button("New plugin").clicked() {
                            self.open_new_plugin_window();
                            ui.close_menu();
                        }
                        if ui.button("Install plugin").clicked() {
                            self.open_install_plugins();
                            ui.close_menu();
                        }
                        if ui.button("Uninstall plugin").clicked() {
                            self.open_uninstall_plugins();
                            ui.close_menu();
                        }
                        if ui.button("Manage plugins").clicked() {
                            self.open_manage_plugins();
                            ui.close_menu();
                        }
                    });

                    ui.menu_button("Connections", |ui| {
                        ui.set_width(220.0);
                        let icon = if self.connections_view_enabled {
                            "\u{f070}"
                        } else {
                            "\u{f06e}"
                        };
                        if ui
                            .button(format!("Toggle connections view {icon}"))
                            .clicked()
                        {
                            self.connections_view_enabled = !self.connections_view_enabled;
                            ui.close_menu();
                        }
                        if ui.button("Manage connections").clicked() {
                            self.windows.manage_connections_open = true;
                            self.pending_window_focus = Some(WindowFocus::ManageConnections);
                            ui.close_menu();
                        }
                    });

                    ui.menu_button("Runtime", |ui| {
                        if ui.button("UML diagram").clicked() {
                            self.windows.uml_diagram_open = true;
                            self.pending_window_focus = Some(WindowFocus::UmlDiagram);
                            self.uml_text_buffer =
                                self.workspace_manager.current_workspace_uml_diagram();
                            self.uml_preview_hash = None;
                            self.uml_preview_error = None;
                            self.uml_preview_texture = None;
                            self.uml_preview_loading = false;
                            self.uml_preview_rx = None;
                            self.uml_export_svg = false;
                            self.uml_export_width = 1920;
                            self.uml_export_height = 1080;
                            self.uml_preview_zoom = 0.0;
                            ui.close_menu();
                        }
                        if ui.button("Settings").clicked() {
                            self.workspace_settings.open = true;
                            self.pending_window_focus = Some(WindowFocus::WorkspaceSettings);
                            ui.close_menu();
                        }
                    });
                    if ui.button("Help").clicked() {
                        self.help_state.open = true;
                        self.pending_window_focus = Some(WindowFocus::Help);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(format!("RTSyn {}", env!("CARGO_PKG_VERSION"))).weak(),
                        );
                    });
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);
            let panel_rect = ui.max_rect();
            self.render_connection_view(ctx, panel_rect);
            self.render_plugin_cards(ctx, panel_rect);
            if ctx.input(|i| i.pointer.primary_clicked()) {
                if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                    let over_plugin = self.plugin_rects.values().any(|rect| rect.contains(pos));
                    if !over_plugin {
                        self.selected_plugin_id = None;
                    }
                }
            }
        });

        self.render_workspace_dialog(ctx);
        self.render_load_workspaces_window(ctx);
        self.render_manage_workspaces_window(ctx);
        self.render_manage_plugins_window(ctx);
        self.render_install_plugins_window(ctx);
        self.render_uninstall_plugins_window(ctx);
        self.render_plugins_window(ctx);
        self.render_new_plugin_window(ctx);
        self.render_manage_connections_window(ctx);
        self.render_connection_editor(ctx);
        self.render_plugin_context_menu(ctx);
        self.render_connection_context_menu(ctx);
        self.render_plugin_config_window(ctx);
        self.render_plotter_windows(ctx);
        self.render_workspace_settings_window(ctx);
        self.render_uml_diagram_window(ctx);
        self.render_help_window(ctx);
        self.render_build_dialog(ctx);
        self.render_confirm_remove_dialog(ctx);
        self.render_info_dialog(ctx);
        self.render_plotter_preview_dialog(ctx);
    }
}
