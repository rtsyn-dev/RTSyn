use rtsyn_runtime::runtime::{LogicMessage, LogicSettings, LogicState};
use rtsyn_runtime::spawn_runtime;
use eframe::{egui, egui::RichText};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
    let mut cmd = Command::new("zenity");
    cmd.arg("--file-selection");
    
    match mode {
        "save" => { cmd.arg("--save"); }
        "folder" => { cmd.arg("--directory"); }
        _ => {} // open file is default
    }
    
    if let Some(f) = filter {
        cmd.arg("--file-filter").arg(f);
    }
    
    cmd.output().ok()
        .and_then(|output| {
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
use workspace::{
    add_connection as workspace_add_connection, input_sum, input_sum_any, remove_extendable_input,
    ConnectionDefinition, ConnectionRuleError, PluginDefinition, WorkspaceDefinition,
    WorkspaceSettings,
};

mod generic_renderer;
mod notifications;
mod plotter;
mod state;
mod ui;
mod utils;
mod workspace_utils;

use notifications::Notification;
use plotter::LivePlotter;
use workspace_utils::{scan_workspace_entries, workspace_file_path};
use state::{
    PluginTab, WorkspaceTimingTab, ConfirmAction, ConnectionEditMode, ConnectionEditTab,
    DetectedPlugin, FrequencyUnit, InstalledPlugin, ManageTab, PeriodUnit, PluginManifest,
    TimeUnit, WorkspaceDialogMode, WorkspaceEntry,
};

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
    Plugins,
    WorkspaceSettings,
    ManageConnections,
    ConnectionEditorAdd,
    ConnectionEditorRemove,
    PluginConfig,
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
    workspace: WorkspaceDefinition,
    workspace_path: PathBuf,
    status: String,
    installed_plugins: Vec<InstalledPlugin>,
    plugin_behaviors: HashMap<String, rtsyn_plugin::ui::PluginBehavior>,
    install_dialog_rx: Option<Receiver<Option<PathBuf>>>,
    import_dialog_rx: Option<Receiver<Option<PathBuf>>>,
    load_dialog_rx: Option<Receiver<Option<PathBuf>>>,
    export_dialog_rx: Option<Receiver<(PathBuf, Option<PathBuf>)>>,
    csv_path_dialog_rx: Option<Receiver<Option<PathBuf>>>,
    csv_path_target_plugin_id: Option<u64>,
    install_db_path: PathBuf,
    workspace_dir: PathBuf,
    workspace_dialog_open: bool,
    workspace_dialog_mode: WorkspaceDialogMode,
    workspace_name_input: String,
    workspace_description_input: String,
    workspace_edit_path: Option<PathBuf>,
    notifications: Vec<Notification>,
    build_dialog_open: bool,
    build_dialog_in_progress: bool,
    build_dialog_message: String,
    build_dialog_title: String,
    build_dialog_rx: Option<Receiver<BuildResult>>,
    confirm_dialog_open: bool,
    confirm_dialog_title: String,
    confirm_dialog_message: String,
    confirm_dialog_action_label: String,
    confirm_action: Option<ConfirmAction>,
    plugin_positions: HashMap<u64, egui::Pos2>,
    plugin_rects: HashMap<u64, egui::Rect>,
    connections_view_enabled: bool,
    manage_connections_open: bool,
    workspace_settings_open: bool,
    workspace_settings_draft: Option<WorkspaceSettingsDraft>,
    workspace_settings_tab: WorkspaceTimingTab,
    available_cores: usize,
    selected_cores: Vec<bool>,
    frequency_value: f64,
    frequency_unit: FrequencyUnit,
    period_value: f64,
    period_unit: PeriodUnit,
    logic_tx: Sender<LogicMessage>,
    logic_state_rx: Receiver<LogicState>,
    workspace_dirty: bool,
    manage_workspace_open: bool,
    load_workspace_open: bool,
    workspace_entries: Vec<WorkspaceEntry>,
    manage_workspace_selected_index: Option<usize>,
    load_workspace_selected_index: Option<usize>,
    manage_plugins_open: bool,
    manage_plugins_tab: ManageTab,
    install_search: String,
    detected_plugins: Vec<DetectedPlugin>,
    manage_selected_index: Option<usize>,
    plugins_open: bool,
    plugin_tab: PluginTab,
    plugin_search: String,
    plugin_selected_index: Option<usize>,
    organize_search: String,
    organize_selected_index: Option<usize>,
    computed_outputs: HashMap<(u64, String), f64>,
    input_values: HashMap<(u64, String), f64>,
    internal_variable_values: HashMap<(u64, String), serde_json::Value>,
    viewer_values: HashMap<u64, f64>,
    last_output_update: Instant,
    plotters: HashMap<u64, Arc<Mutex<LivePlotter>>>,
    logic_period_seconds: f64,
    logic_time_scale: f64,
    logic_time_label: String,
    logic_ui_hz: f64,
    output_refresh_hz: f64,
    plotter_screenshot_rx: Option<Receiver<Option<PathBuf>>>,
    plotter_screenshot_target: Option<u64>,
    plotter_preview_open: bool,
    plotter_preview_target: Option<u64>,
    plotter_preview_show_axes: bool,
    plotter_preview_show_legend: bool,
    plotter_preview_show_grid: bool,
    plotter_preview_series_names: Vec<String>,
    plotter_preview_colors: Vec<egui::Color32>,
    plotter_preview_title: String,
    plotter_preview_dark_theme: bool,
    plotter_preview_x_axis_name: String,
    plotter_preview_y_axis_name: String,
    plotter_preview_high_quality: bool,
    plotter_preview_export_svg: bool,
    plotter_preview_width: u32,
    plotter_preview_height: u32,
    plotter_preview_settings: HashMap<u64, (bool, bool, bool, Vec<String>, Vec<egui::Color32>, String, bool, String, String, bool, bool)>,
    next_plugin_id: u64,
    connection_from_idx: usize,
    connection_to_idx: usize,
    connection_from_port: String,
    connection_to_port: String,
    connection_kind: String,
    connection_kind_options: Vec<String>,
    connection_edit_open: bool,
    connection_edit_mode: ConnectionEditMode,
    connection_edit_tab: ConnectionEditTab,
    connection_edit_plugin_id: Option<u64>,
    connection_edit_selected_idx: Option<usize>,
    connection_edit_from_port_idx: usize,
    connection_edit_to_port_idx: usize,
    connection_edit_last_selected: Option<u64>,
    connection_edit_last_tab: Option<ConnectionEditTab>,
    connection_highlight_plugin_id: Option<u64>,
    selected_plugin_id: Option<u64>,
    plugin_context_menu: Option<(u64, egui::Pos2, u64)>,
    connection_context_menu: Option<(Vec<ConnectionDefinition>, egui::Pos2, u64)>,
    plugin_config_open: bool,
    plugin_config_id: Option<u64>,
    number_edit_buffers: HashMap<(u64, String), String>,
    window_rects: Vec<egui::Rect>,
    pending_window_focus: Option<WindowFocus>,
}

impl GuiApp {
    fn new_with_runtime(
        logic_tx: Sender<LogicMessage>,
        logic_state_rx: Receiver<LogicState>,
    ) -> Self {
        let install_db_path = PathBuf::from("app_plugins").join("installed_plugins.json");
        let workspace_dir = PathBuf::from("app_workspaces");
        let workspace = WorkspaceDefinition {
            name: "default".to_string(),
            description: String::new(),
            target_hz: 1000,
            plugins: Vec::new(),
            connections: Vec::new(),
            settings: WorkspaceSettings::default(),
        };

        let available_cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);

        let mut app = Self {
            workspace,
            workspace_path: PathBuf::new(),
            status: String::new(),
            installed_plugins: Vec::new(),
            plugin_behaviors: HashMap::new(),
            install_dialog_rx: None,
            import_dialog_rx: None,
            load_dialog_rx: None,
            export_dialog_rx: None,
            csv_path_dialog_rx: None,
            csv_path_target_plugin_id: None,
            install_db_path,
            workspace_dir,
            workspace_dialog_open: false,
            workspace_dialog_mode: WorkspaceDialogMode::New,
            workspace_name_input: String::new(),
            workspace_description_input: String::new(),
            workspace_edit_path: None,
            notifications: Vec::new(),
            build_dialog_open: false,
            build_dialog_in_progress: false,
            build_dialog_message: String::new(),
            build_dialog_title: String::new(),
            build_dialog_rx: None,
            confirm_dialog_open: false,
            confirm_dialog_title: String::new(),
            confirm_dialog_message: String::new(),
            confirm_dialog_action_label: "Remove".to_string(),
            confirm_action: None,
            plugin_positions: HashMap::new(),
            plugin_rects: HashMap::new(),
            connections_view_enabled: true,
            manage_connections_open: false,
            workspace_settings_open: false,
            workspace_settings_draft: None,
            workspace_settings_tab: WorkspaceTimingTab::Period,
            available_cores,
            selected_cores: (0..available_cores).map(|i| i == 0).collect(),
            frequency_value: 1000.0,
            frequency_unit: FrequencyUnit::Hz,
            period_value: 1.0,
            period_unit: PeriodUnit::Ms,
            logic_tx,
            logic_state_rx,
            workspace_dirty: true,
            manage_workspace_open: false,
            load_workspace_open: false,
            workspace_entries: Vec::new(),
            manage_workspace_selected_index: None,
            load_workspace_selected_index: None,
            manage_plugins_open: false,
            manage_plugins_tab: ManageTab::default(),
            install_search: String::new(),
            detected_plugins: Vec::new(),
            manage_selected_index: None,
            plugins_open: false,
            plugin_tab: PluginTab::default(),
            plugin_search: String::new(),
            plugin_selected_index: None,
            organize_search: String::new(),
            organize_selected_index: None,
            computed_outputs: HashMap::new(),
            input_values: HashMap::new(),
            internal_variable_values: HashMap::new(),
            viewer_values: HashMap::new(),
            last_output_update: Instant::now(),
            plotters: HashMap::new(),
            logic_period_seconds: 0.001,
            logic_time_scale: 1000.0,
            logic_time_label: "time_ms".to_string(),
            logic_ui_hz: 60.0,
            output_refresh_hz: 1.0,
            plotter_screenshot_rx: None,
            plotter_screenshot_target: None,
            plotter_preview_open: false,
            plotter_preview_target: None,
            plotter_preview_show_axes: true,
            plotter_preview_show_legend: true,
            plotter_preview_show_grid: true,
            plotter_preview_series_names: Vec::new(),
            plotter_preview_colors: Vec::new(),
            plotter_preview_title: String::new(),
            plotter_preview_dark_theme: true,
            plotter_preview_x_axis_name: String::new(),
            plotter_preview_y_axis_name: String::new(),
            plotter_preview_high_quality: false,
            plotter_preview_export_svg: false,
            plotter_preview_width: 1200,
            plotter_preview_height: 700,
            plotter_preview_settings: HashMap::new(),
            next_plugin_id: 1,
            connection_from_idx: 0,
            connection_to_idx: 0,
            connection_from_port: "out".to_string(),
            connection_to_port: "in".to_string(),
            connection_kind: "shared_memory".to_string(),
            connection_kind_options: vec![
                "shared_memory".to_string(),
                "pipe".to_string(),
                "in_process".to_string(),
            ],
            connection_edit_open: false,
            connection_edit_mode: ConnectionEditMode::Add,
            connection_edit_tab: ConnectionEditTab::Outputs,
            connection_edit_plugin_id: None,
            connection_edit_selected_idx: None,
            connection_edit_from_port_idx: 0,
            connection_edit_to_port_idx: 0,
            connection_edit_last_selected: None,
            connection_edit_last_tab: None,
            connection_highlight_plugin_id: None,
            selected_plugin_id: None,
            plugin_context_menu: None,
            connection_context_menu: None,
            plugin_config_open: false,
            plugin_config_id: None,
            number_edit_buffers: HashMap::new(),
            window_rects: Vec::new(),
            pending_window_focus: None,
        };

        app.load_installed_plugins();
        app.refresh_installed_library_paths();
        app.inject_library_paths_into_workspace();
        app
    }

    fn center_window(ctx: &egui::Context, size: egui::Vec2) -> egui::Pos2 {
        let rect = ctx.available_rect();
        let center = rect.center();
        center - size * 0.5
    }

    fn sync_next_plugin_id(&mut self) {
        if let Some(max_id) = self.workspace.plugins.iter().map(|p| p.id).max() {
            self.next_plugin_id = max_id + 1;
        } else {
            self.next_plugin_id = 1;
        }
    }

    fn mark_workspace_dirty(&mut self) {
        self.workspace_dirty = true;
    }

    fn start_plugin_build(&mut self, action: BuildAction, label: String) {
        if self.build_dialog_rx.is_some() {
            self.status = "Plugin build already running".to_string();
            return;
        }
        let path = match &action {
            BuildAction::Install { path, .. } => path.clone(),
            BuildAction::Reinstall { path, .. } => path.clone(),
        };
        if !path.join("Cargo.toml").is_file() {
            match action {
                BuildAction::Install {
                    path,
                    removable,
                    persist,
                } => {
                    self.install_plugin_from_folder(path, removable, persist);
                    self.scan_detected_plugins();
                    return;
                }
                BuildAction::Reinstall { .. } => {
                    let (tx, rx) = mpsc::channel();
                    self.build_dialog_rx = Some(rx);
                    self.build_dialog_open = true;
                    self.build_dialog_in_progress = true;
                    self.build_dialog_title = "Building plugin".to_string();
                    self.build_dialog_message = format!("Building {label}...");
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(150));
                        let _ = tx.send(BuildResult {
                            success: true,
                            action,
                        });
                    });
                    return;
                }
            }
        }
        let (tx, rx) = mpsc::channel();
        self.build_dialog_rx = Some(rx);
        self.build_dialog_open = true;
        self.build_dialog_in_progress = true;
        self.build_dialog_title = "Building plugin".to_string();
        self.build_dialog_message = format!("Building {label}...");
        std::thread::spawn(move || {
            let success = GuiApp::build_plugin(&path);
            let _ = tx.send(BuildResult { success, action });
        });
    }

    fn poll_build_dialog(&mut self) {
        let result = match &self.build_dialog_rx {
            Some(rx) => rx.try_recv().ok(),
            None => None,
        };
        if let Some(result) = result {
            self.build_dialog_rx = None;
            self.build_dialog_in_progress = false;
            if result.success {
                match result.action {
                    BuildAction::Install {
                        path,
                        removable,
                        persist,
                    } => {
                        let prev_count = self.installed_plugins.len();
                        self.install_plugin_from_folder(path, removable, persist);
                        let was_installed = self.installed_plugins.len() > prev_count;
                        if was_installed {
                            self.show_info("Plugin", "Plugin built and installed");
                        } else {
                            let msg = self.status.clone();
                            self.show_info("Plugin", &msg);
                        }
                        self.scan_detected_plugins();
                    }
                    BuildAction::Reinstall { kind, path } => {
                        self.refresh_installed_plugin(kind, &path);
                        self.scan_detected_plugins();
                        self.status = "Plugin rebuilt".to_string();
                        self.show_info("Plugin", "Plugin rebuilt");
                    }
                }
            } else {
                self.status = "Plugin build failed".to_string();
                self.show_info("Plugin", "Plugin build failed");
            }
            self.build_dialog_open = false;
        }
    }

    fn install_plugin_from_folder<P: AsRef<Path>>(
        &mut self,
        folder: P,
        removable: bool,
        persist: bool,
    ) {
        let manifest_path = folder.as_ref().join("plugin.toml");
        let data = match fs::read_to_string(&manifest_path) {
            Ok(content) => content,
            Err(err) => {
                self.status = format!("Failed to read plugin.toml: {err}");
                return;
            }
        };

        let manifest: PluginManifest = match toml::from_str(&data) {
            Ok(parsed) => parsed,
            Err(err) => {
                self.status = format!("Invalid plugin.toml: {err}");
                return;
            }
        };
        if manifest.kind == "comedi_daq" && !cfg!(feature = "comedi") {
            return;
        }

        let library_path = Self::resolve_library_path(&manifest, folder.as_ref());
        let (mut metadata_inputs, mut metadata_outputs, mut metadata_variables, mut display_schema) = if let Some(ref lib_path) = library_path {
            let (tx, rx) = std::sync::mpsc::channel();
            let _ = self.logic_tx.send(LogicMessage::QueryPluginMetadata(lib_path.to_string_lossy().to_string(), tx));
            if let Ok(Some((inputs, outputs, vars, display_schema))) = rx.recv() {
                (inputs, outputs, vars, display_schema)
            } else {
                (vec![], vec![], vec![], None)
            }
        } else {
            (vec![], vec![], vec![], None)
        };
        if manifest.kind == "performance_monitor" {
            metadata_inputs = Vec::new();
            metadata_outputs = vec![
                "period_us".to_string(),
                "latency_us".to_string(),
                "jitter_us".to_string(),
                "realtime_violation".to_string(),
            ];
            metadata_variables = vec![("max_latency_us".to_string(), 1000.0)];
            display_schema = Some(rtsyn_plugin::ui::DisplaySchema {
                outputs: metadata_outputs.clone(),
                inputs: Vec::new(),
                variables: Vec::new(),
            });
        } else if matches!(manifest.kind.as_str(), "csv_recorder" | "live_plotter") {
            display_schema = Some(rtsyn_plugin::ui::DisplaySchema {
                outputs: Vec::new(),
                inputs: Vec::new(),
                variables: vec!["input_count".to_string(), "running".to_string()],
            });
        }
        
        // Check if plugin of this kind is already installed
        if self.installed_plugins.iter().any(|p| p.manifest.kind == manifest.kind) {
            self.status = format!("Plugin '{}' is already installed", manifest.kind);
            return;
        }
        
        self.installed_plugins.push(InstalledPlugin {
            manifest,
            path: folder.as_ref().to_path_buf(),
            library_path,
            removable,
            metadata_inputs,
            metadata_outputs,
            metadata_variables,
            display_schema,
        });
        self.status = "Plugin installed".to_string();
        if persist {
            self.persist_installed_plugins();
        }
    }

    fn resolve_library_path(manifest: &PluginManifest, folder: &Path) -> Option<PathBuf> {
        if let Some(library) = manifest.library.as_ref() {
            let direct = folder.join(library);
            if direct.is_file() {
                return Some(direct);
            }
            let cwd_direct = PathBuf::from(library);
            if cwd_direct.is_file() {
                return Some(cwd_direct);
            }
        }
        let kind = &manifest.kind;
        let candidates = [
            format!("lib{kind}.so"),
            format!("lib{kind}.dylib"),
            format!("{kind}.dll"),
        ];
        let search_dirs = [
            folder.to_path_buf(),
            folder.join("target").join("release"),
            folder.join("target").join("debug"),
            PathBuf::from("target").join("release"),
            PathBuf::from("target").join("debug"),
        ];
        if let Some(library) = manifest.library.as_ref() {
            for dir in &search_dirs {
                let path = dir.join(library);
                if path.is_file() {
                    return Some(path);
                }
            }
        }
        for dir in search_dirs {
            for candidate in &candidates {
                let path = dir.join(candidate);
                if path.is_file() {
                    return Some(path);
                }
            }
        }
        None
    }

    fn workspace_root() -> Option<PathBuf> {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(|path| path.to_path_buf())
    }

    fn build_plugin(folder: &Path) -> bool {
        let cargo_toml = folder.join("Cargo.toml");
        if !cargo_toml.is_file() {
            return false;
        }
        let local_ok = Command::new("cargo")
            .arg("build")
            .arg("--release")
            .current_dir(folder)
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if local_ok {
            return true;
        }
        let Some(workspace_root) = Self::workspace_root() else {
            return false;
        };
        if !folder.starts_with(&workspace_root) {
            return false;
        }
        let workspace_manifest = workspace_root.join("Cargo.toml");
        let mut cmd = Command::new("cargo");
        cmd.arg("build")
            .arg("--release")
            .arg("--manifest-path")
            .arg(workspace_manifest)
            .current_dir(workspace_root);
        cmd.status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    fn refresh_installed_library_paths(&mut self) {
        let mut changed = false;
        for installed in &mut self.installed_plugins {
            let needs_update = installed
                .library_path
                .as_ref()
                .map(|path| !path.is_file())
                .unwrap_or(true);
            if needs_update {
                installed.library_path =
                    Self::resolve_library_path(&installed.manifest, &installed.path);
                changed = true;
            }
        }
        if changed {
            self.persist_installed_plugins();
        }
    }

    fn inject_library_paths_into_workspace(&mut self) {
        let mut paths_by_kind: HashMap<String, String> = HashMap::new();
        for installed in &self.installed_plugins {
            if let Some(path) = installed.library_path.as_ref() {
                if path.is_file() {
                    paths_by_kind.insert(
                        installed.manifest.kind.clone(),
                        path.to_string_lossy().to_string(),
                    );
                }
            }
        }
        if paths_by_kind.is_empty() {
            return;
        }
        for plugin in &mut self.workspace.plugins {
            if let Some(path) = paths_by_kind.get(&plugin.kind) {
                if let Value::Object(ref mut map) = plugin.config {
                    let needs_update = match map.get("library_path") {
                        Some(Value::String(existing)) => {
                            existing.is_empty() || !Path::new(existing).is_file()
                        }
                        _ => true,
                    };
                    if needs_update {
                        map.insert("library_path".to_string(), Value::String(path.to_string()));
                    }
                }
            }
        }
    }

    fn poll_import_dialog(&mut self) {
        let result = match &self.import_dialog_rx {
            Some(rx) => rx.try_recv().ok(),
            None => None,
        };
        if let Some(selection) = result {
            self.import_dialog_rx = None;
            if let Some(path) = selection {
                self.import_workspace_from_path(&path);
            }
        }
    }

    fn poll_load_dialog(&mut self) {
        let result = match &self.load_dialog_rx {
            Some(rx) => rx.try_recv().ok(),
            None => None,
        };
        if let Some(selection) = result {
            self.load_dialog_rx = None;
            if let Some(path) = selection {
                self.workspace_path = path;
                self.load_workspace();
            }
        }
    }

    fn poll_csv_path_dialog(&mut self) {
        let result = match &self.csv_path_dialog_rx {
            Some(rx) => rx.try_recv().ok(),
            None => None,
        };
        if let Some(selection) = result {
            self.csv_path_dialog_rx = None;
            let plugin_id = self.csv_path_target_plugin_id.take();
            if let (Some(path), Some(id)) = (selection, plugin_id) {
                if let Some(plugin) = self.workspace.plugins.iter_mut().find(|p| p.id == id) {
                    if let Value::Object(ref mut map) = plugin.config {
                        map.insert(
                            "path".to_string(),
                            Value::String(path.to_string_lossy().to_string()),
                        );
                        map.insert("path_autogen".to_string(), Value::from(false));
                        self.mark_workspace_dirty();
                    }
                }
            }
        }
    }

    fn poll_plotter_screenshot_dialog(&mut self) {
        let result = match &self.plotter_screenshot_rx {
            Some(rx) => rx.try_recv().ok(),
            None => None,
        };
        if let Some(selection) = result {
            self.plotter_screenshot_rx = None;
            let target = self.plotter_screenshot_target.take();
            if let (Some(path), Some(plugin_id)) = (selection, target) {
                // Get preview settings for this plugin
                let settings = self.plotter_preview_settings.get(&plugin_id).cloned();
                let export_result = self
                    .plotters
                    .get(&plugin_id)
                    .and_then(|plotter| plotter.lock().ok())
                    .and_then(|mut plotter| {
                        if let Some((show_axes, show_legend, show_grid, series_names, colors, title, dark_theme, x_axis, y_axis, high_quality, export_svg)) = settings {
                            if export_svg {
                                plotter.export_svg_with_settings(
                                    &path,
                                    &self.logic_time_label,
                                    show_axes,
                                    show_legend,
                                    show_grid,
                                    &title,
                                    &series_names,
                                    &colors,
                                    dark_theme,
                                    &x_axis,
                                    &y_axis,
                                    self.plotter_preview_width,
                                    self.plotter_preview_height,
                                ).err()
                            } else if high_quality {
                                plotter.export_png_hq_with_settings(
                                    &path,
                                    &self.logic_time_label,
                                    show_axes,
                                    show_legend,
                                    show_grid,
                                    &title,
                                    &series_names,
                                    &colors,
                                    dark_theme,
                                    &x_axis,
                                    &y_axis,
                                ).err()
                            } else {
                                plotter.export_png_with_settings(
                                    &path,
                                    &self.logic_time_label,
                                    show_axes,
                                    show_legend,
                                    show_grid,
                                    &title,
                                    &series_names,
                                    &colors,
                                    dark_theme,
                                    &x_axis,
                                    &y_axis,
                                    self.plotter_preview_width,
                                    self.plotter_preview_height,
                                ).err()
                            }
                        } else {
                            plotter.export_png(&path, &self.logic_time_label).err()
                        }
                    });
                if let Some(err) = export_result {
                    self.show_info("Plotter", &err);
                }
            }
        }
    }

    fn request_plotter_screenshot(&mut self, plugin_id: u64) {
        if self.plotter_screenshot_rx.is_some() {
            return;
        }
        
        // Use title from preview settings, or default to "live_plotter"
        let base_name = self.plotter_preview_settings
            .get(&plugin_id)
            .and_then(|(_, _, _, _, _, title, _, _, _, _, _)| {
                if title.trim().is_empty() {
                    None
                } else {
                    Some(title.trim().replace(' ', "_").replace('/', "_").to_lowercase())
                }
            })
            .unwrap_or_else(|| "live_plotter".to_string());
            
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let day = now / 86_400;
        let hour = (now % 86_400) / 3_600;
        let minute = (now % 3_600) / 60;
        let second = now % 60;
        let default_name = format!("{}-{day}-{hour:02}-{minute:02}-{second:02}.png", base_name);
        
        let (tx, rx) = mpsc::channel();
        self.plotter_screenshot_rx = Some(rx);
        self.plotter_screenshot_target = Some(plugin_id);
        
        let is_svg = self.plotter_preview_settings
            .get(&plugin_id)
            .map(|(_, _, _, _, _, _, _, _, _, _, svg)| *svg)
            .unwrap_or(false);
            
        let extension = if is_svg { "svg" } else { "png" };
        let filter_name = if is_svg { "SVG" } else { "PNG" };
        
        spawn_file_dialog_thread(move || {
            let file = if has_rt_capabilities() {
                zenity_file_dialog("save", Some(&format!("*.{}", extension)))
            } else {
                rfd::FileDialog::new()
                    .add_filter(filter_name, &[extension])
                    .set_file_name(&default_name.replace(".png", &format!(".{}", extension)))
                    .save_file()
            };
            let _ = tx.send(file);
        });
    }

    fn poll_export_dialog(&mut self) {
        let result = match &self.export_dialog_rx {
            Some(rx) => rx.try_recv().ok(),
            None => None,
        };
        if let Some((source, dest)) = result {
            self.export_dialog_rx = None;
            if let Some(dest) = dest {
                let _ = fs::copy(source, dest);
                self.show_info("Workspace", "Workspace exported");
            }
        }
    }

    fn poll_install_dialog(&mut self) {
        let result = match &self.install_dialog_rx {
            Some(rx) => rx.try_recv().ok(),
            None => None,
        };

        if let Some(selection) = result {
            self.install_dialog_rx = None;
            if let Some(folder) = selection {
                let label = folder
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("plugin")
                    .to_string();
                self.start_plugin_build(
                    BuildAction::Install {
                        path: folder,
                        removable: true,
                        persist: true,
                    },
                    label,
                );
            } else {
                self.status = "Plugin install cancelled".to_string();
            }
        }
    }

    fn scan_detected_plugins(&mut self) {
        let mut detected = Vec::new();
        for base in ["plugins", "app_plugins"] {
            let base = PathBuf::from(base);
            if let Ok(entries) = fs::read_dir(&base) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_dir() {
                        continue;
                    }
                    let folder_name = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or_default();
                    if folder_name.eq_ignore_ascii_case("template") {
                        continue;
                    }
                    let manifest_path = path.join("plugin.toml");
                    if !manifest_path.is_file() {
                        continue;
                    }
                    let data = match fs::read_to_string(&manifest_path) {
                        Ok(content) => content,
                        Err(_) => continue,
                    };
                    let manifest: PluginManifest = match toml::from_str(&data) {
                        Ok(parsed) => parsed,
                        Err(_) => continue,
                    };
                    if manifest.kind == "comedi_daq" && !cfg!(feature = "comedi") {
                        continue;
                    }
                    let library_path = Self::resolve_library_path(&manifest, &path);
                    detected.push(DetectedPlugin {
                        manifest,
                        path,
                        library_path,
                    });
                }
            }
        }
        let mut detected_kinds: HashSet<String> = detected
            .iter()
            .map(|plugin| plugin.manifest.kind.clone())
            .collect();
        for installed in &self.installed_plugins {
            if detected_kinds.contains(&installed.manifest.kind) {
                continue;
            }
            detected.push(DetectedPlugin {
                manifest: installed.manifest.clone(),
                path: installed.path.clone(),
                library_path: installed.library_path.clone(),
            });
            detected_kinds.insert(installed.manifest.kind.clone());
        }
        self.detected_plugins = detected;
    }

    fn add_installed_plugin(&mut self, installed_index: usize) {
        let installed = match self.installed_plugins.get(installed_index) {
            Some(plugin) => plugin.clone(),
            None => {
                self.status = "Invalid installed plugin".to_string();
                return;
            }
        };

        let mut config_map = serde_json::Map::new();
        if let Some(library_path) = &installed.library_path {
            let (tx, rx) = std::sync::mpsc::channel();
            let _ = self.logic_tx.send(LogicMessage::QueryPluginMetadata(library_path.to_string_lossy().to_string(), tx));
            if let Ok(Some((_inputs, _outputs, variables, _display_schema))) = rx.recv() {
                for (name, value) in variables {
                    config_map.insert(name, Value::from(value));
                }
            }
        }
        if installed.manifest.kind == "csv_recorder" {
            config_map.insert("separator".to_string(), Value::from(","));
            config_map.insert("path".to_string(), Value::from(Self::default_csv_path()));
            config_map.insert("input_count".to_string(), Value::from(0));
            config_map.insert("columns".to_string(), Value::Array(Vec::new()));
            config_map.insert("include_time".to_string(), Value::from(true));
            config_map.insert("path_autogen".to_string(), Value::from(true));
        } else if installed.manifest.kind == "live_plotter" {
            config_map.insert("input_count".to_string(), Value::from(0));
            config_map.insert("refresh_hz".to_string(), Value::from(60.0));
            config_map.insert("window_ms".to_string(), Value::from(10000.0));
        } else if installed.manifest.kind == "performance_monitor" {
            config_map.insert("input_count".to_string(), Value::from(0));
            config_map.insert("max_latency_us".to_string(), Value::from(1000.0));
        } else if installed.manifest.kind == "comedi_daq" {
            config_map.insert("device_path".to_string(), Value::from("/dev/comedi0"));
            config_map.insert("scan_devices".to_string(), Value::from(false));
            config_map.insert("scan_nonce".to_string(), Value::from(0));
        }
        if let Some(library_path) = installed.library_path.as_ref() {
            config_map.insert(
                "library_path".to_string(),
                Value::String(library_path.to_string_lossy().to_string()),
            );
        }

        // Cache plugin behavior first
        self.ensure_plugin_behavior_cached_with_path(&installed.manifest.kind, installed.library_path.as_ref());
        
        // Determine if plugin should start based on behavior
        let loads_started = self.plugin_behaviors
            .get(&installed.manifest.kind)
            .map(|b| b.loads_started)
            .unwrap_or(false);
        

        let plugin = PluginDefinition {
            id: self.next_plugin_id,
            kind: installed.manifest.kind.clone(),
            config: Value::Object(config_map),
            priority: 99,
            running: loads_started,
        };

        self.workspace.plugins.push(plugin);
        self.next_plugin_id += 1;
        
        self.status = "Installed plugin added".to_string();
        self.mark_workspace_dirty();
    }
    fn duplicate_plugin(&mut self, plugin_id: u64) {
        let source = match self.workspace.plugins.iter().find(|p| p.id == plugin_id) {
            Some(plugin) => plugin.clone(),
            None => {
                self.show_info("Plugin", "Invalid plugin");
                return;
            }
        };
        let plugin = PluginDefinition {
            id: self.next_plugin_id,
            kind: source.kind,
            config: source.config,
            priority: source.priority,
            running: source.running,
        };
        let kind = plugin.kind.clone();
        self.workspace.plugins.push(plugin);
        self.next_plugin_id += 1;
        
        // Cache plugin behavior
        self.ensure_plugin_behavior_cached(&kind);
        
        self.status = "Plugin duplicated".to_string();
        self.mark_workspace_dirty();
    }

    fn uninstall_plugin(&mut self, installed_index: usize) {
        let plugin = match self.installed_plugins.get(installed_index) {
            Some(plugin) => plugin.clone(),
            None => {
                self.show_info("Plugin", "Invalid installed plugin");
                return;
            }
        };

        if !plugin.removable {
            self.show_info("Plugin", "Plugin is bundled and cannot be uninstalled");
            return;
        }

        let kind = plugin.manifest.kind.clone();
        
        // Close windows for plugins of this kind
        let plugin_ids: Vec<u64> = self.workspace.plugins
            .iter()
            .filter(|p| p.kind == kind)
            .map(|p| p.id)
            .collect();
        
        for id in &plugin_ids {
            if self.selected_plugin_id == Some(*id) {
                self.selected_plugin_id = None;
            }
            if self.plugin_config_id == Some(*id) {
                self.plugin_config_id = None;
                self.plugin_config_open = false;
            }
            self.plotters.remove(id);
        }
        
        // Remove all workspace plugin instances of this kind
        self.workspace.plugins.retain(|p| p.kind != kind);
        
        // Remove connections involving these plugins
        self.workspace.connections.retain(|conn| !plugin_ids.contains(&conn.from_plugin) && !plugin_ids.contains(&conn.to_plugin));
        
        self.installed_plugins.remove(installed_index);
        self.scan_detected_plugins();
        self.show_info("Plugin", "Plugin uninstalled");
        self.persist_installed_plugins();
    }

    fn refresh_installed_plugin(&mut self, kind: String, path: &Path) {
        // Clear cached plugin behavior for this kind so new version is loaded
        self.plugin_behaviors.remove(&kind);
        
        // Close windows for plugins of this kind (they'll need to be reopened with new version)
        let plugin_ids: Vec<u64> = self.workspace.plugins
            .iter()
            .filter(|p| p.kind == kind)
            .map(|p| p.id)
            .collect();
        
        for id in &plugin_ids {
            if self.selected_plugin_id == Some(*id) {
                self.selected_plugin_id = None;
            }
            if self.plugin_config_id == Some(*id) {
                self.plugin_config_id = None;
                self.plugin_config_open = false;
            }
            self.plotters.remove(id);
        }
        
        // Keep workspace plugins - just update the installed plugin metadata
        let manifest_path = path.join("plugin.toml");
        let data = match fs::read_to_string(&manifest_path) {
            Ok(content) => content,
            Err(err) => {
                self.status = format!("Failed to read plugin.toml: {err}");
                return;
            }
        };

        let manifest: PluginManifest = match toml::from_str(&data) {
            Ok(parsed) => parsed,
            Err(err) => {
                self.status = format!("Failed to parse plugin.toml: {err}");
                return;
            }
        };

        let library_path = Self::resolve_library_path(&manifest, path);
        if let Some(installed) = self
            .installed_plugins
            .iter_mut()
            .find(|plugin| plugin.manifest.kind == kind)
        {
            installed.manifest = manifest;
            let (tx, rx) = std::sync::mpsc::channel();
            if let Some(ref lib_path) = library_path {
                let _ = self.logic_tx.send(LogicMessage::QueryPluginMetadata(lib_path.to_string_lossy().to_string(), tx));
                if let Ok(Some((inputs, outputs, vars, display_schema))) = rx.recv() {
                    installed.metadata_inputs = inputs;
                    installed.metadata_outputs = outputs;
                    installed.metadata_variables = vars;
                    installed.display_schema = display_schema;
                }
            }
            if installed.manifest.kind == "performance_monitor" {
                installed.metadata_inputs = Vec::new();
                installed.metadata_outputs = vec![
                    "period_us".to_string(),
                    "latency_us".to_string(),
                    "jitter_us".to_string(),
                    "realtime_violation".to_string(),
                ];
                installed.metadata_variables = vec![("max_latency_us".to_string(), 1000.0)];
                installed.display_schema = Some(rtsyn_plugin::ui::DisplaySchema {
                    outputs: installed.metadata_outputs.clone(),
                    inputs: Vec::new(),
                    variables: Vec::new(),
                });
            } else if matches!(installed.manifest.kind.as_str(), "csv_recorder" | "live_plotter") {
                installed.display_schema = Some(rtsyn_plugin::ui::DisplaySchema {
                    outputs: Vec::new(),
                    inputs: Vec::new(),
                    variables: vec!["input_count".to_string(), "running".to_string()],
                });
            }
            installed.path = path.to_path_buf();
            installed.library_path = library_path;
        } else {
            let (mut metadata_inputs, mut metadata_outputs, mut metadata_variables, mut display_schema) = if let Some(ref lib_path) = library_path {
                let (tx, rx) = std::sync::mpsc::channel();
                let _ = self.logic_tx.send(LogicMessage::QueryPluginMetadata(lib_path.to_string_lossy().to_string(), tx));
                if let Ok(Some((inputs, outputs, vars, display_schema))) = rx.recv() {
                    (inputs, outputs, vars, display_schema)
                } else {
                    (vec![], vec![], vec![], None)
                }
            } else {
                (vec![], vec![], vec![], None)
            };
            if manifest.kind == "performance_monitor" {
                metadata_inputs = Vec::new();
                metadata_outputs = vec![
                    "period_us".to_string(),
                    "latency_us".to_string(),
                    "jitter_us".to_string(),
                    "realtime_violation".to_string(),
                ];
                metadata_variables = vec![("max_latency_us".to_string(), 1000.0)];
                display_schema = Some(rtsyn_plugin::ui::DisplaySchema {
                    outputs: metadata_outputs.clone(),
                    inputs: Vec::new(),
                    variables: Vec::new(),
                });
            } else if matches!(manifest.kind.as_str(), "csv_recorder" | "live_plotter") {
                display_schema = Some(rtsyn_plugin::ui::DisplaySchema {
                    outputs: Vec::new(),
                    inputs: Vec::new(),
                    variables: vec!["input_count".to_string(), "running".to_string()],
                });
            }
            self.installed_plugins.push(InstalledPlugin {
                manifest,
                path: path.to_path_buf(),
                library_path,
                removable: false,
                metadata_inputs,
                metadata_outputs,
                metadata_variables,
                display_schema,
            });
        }
        self.persist_installed_plugins();
    }

    fn remove_plugin(&mut self, plugin_index: usize) {
        if plugin_index >= self.workspace.plugins.len() {
            self.status = "Invalid plugin selection".to_string();
            return;
        }

        let removed_id = self.workspace.plugins[plugin_index].id;
        
        // Close associated windows
        if self.selected_plugin_id == Some(removed_id) {
            self.selected_plugin_id = None;
        }
        if self.plugin_config_id == Some(removed_id) {
            self.plugin_config_id = None;
            self.plugin_config_open = false;
        }
        self.plotters.remove(&removed_id);
        
        self.workspace.plugins.remove(plugin_index);
        self.workspace
            .connections
            .retain(|conn| conn.from_plugin != removed_id && conn.to_plugin != removed_id);
        let ids: Vec<u64> = self.workspace.plugins.iter().map(|p| p.id).collect();
        for id in ids {
            self.sync_extendable_input_count(id);
        }
        self.recompute_plotter_ui_hz();
        self.enforce_connection_dependent();
        self.status = "Plugin removed".to_string();
        self.mark_workspace_dirty();
    }

    fn restart_plugin(&mut self, plugin_id: u64) {
        let _ = self.logic_tx.send(LogicMessage::RestartPlugin(plugin_id));
    }

    fn add_connection(&mut self) {
        if self.connection_from_idx == self.connection_to_idx {
            self.show_info("Connections", "Cannot connect a plugin to itself");
            return;
        }
        if self.workspace.plugins.len() < 2 {
            self.status = "Add at least two plugins before connecting".to_string();
            return;
        }

        let from_plugin = match self.workspace.plugins.get(self.connection_from_idx) {
            Some(plugin) => plugin.id,
            None => {
                self.status = "Invalid source plugin".to_string();
                return;
            }
        };

        let to_plugin = match self.workspace.plugins.get(self.connection_to_idx) {
            Some(plugin) => plugin.id,
            None => {
                self.status = "Invalid target plugin".to_string();
                return;
            }
        };

        let from_port = self.connection_from_port.trim();
        let to_port = self.connection_to_port.trim();
        let kind = self.connection_kind.trim();

        if from_port.is_empty() || to_port.is_empty() || kind.is_empty() {
            self.status = "Connection fields cannot be empty".to_string();
            return;
        }
        let mut to_port_string = to_port.to_string();
        if let Some(target) = self.workspace.plugins.iter().find(|p| p.id == to_plugin) {
            if self.is_extendable_inputs(&target.kind) && to_port_string == "in" {
                let next_idx = self.next_available_extendable_input_index(to_plugin);
                to_port_string = format!("in_{next_idx}");
            }
        }
        let input_idx = to_port_string
            .strip_prefix("in_")
            .and_then(|v| v.parse::<usize>().ok());
        let default_column = input_idx.map(|idx| self.default_csv_column(to_plugin, idx));

        let connection = ConnectionDefinition {
            from_plugin,
            from_port: from_port.to_string(),
            to_plugin,
            to_port: to_port_string,
            kind: kind.to_string(),
        };
        if let Err(err) = workspace_add_connection(&mut self.workspace.connections, connection, 1) {
            let message = match err {
                ConnectionRuleError::SelfConnection => "Cannot connect a plugin to itself.",
                ConnectionRuleError::InputLimitExceeded => "Input already has a connection.",
                ConnectionRuleError::DuplicateConnection => {
                    "Connection between these plugins already exists."
                }
            };
            self.show_info("Connections", message);
            return;
        }
        if let Some(idx) = input_idx {
            if let Some(target) = self.workspace.plugins.iter().find(|p| p.id == to_plugin) {
                if self.is_extendable_inputs(&target.kind) {
                    self.ensure_extendable_input_count(to_plugin, idx + 1);
                }
            }
        }
        if let (Some(idx), Some(default_name)) = (input_idx, default_column) {
            if let Some(plugin) = self
                .workspace
                .plugins
                .iter_mut()
                .find(|p| p.id == to_plugin)
            {
                if plugin.kind == "csv_recorder" {
                    if let Value::Object(ref mut map) = plugin.config {
                        let input_count =
                            map.get("input_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                        let mut columns: Vec<String> = map
                            .get("columns")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .map(|v| v.as_str().unwrap_or("").to_string())
                                    .collect()
                            })
                            .unwrap_or_default();
                        if columns.len() < input_count {
                            for _ in columns.len()..input_count {
                                columns.push(String::new());
                            }
                        }
                        if idx < columns.len() && columns[idx].is_empty() {
                            columns[idx] = default_name;
                            map.insert(
                                "columns".to_string(),
                                Value::Array(columns.into_iter().map(Value::from).collect()),
                            );
                        }
                    }
                }
            }
        }
        self.status = "Connection added".to_string();
        self.enforce_connection_dependent();
        self.mark_workspace_dirty();
    }

    fn add_connection_direct(
        &mut self,
        from_plugin: u64,
        from_port: String,
        to_plugin: u64,
        to_port: String,
        kind: String,
    ) {
        if from_plugin == to_plugin {
            self.show_info("Connections", "Cannot connect a plugin to itself");
            return;
        }
        if from_port.trim().is_empty() || to_port.trim().is_empty() || kind.trim().is_empty() {
            self.show_info("Connections", "Connection fields cannot be empty");
            return;
        }
        let mut to_port_string = to_port.clone();
        if let Some(target) = self.workspace.plugins.iter().find(|p| p.id == to_plugin) {
            if self.is_extendable_inputs(&target.kind) && to_port_string == "in" {
                let next_idx = self.next_available_extendable_input_index(to_plugin);
                to_port_string = format!("in_{next_idx}");
            }
        }
        let input_idx = to_port_string
            .strip_prefix("in_")
            .and_then(|v| v.parse::<usize>().ok());
        let default_column = input_idx.map(|idx| self.default_csv_column(to_plugin, idx));
        let connection = ConnectionDefinition {
            from_plugin,
            from_port,
            to_plugin,
            to_port: to_port_string,
            kind,
        };
        if let Err(err) = workspace_add_connection(&mut self.workspace.connections, connection, 1) {
            let message = match err {
                ConnectionRuleError::SelfConnection => "Cannot connect a plugin to itself.",
                ConnectionRuleError::InputLimitExceeded => "Input already has a connection.",
                ConnectionRuleError::DuplicateConnection => {
                    "Connection between these plugins already exists."
                }
            };
            self.show_info("Connections", message);
            return;
        }
        if let (Some(idx), Some(default_name)) = (input_idx, default_column) {
            if let Some(plugin) = self.workspace.plugins.iter().find(|p| p.id == to_plugin) {
                if self.is_extendable_inputs(&plugin.kind) {
                    self.ensure_extendable_input_count(to_plugin, idx + 1);
                }
            }
            if let Some(plugin) = self
                .workspace
                .plugins
                .iter_mut()
                .find(|p| p.id == to_plugin)
            {
                if plugin.kind == "csv_recorder" {
                    if let Value::Object(ref mut map) = plugin.config {
                        let input_count =
                            map.get("input_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                        let mut columns: Vec<String> = map
                            .get("columns")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .map(|v| v.as_str().unwrap_or("").to_string())
                                    .collect()
                            })
                            .unwrap_or_default();
                        if columns.len() < input_count {
                            for _ in columns.len()..input_count {
                                columns.push(String::new());
                            }
                        }
                        if idx < columns.len() && columns[idx].is_empty() {
                            columns[idx] = default_name;
                            map.insert(
                                "columns".to_string(),
                                Value::Array(columns.into_iter().map(Value::from).collect()),
                            );
                        }
                    }
                }
            }
        }
        self.mark_workspace_dirty();
        self.enforce_connection_dependent();
    }

    fn load_workspace(&mut self) {
        if self.workspace_path.as_os_str().is_empty() {
            self.show_info("Workspace", "Workspace path is required");
            return;
        }

        match WorkspaceDefinition::load_from_file(&self.workspace_path) {
            Ok(workspace) => {
                self.workspace = workspace;
                self.refresh_installed_library_paths();
                self.inject_library_paths_into_workspace();
                self.apply_loads_started_on_load();
                self.enforce_connection_dependent();
                self.apply_workspace_settings();
                self.sync_next_plugin_id();
                self.mark_workspace_dirty();
            }
            Err(err) => {
                self.show_info("Workspace", &format!("Load failed: {err}"));
            }
        }
    }

    fn scan_workspaces(&mut self) {
        self.workspace_entries = scan_workspace_entries(&self.workspace_dir);
    }

    fn workspace_file_path(&self, name: &str) -> PathBuf {
        workspace_file_path(&self.workspace_dir, name)
    }

    fn create_workspace_from_dialog(&mut self) -> bool {
        let name = self.workspace_name_input.trim();
        if name.is_empty() {
            self.show_info("Workspace", "Workspace name is required");
            return false;
        }
        let path = self.workspace_file_path(name);
        let _ = fs::create_dir_all(&self.workspace_dir);
        if path.exists() {
            let workspace = WorkspaceDefinition {
                name: name.to_string(),
                description: self.workspace_description_input.trim().to_string(),
                target_hz: 1000,
                plugins: Vec::new(),
                connections: Vec::new(),
                settings: self.current_workspace_settings(),
            };
            self.show_confirm(
                "Overwrite workspace",
                "Workspace already exists. Override it?",
                "Overwrite",
                ConfirmAction::OverwriteWorkspace(path, workspace),
            );
            return false;
        }
        let workspace = WorkspaceDefinition {
            name: name.to_string(),
            description: self.workspace_description_input.trim().to_string(),
            target_hz: 1000,
            plugins: Vec::new(),
            connections: Vec::new(),
            settings: self.current_workspace_settings(),
        };
        self.workspace_path = path.clone();
        self.workspace = workspace;
        self.next_plugin_id = 1;
        let _ = self.workspace.save_to_file(&path);
        self.show_info("Workspace", "Workspace created");
        self.scan_workspaces();
        self.mark_workspace_dirty();
        true
    }

    fn save_workspace_as(&mut self) -> bool {
        let name = self.workspace_name_input.trim();
        if name.is_empty() {
            self.show_info("Workspace", "Workspace name is required");
            return false;
        }
        let path = self.workspace_file_path(name);
        let _ = fs::create_dir_all(&self.workspace_dir);
        let existed = path.is_file();
        self.workspace.name = name.to_string();
        self.workspace.description = self.workspace_description_input.trim().to_string();
        self.workspace.settings = self.current_workspace_settings();
        self.workspace_path = path.clone();
        let _ = self.workspace.save_to_file(&path);
        if existed {
            self.show_info("Workspace", "Workspace updated");
        } else {
            self.show_info("Workspace", "Workspace created");
        }
        self.scan_workspaces();
        true
    }

    fn save_workspace_overwrite_current(&mut self) {
        if self.workspace_path.as_os_str().is_empty() {
            self.open_workspace_dialog(WorkspaceDialogMode::Save);
            return;
        }
        self.workspace.settings = self.current_workspace_settings();
        let _ = self.workspace.save_to_file(&self.workspace_path);
        self.show_info("Workspace", "Workspace updated");
        self.scan_workspaces();
    }

    fn update_workspace_metadata(&mut self, path: &Path) -> bool {
        let name = self.workspace_name_input.trim();
        if name.is_empty() {
            self.show_info("Workspace", "Workspace name is required");
            return false;
        }
        let new_path = self.workspace_file_path(name);
        let mut updated = false;
        if let Ok(data) = fs::read(path) {
            if let Ok(mut workspace) = serde_json::from_slice::<WorkspaceDefinition>(&data) {
                workspace.name = name.to_string();
                workspace.description = self.workspace_description_input.trim().to_string();
                let _ = workspace.save_to_file(&new_path);
                if new_path != path {
                    let _ = fs::remove_file(path);
                }
                self.show_info("Workspace", "Workspace updated");
                updated = true;
            }
        }
        self.scan_workspaces();
        updated
    }

    fn export_workspace_path(&mut self, source: &Path) {
        if self.export_dialog_rx.is_some() {
            self.show_info("Workspace", "Dialog already open");
            return;
        }
        let source = source.to_path_buf();
        let (tx, rx) = mpsc::channel();
        self.export_dialog_rx = Some(rx);
        spawn_file_dialog_thread(move || {
            let dest = if has_rt_capabilities() {
                zenity_file_dialog("save", None)
            } else {
                rfd::FileDialog::new().save_file()
            };
            let _ = tx.send((source, dest));
        });
    }

    fn import_workspace_from_path(&mut self, path: &Path) {
        if let Ok(data) = fs::read(path) {
            if let Ok(workspace) = serde_json::from_slice::<WorkspaceDefinition>(&data) {
                let dest = self.workspace_file_path(&workspace.name);
                let _ = fs::write(dest, data);
                self.show_info("Workspace", "Workspace imported");
                self.scan_workspaces();
            }
        }
    }

    fn load_installed_plugins(&mut self) {
        self.installed_plugins.clear();
        self.load_bundled_plugins();

        let data = match fs::read_to_string(&self.install_db_path) {
            Ok(content) => content,
            Err(_) => return,
        };

        let entries: Vec<String> = match serde_json::from_str(&data) {
            Ok(value) => value,
            Err(err) => {
                self.status = format!("Failed to read plugin registry: {err}");
                return;
            }
        };

        for entry in entries {
            self.install_plugin_from_folder(entry, true, false);
        }
    }

    fn load_bundled_plugins(&mut self) {
        let app_plugins_dir = PathBuf::from("app_plugins");
        if let Ok(entries) = fs::read_dir(&app_plugins_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join("plugin.toml").is_file() {
                    self.install_plugin_from_folder(path, false, false);
                }
            }
        }
    }

    fn persist_installed_plugins(&mut self) {
        let _ = fs::create_dir_all(
            self.install_db_path
                .parent()
                .unwrap_or_else(|| Path::new("app_plugins")),
        );

        let user_entries: Vec<String> = self
            .installed_plugins
            .iter()
            .filter(|plugin| plugin.removable)
            .map(|plugin| plugin.path.to_string_lossy().to_string())
            .collect();

        if let Ok(data) = serde_json::to_string_pretty(&user_entries) {
            let _ = fs::write(&self.install_db_path, data);
        }
    }

    fn display_kind(kind: &str) -> String {
        kind.replace('_', " ")
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
        self.confirm_dialog_title = title.to_string();
        self.confirm_dialog_message = message.to_string();
        self.confirm_dialog_action_label = action_label.to_string();
        self.confirm_action = Some(action);
        self.confirm_dialog_open = true;
    }

    fn perform_confirm_action(&mut self, action: ConfirmAction) {
        match action {
            ConfirmAction::RemovePlugin(plugin_id) => {
                if let Some(index) = self
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
                let _ = fs::remove_file(&path);
                self.scan_workspaces();
                self.show_info("Workspace", "Workspace deleted");
            }
            ConfirmAction::OverwriteWorkspace(path, workspace) => {
                let _ = workspace.save_to_file(&path);
                self.workspace = workspace;
                self.workspace_path = path;
                self.scan_workspaces();
                self.show_info("Workspace", "Workspace updated");
            }
        }
    }

    fn poll_logic_state(&mut self) {
        let mut latest: Option<LogicState> = None;
        let mut merged_samples: HashMap<u64, Vec<(u64, Vec<f64>)>> = HashMap::new();
        while let Ok(state) = self.logic_state_rx.try_recv() {
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
            if self.last_output_update.elapsed() >= output_interval {
                // Filter out outputs from stopped plugins
                let running_plugins: std::collections::HashSet<u64> = self.workspace.plugins
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
                let filtered_internals: HashMap<(u64, String), serde_json::Value> = internal_variable_values
                    .into_iter()
                    .filter(|((id, _), _)| running_plugins.contains(id))
                    .collect();
                
                self.computed_outputs = filtered_outputs;
                self.input_values = filtered_inputs;
                self.internal_variable_values = filtered_internals;
                self.viewer_values = viewer_values;
                self.last_output_update = Instant::now();
            }
        }
    }

    fn ports_for_kind(&self, kind: &str, inputs: bool) -> Vec<String> {
        self.installed_plugins
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
        if let Some(cached) = self.plugin_behaviors.get(kind) {
            return matches!(cached.extendable_inputs, rtsyn_plugin::ui::ExtendableInputs::Auto { .. } | rtsyn_plugin::ui::ExtendableInputs::Manual);
        }
        false
    }

    fn auto_extend_inputs(&self, kind: &str) -> bool {
        if let Some(cached) = self.plugin_behaviors.get(kind) {
            return matches!(cached.extendable_inputs, rtsyn_plugin::ui::ExtendableInputs::Auto { .. });
        }
        false
    }
    

    fn ensure_plugin_behavior_cached_with_path(&mut self, kind: &str, library_path: Option<&PathBuf>) {
        if self.plugin_behaviors.contains_key(kind) {
            return;
        }
        
        let (tx, rx) = std::sync::mpsc::channel();
        let path_str = library_path.map(|p| p.to_string_lossy().to_string());
        let _ = self.logic_tx.send(LogicMessage::QueryPluginBehavior(kind.to_string(), path_str, tx));
        if let Ok(Some(behavior)) = rx.recv_timeout(std::time::Duration::from_millis(100)) {
            self.plugin_behaviors.insert(kind.to_string(), behavior);
        }
    }

    fn ensure_plugin_behavior_cached(&mut self, kind: &str) {
        if self.plugin_behaviors.contains_key(kind) {
            return;
        }
        
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = self.logic_tx.send(LogicMessage::QueryPluginBehavior(kind.to_string(), None, tx));
        if let Ok(Some(behavior)) = rx.recv_timeout(std::time::Duration::from_millis(100)) {
            self.plugin_behaviors.insert(kind.to_string(), behavior);
        }
    }

    fn extendable_input_index(port: &str) -> Option<usize> {
        if port == "in" {
            Some(0)
        } else {
            port.strip_prefix("in_")
                .and_then(|value| value.parse::<usize>().ok())
        }
    }

    fn next_available_extendable_input_index(&self, plugin_id: u64) -> usize {
        let mut used: HashSet<usize> = HashSet::new();
        for connection in &self.workspace.connections {
            if connection.to_plugin == plugin_id {
                if let Some(idx) = Self::extendable_input_index(&connection.to_port) {
                    used.insert(idx);
                }
            }
        }
        let mut idx = 0;
        while used.contains(&idx) {
            idx += 1;
        }
        idx
    }

    fn extendable_input_display_ports(
        &self,
        plugin_id: u64,
        include_placeholder: bool,
    ) -> Vec<String> {
        let mut entries: Vec<(usize, String)> = self
            .workspace
            .connections
            .iter()
            .filter(|conn| conn.to_plugin == plugin_id)
            .filter_map(|conn| {
                Self::extendable_input_index(&conn.to_port)
                    .map(|idx| (idx, conn.to_port.clone()))
            })
            .collect();
        entries.sort_by_key(|(idx, _)| *idx);
        entries.dedup_by(|a, b| a.0 == b.0);
        let mut list: Vec<String> = entries.into_iter().map(|(_, port)| port).collect();
        if include_placeholder {
            if list.is_empty() {
                list.push("in_0".to_string());
            } else {
                let next_idx = self.next_available_extendable_input_index(plugin_id);
                let next_name = format!("in_{next_idx}");
                if !list.contains(&next_name) {
                    list.push(next_name);
                }
            }
        }
        list
    }

    fn remove_extendable_input_at(&mut self, plugin_id: u64, remove_idx: usize) {
        let plugin_index = match self
            .workspace
            .plugins
            .iter()
            .position(|p| p.id == plugin_id)
        {
            Some(idx) => idx,
            None => return,
        };
        let kind = self.workspace.plugins[plugin_index].kind.clone();
        if !self.is_extendable_inputs(&kind) {
            return;
        }

        let (current_count, mut columns, is_csv) = {
            let plugin = &self.workspace.plugins[plugin_index];
            let mut input_count = plugin
                .config
                .get("input_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let mut columns = Vec::new();
            let is_csv = plugin.kind == "csv_recorder";
            if is_csv {
                columns = plugin
                    .config
                    .get("columns")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .map(|v| v.as_str().unwrap_or("").to_string())
                            .collect()
                    })
                    .unwrap_or_default();
                if columns.len() > input_count {
                    input_count = columns.len();
                }
            }
            let mut max_idx: Option<usize> = None;
            for conn in &self.workspace.connections {
                if conn.to_plugin != plugin_id {
                    continue;
                }
                if let Some(idx) = Self::extendable_input_index(&conn.to_port) {
                    max_idx = Some(max_idx.map(|v| v.max(idx)).unwrap_or(idx));
                }
            }
            if let Some(idx) = max_idx {
                input_count = input_count.max(idx + 1);
            }
            (input_count, columns, is_csv)
        };

        if remove_idx >= current_count {
            return;
        }

        remove_extendable_input(&mut self.workspace.connections, plugin_id, remove_idx);
        let new_count = current_count.saturating_sub(1);

        let map = match self.workspace.plugins[plugin_index].config {
            Value::Object(ref mut map) => map,
            _ => {
                self.workspace.plugins[plugin_index].config = Value::Object(serde_json::Map::new());
                match self.workspace.plugins[plugin_index].config {
                    Value::Object(ref mut map) => map,
                    _ => return,
                }
            }
        };
        map.insert("input_count".to_string(), Value::from(new_count as u64));
        if is_csv {
            if remove_idx < columns.len() {
                columns.remove(remove_idx);
            }
            if columns.len() > new_count {
                columns.truncate(new_count);
            } else if columns.len() < new_count {
                columns.resize(new_count, String::new());
            }
            map.insert(
                "columns".to_string(),
                Value::Array(columns.into_iter().map(Value::from).collect()),
            );
        }

        self.mark_workspace_dirty();
        self.enforce_connection_dependent();
        if kind == "live_plotter" {
            self.recompute_plotter_ui_hz();
        }
    }

    fn reindex_extendable_inputs(&mut self, plugin_id: u64) {
        let kind = match self
            .workspace
            .plugins
            .iter()
            .find(|p| p.id == plugin_id)
            .map(|p| p.kind.clone())
        {
            Some(kind) => kind,
            None => return,
        };
        if !self.is_extendable_inputs(&kind) {
            return;
        }

        let mut entries: Vec<(usize, usize)> = self
            .workspace
            .connections
            .iter()
            .enumerate()
            .filter(|(_, conn)| conn.to_plugin == plugin_id)
            .filter_map(|(idx, conn)| {
                Self::extendable_input_index(&conn.to_port).map(|port_idx| (idx, port_idx))
            })
            .collect();
        entries.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

        for (new_idx, (conn_idx, _)) in entries.iter().enumerate() {
            if let Some(conn) = self.workspace.connections.get_mut(*conn_idx) {
                conn.to_port = format!("in_{new_idx}");
            }
        }

        let Some(plugin) = self
            .workspace
            .plugins
            .iter_mut()
            .find(|p| p.id == plugin_id)
        else {
            return;
        };
        let map = match plugin.config {
            Value::Object(ref mut map) => map,
            _ => {
                plugin.config = Value::Object(serde_json::Map::new());
                match plugin.config {
                    Value::Object(ref mut map) => map,
                    _ => return,
                }
            }
        };
        let required_count = entries.len();
        map.insert("input_count".to_string(), Value::from(required_count as u64));

        if plugin.kind == "csv_recorder" {
            let mut columns: Vec<String> = map
                .get("columns")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|v| v.as_str().unwrap_or("").to_string())
                        .collect()
                })
                .unwrap_or_default();
            if columns.len() > required_count {
                columns.truncate(required_count);
            } else if columns.len() < required_count {
                columns.resize(required_count, String::new());
            }
            map.insert(
                "columns".to_string(),
                Value::Array(columns.into_iter().map(Value::from).collect()),
            );
        }
    }

    fn remove_connection_with_input(&mut self, connection: ConnectionDefinition) {
        if Self::extendable_input_index(&connection.to_port).is_some() {
            let target_kind = self
                .workspace
                .plugins
                .iter()
                .find(|p| p.id == connection.to_plugin)
                .map(|p| p.kind.clone());
            if let Some(kind) = target_kind {
                if self.is_extendable_inputs(&kind) {
                    let matches = |left: &ConnectionDefinition, right: &ConnectionDefinition| {
                        left.from_plugin == right.from_plugin
                            && left.to_plugin == right.to_plugin
                            && left.from_port == right.from_port
                            && left.to_port == right.to_port
                            && left.kind == right.kind
                    };
                    self.workspace
                        .connections
                        .retain(|conn| !matches(conn, &connection));
                    self.reindex_extendable_inputs(connection.to_plugin);
                    self.mark_workspace_dirty();
                    self.enforce_connection_dependent();
                    if kind == "live_plotter" {
                        self.recompute_plotter_ui_hz();
                    }
                    return;
                }
            }
        }
        let matches = |left: &ConnectionDefinition, right: &ConnectionDefinition| {
            left.from_plugin == right.from_plugin
                && left.to_plugin == right.to_plugin
                && left.from_port == right.from_port
                && left.to_port == right.to_port
                && left.kind == right.kind
        };
        self.workspace
            .connections
            .retain(|conn| !matches(conn, &connection));
        self.mark_workspace_dirty();
        self.enforce_connection_dependent();
    }

    fn enforce_connection_dependent(&mut self) {
        let mut stopped = Vec::new();
        let mut plotter_closed = false;
        
        // Build map of connection-dependent plugins from cached behaviors
        let mut dependent_by_kind: HashMap<String, bool> = HashMap::new();
        
        // Hardcoded for app plugins
        dependent_by_kind.insert("csv_recorder".to_string(), true);
        dependent_by_kind.insert("live_plotter".to_string(), true);
        dependent_by_kind.insert("comedi_daq".to_string(), true);
        
        let incoming: HashSet<u64> = self
            .workspace
            .connections
            .iter()
            .map(|conn| conn.to_plugin)
            .collect();
        for plugin in &mut self.workspace.plugins {
            if !dependent_by_kind
                .get(&plugin.kind)
                .copied()
                .unwrap_or(false)
            {
                continue;
            }
            if incoming.contains(&plugin.id) {
                continue;
            }
            if plugin.kind == "live_plotter" {
                if let Some(plotter) = self.plotters.get(&plugin.id) {
                    if let Ok(mut plotter) = plotter.lock() {
                        if plotter.open {
                            plotter.open = false;
                            plotter_closed = true;
                        }
                    }
                }
            }
            if plugin.running {
                plugin.running = false;
                stopped.push(plugin.id);
            }
        }
        for id in stopped {
            let _ = self
                .logic_tx
                .send(LogicMessage::SetPluginRunning(id, false));
        }
        if plotter_closed {
            self.recompute_plotter_ui_hz();
        }
    }

    fn ensure_extendable_input_count(&mut self, plugin_id: u64, required_count: usize) {
        let kind = self
            .workspace
            .plugins
            .iter()
            .find(|p| p.id == plugin_id)
            .map(|p| p.kind.clone());
        let Some(kind) = kind else {
            return;
        };
        if !self.is_extendable_inputs(&kind) {
            return;
        }
        let Some(plugin) = self
            .workspace
            .plugins
            .iter_mut()
            .find(|p| p.id == plugin_id)
        else {
            return;
        };
        let map = match plugin.config {
            Value::Object(ref mut map) => map,
            _ => {
                plugin.config = Value::Object(serde_json::Map::new());
                match plugin.config {
                    Value::Object(ref mut map) => map,
                    _ => return,
                }
            }
        };
        let mut input_count = map.get("input_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        if input_count < required_count {
            input_count = required_count;
            map.insert("input_count".to_string(), Value::from(input_count as u64));
        }

        if plugin.kind == "csv_recorder" {
            let mut columns: Vec<String> = map
                .get("columns")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|v| v.as_str().unwrap_or("").to_string())
                        .collect()
                })
                .unwrap_or_default();
            if columns.len() < input_count {
                columns.resize(input_count, String::new());
                map.insert(
                    "columns".to_string(),
                    Value::Array(columns.into_iter().map(Value::from).collect()),
                );
            }
        }
    }

    fn sync_extendable_input_count(&mut self, plugin_id: u64) {
        let kind = self
            .workspace
            .plugins
            .iter()
            .find(|p| p.id == plugin_id)
            .map(|p| p.kind.clone());
        let Some(kind) = kind else {
            return;
        };
        if !self.is_extendable_inputs(&kind) {
            return;
        }
        let Some(plugin) = self
            .workspace
            .plugins
            .iter_mut()
            .find(|p| p.id == plugin_id)
        else {
            return;
        };
        let mut max_idx: Option<usize> = None;
        for conn in &self.workspace.connections {
            if conn.to_plugin != plugin_id {
                continue;
            }
            if let Some(idx) = conn
                .to_port
                .strip_prefix("in_")
                .and_then(|v| v.parse().ok())
            {
                max_idx = Some(max_idx.map(|v| v.max(idx)).unwrap_or(idx));
            }
        }
        let required_count = max_idx.map(|v| v + 1).unwrap_or(0);
        let map = match plugin.config {
            Value::Object(ref mut map) => map,
            _ => {
                plugin.config = Value::Object(serde_json::Map::new());
                match plugin.config {
                    Value::Object(ref mut map) => map,
                    _ => return,
                }
            }
        };
        map.insert(
            "input_count".to_string(),
            Value::from(required_count as u64),
        );
        if plugin.kind == "csv_recorder" {
            let mut columns: Vec<String> = map
                .get("columns")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|v| v.as_str().unwrap_or("").to_string())
                        .collect()
                })
                .unwrap_or_default();
            if columns.len() > required_count {
                columns.truncate(required_count);
            } else if columns.len() < required_count {
                columns.resize(required_count, String::new());
            }
            map.insert(
                "columns".to_string(),
                Value::Array(columns.into_iter().map(Value::from).collect()),
            );
        }
    }

    fn ports_for_plugin(&self, plugin_id: u64, inputs: bool) -> Vec<String> {
        let Some(plugin) = self.workspace.plugins.iter().find(|p| p.id == plugin_id) else {
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
            .installed_plugins
            .iter()
            .map(|plugin| (plugin.manifest.kind.clone(), plugin.manifest.name.clone()))
            .collect();
        self.workspace
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

    fn default_csv_column(&self, recorder_id: u64, input_idx: usize) -> String {
        let port = format!("in_{input_idx}");
        if let Some(conn) = self
            .workspace
            .connections
            .iter()
            .find(|conn| conn.to_plugin == recorder_id && conn.to_port == port)
        {
            let source_name = self
                .plugin_display_name(conn.from_plugin)
                .replace(' ', "_")
                .to_lowercase();
            let port = conn.from_port.to_lowercase();
            return format!("{source_name}_{}_{}", conn.from_plugin, port);
        }
        let recorder_name = self
            .plugin_display_name(recorder_id)
            .replace(' ', "_")
            .to_lowercase();
        format!("{recorder_name}_{}_{}", recorder_id, port.to_lowercase())
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

    fn plotter_config_from_value(&self, config: &Value) -> (usize, f64, f64, f64) {
        let input_count = config
            .get("input_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let refresh_hz = config
            .get("refresh_hz")
            .and_then(|v| v.as_f64())
            .unwrap_or(60.0);
        let window_multiplier = config
            .get("window_multiplier")
            .and_then(|v| v.as_f64())
            .unwrap_or(10000.0);
        let window_value = config
            .get("window_value")
            .and_then(|v| v.as_f64())
            .unwrap_or(10.0);
        let window_ms = config
            .get("window_ms")
            .and_then(|v| v.as_f64())
            .unwrap_or(window_multiplier * window_value);
        let amplitude = config
            .get("amplitude")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        (input_count, refresh_hz, window_ms, amplitude)
    }

    fn plotter_series_names(&self, plotter_id: u64, input_count: usize) -> Vec<String> {
        let mut names = Vec::with_capacity(input_count);
        for idx in 0..input_count {
            let port = format!("in_{idx}");
            if let Some(conn) = self
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
                input_sum_any(&self.workspace.connections, outputs, plotter_id, &ports)
            } else {
                input_sum(&self.workspace.connections, outputs, plotter_id, &port)
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
        let time_s = tick as f64 * self.logic_period_seconds.max(0.0);
        let mut live_plotter_ids: HashSet<u64> = HashSet::new();

        for plugin in &self.workspace.plugins {
            if plugin.kind != "live_plotter" {
                continue;
            }
            live_plotter_ids.insert(plugin.id);
            let (input_count, refresh_hz, window_ms, amplitude) =
                self.plotter_config_from_value(&plugin.config);
            let series_names = self.plotter_series_names(plugin.id, input_count);
            let is_open = self
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
                .plotters
                .entry(plugin.id)
                .or_insert_with(|| Arc::new(Mutex::new(LivePlotter::new(plugin.id))));
            if let Ok(mut plotter) = plotter.lock() {
                plotter.update_config(
                    input_count,
                    refresh_hz,
                    window_ms,
                    amplitude,
                    self.logic_period_seconds,
                );
                plotter.set_series_names(series_names);
                if plotter.open && plugin.running {
                    if let Some(samples) = samples.get(&plugin.id) {
                        for (sample_tick, values) in samples {
                            let sample_time_s =
                                *sample_tick as f64 * self.logic_period_seconds.max(0.0);
                            plotter.push_sample(
                                *sample_tick,
                                sample_time_s,
                                self.logic_time_scale,
                                values,
                            );
                        }
                    } else {
                        plotter.push_sample(tick, time_s, self.logic_time_scale, &values);
                    }
                    if refresh_hz > max_refresh {
                        max_refresh = refresh_hz;
                    }
                }
            }
        }

        self.plotters
            .retain(|plugin_id, _| live_plotter_ids.contains(plugin_id));
        self.refresh_logic_ui_hz(max_refresh);
    }

    fn refresh_logic_ui_hz(&mut self, max_refresh: f64) {
        let target_hz = if max_refresh > 0.0 { max_refresh } else { 1.0 };
        if (self.logic_ui_hz - target_hz).abs() > f64::EPSILON {
            self.logic_ui_hz = target_hz;
            self.send_logic_settings();
        }
    }

    fn recompute_plotter_ui_hz(&mut self) {
        let mut max_refresh = 1.0;
        for plotter in self.plotters.values() {
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
            self.workspace_settings_tab,
            self.frequency_unit,
            self.period_unit,
        );
        let cores: Vec<usize> = self
            .selected_cores
            .iter()
            .enumerate()
            .filter_map(|(idx, enabled)| if *enabled { Some(idx) } else { None })
            .collect();
        self.logic_period_seconds = period_seconds;
        self.logic_time_scale = time_scale;
        self.logic_time_label = time_label.clone();
        let _ = self
            .logic_tx
            .send(LogicMessage::UpdateSettings(LogicSettings {
                cores,
                period_seconds,
                time_scale,
                time_label,
                ui_hz: self.logic_ui_hz,
                max_integration_steps: 10, // Default reasonable limit for real-time performance
            }));
    }

    fn current_workspace_settings(&self) -> WorkspaceSettings {
        let timing_mode = match self.workspace_settings_tab {
            WorkspaceTimingTab::Frequency => "frequency",
            WorkspaceTimingTab::Period => "period",
        };
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
            timing_mode: timing_mode.to_string(),
            frequency_value: self.frequency_value,
            frequency_unit: frequency_unit.to_string(),
            period_value: self.period_value,
            period_unit: period_unit.to_string(),
            selected_cores,
        }
    }

    fn apply_workspace_settings(&mut self) {
        let settings = self.workspace.settings.clone();
        self.workspace_settings_tab = match settings.timing_mode.as_str() {
            "period" => WorkspaceTimingTab::Period,
            _ => WorkspaceTimingTab::Frequency,
        };
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
        // All plugins start stopped by default (behavior.loads_started is queried at runtime if needed)
        for plugin in &mut self.workspace.plugins {
            plugin.running = false;
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
        self.poll_plotter_screenshot_dialog();
        self.poll_logic_state();
        let mut plotter_refresh = 0.0;
        for plotter in self.plotters.values() {
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
        if self.workspace_dirty {
            let _ = self
                .logic_tx
                .send(LogicMessage::UpdateWorkspace(self.workspace.clone()));
            self.workspace_dirty = false;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.build_dialog_open && !self.build_dialog_in_progress {
                self.build_dialog_open = false;
            } else if self.confirm_dialog_open {
                self.confirm_dialog_open = false;
                self.confirm_action = None;
            }
        }
        self.window_rects.clear();

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Workspace", |ui| {
                    let label = if self.workspace_path.as_os_str().is_empty() {
                        "No Workspace loaded".to_string()
                    } else {
                        self.workspace.name.clone()
                    };
                    ui.add_enabled(
                        false,
                        egui::Label::new(RichText::new(label).color(egui::Color32::from_gray(230))),
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
                    if ui.button("Manage Workspaces").clicked() {
                        self.open_manage_workspaces();
                        ui.close_menu();
                    }
                    if ui.button("Settings").clicked() {
                        self.workspace_settings_open = true;
                        self.pending_window_focus = Some(WindowFocus::WorkspaceSettings);
                        ui.close_menu();
                    }
                });

                ui.menu_button("Plugins", |ui| {
                    if ui.button("Add plugins").clicked() {
                        self.open_plugins();
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
                        self.manage_connections_open = true;
                        self.pending_window_focus = Some(WindowFocus::ManageConnections);
                        ui.close_menu();
                    }
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(6.0);
                    ui.label(RichText::new(format!("RTSyn {}", env!("CARGO_PKG_VERSION"))).weak());
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

            // Connections panel removed for now.
        });

        self.render_workspace_dialog(ctx);
        self.render_load_workspaces_window(ctx);
        self.render_manage_workspaces_window(ctx);
        self.render_manage_plugins_window(ctx);
        self.render_plugins_window(ctx);
        self.render_manage_connections_window(ctx);
        self.render_connection_editor(ctx);
        self.render_plugin_context_menu(ctx);
        self.render_connection_context_menu(ctx);
        self.render_plugin_config_window(ctx);
        self.render_plotter_windows(ctx);
        self.render_workspace_settings_window(ctx);
        self.render_build_dialog(ctx);
        self.render_confirm_remove_dialog(ctx);
        self.render_info_dialog(ctx);
        self.render_plotter_preview_dialog(ctx);
    }
}
