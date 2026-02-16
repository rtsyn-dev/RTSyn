use eframe::{egui, egui::RichText};
use rtsyn_runtime::{LogicMessage, LogicState};
use rtsyn_runtime::spawn_runtime;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;
use workspace::ConnectionDefinition;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum HighlightMode {
    None,
    AllConnections(u64),
    SingleConnection(u64, u64),
}

// GuiApp implementation modules
mod app_impl;

// Core modules
mod daemon;
mod managers;
mod plotter;
mod state;
mod ui;
mod utils;

use managers::{FileDialogManager, NotificationHandler, PlotterManager, PluginBehaviorManager};
use utils::{distance_to_segment, has_rt_capabilities, spawn_file_dialog_thread, zenity_file_dialog, zenity_file_dialog_with_name};
use plotter::LivePlotter;
use rtsyn_core::plugin::PluginManager;
use rtsyn_core::workspace::WorkspaceManager;
use state::{
    ConfirmAction, ConnectionEditorHost, FrequencyUnit, PeriodUnit, StateSync,
    WorkspaceDialogMode, WorkspaceTimingTab,
};

const DEDICATED_PLOTTER_VIEW_KINDS: &[&str] = &["live_plotter"];

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
    default_value: String,
}

impl Default for PluginFieldDraft {
    fn default() -> Self {
        Self {
            name: String::new(),
            type_name: "f64".to_string(),
            default_value: "0.0".to_string(),
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
    supports_apply: bool,
    external_window: bool,
    starts_expanded: bool,
    required_input_ports_csv: String,
    required_output_ports_csv: String,
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
            supports_apply: false,
            external_window: false,
            starts_expanded: true,
            required_input_ports_csv: String::new(),
            required_output_ports_csv: String::new(),
            variables: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            internal_variables: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct BuildResult {
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

/// Initializes and runs the RTSyn GUI application with automatic runtime spawning.
/// 
/// This is the main entry point for the RTSyn GUI application. It handles two
/// execution modes: daemon plugin viewer mode and normal application mode.
/// In normal mode, it spawns the logic runtime and initializes the GUI.
/// 
/// # Parameters
/// 
/// * `config` - GUI configuration specifying window title, dimensions, and other settings
/// 
/// # Returns
/// 
/// * `Ok(())` - GUI application completed successfully
/// * `Err(GuiError)` - GUI initialization or runtime error occurred
/// 
/// # Execution Modes
/// 
/// ## Daemon Plugin Viewer Mode
/// Activated when environment variables are set:
/// - `RTSYN_DAEMON_VIEW_PLUGIN_ID` - Plugin ID to view
/// - `RTSYN_DAEMON_SOCKET` - Socket path (defaults to "/tmp/rtsyn-daemon.sock")
/// 
/// In this mode, the GUI connects to an existing daemon process to view a specific
/// plugin's interface rather than running a full application instance.
/// 
/// ## Normal Application Mode
/// 1. Spawns the logic runtime using `spawn_runtime()`
/// 2. Creates communication channels between GUI and runtime
/// 3. Delegates to `run_gui_with_runtime()` for GUI initialization
/// 
/// # Error Handling
/// 
/// - Runtime spawn failures cause immediate process termination with error message
/// - GUI initialization errors are propagated as `GuiError::Gui`
/// - Environment variable parsing errors fall back to normal mode
/// 
/// # Side Effects
/// 
/// - May spawn background runtime threads
/// - Creates GUI window and event loop
/// - In daemon mode, establishes socket connection to existing daemon
/// - On runtime failure, prints error and calls `process::exit(1)`
pub fn run_gui(config: GuiConfig) -> Result<(), GuiError> {
    if let Ok(id_str) = std::env::var("RTSYN_DAEMON_VIEW_PLUGIN_ID") {
        if let Ok(plugin_id) = id_str.parse::<u64>() {
            let socket_path = std::env::var("RTSYN_DAEMON_SOCKET")
                .unwrap_or_else(|_| "/tmp/rtsyn-daemon.sock".to_string());
            return daemon::run_daemon_plugin_viewer(config, plugin_id, socket_path);
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

/// Runs the RTSyn GUI application with pre-existing runtime communication channels.
/// 
/// This function initializes and runs the eframe-based GUI application using provided
/// communication channels to an already-running logic runtime. It configures the
/// GUI framework, sets up fonts, and creates the main application instance.
/// 
/// # Parameters
/// 
/// * `config` - GUI configuration containing window title, dimensions, and display settings
/// * `logic_tx` - Sender channel for sending messages to the logic runtime
/// * `logic_state_rx` - Receiver channel for receiving state updates from the logic runtime
/// 
/// # Returns
/// 
/// * `Ok(())` - GUI application completed successfully (user closed window)
/// * `Err(GuiError::Gui)` - eframe initialization or runtime error occurred
/// 
/// # GUI Framework Setup
/// 
/// 1. **Window Configuration**: Creates native window with specified dimensions
/// 2. **VSync Disabled**: Prevents hangs and lag on occluded windows
/// 3. **Font Setup**: Loads FontAwesome icons for UI elements
/// 4. **Application Creation**: Instantiates `GuiApp` with runtime channels
/// 
/// # Font Configuration
/// 
/// Embeds and configures FontAwesome solid icons (fa-solid-900.ttf) for use in
/// buttons and UI elements. The font is added to the proportional font family
/// to enable icon rendering alongside text.
/// 
/// # Application Lifecycle
/// 
/// - Creates eframe native options with custom viewport settings
/// - Disables VSync to prevent performance issues with window occlusion
/// - Sets up font definitions including embedded FontAwesome icons
/// - Instantiates GuiApp with runtime communication channels
/// - Runs the event loop until application termination
/// 
/// # Error Propagation
/// 
/// eframe errors are wrapped in `GuiError::Gui` and propagated to the caller.
/// The error message includes the original eframe error description.
pub fn run_gui_with_runtime(
    config: GuiConfig,
    logic_tx: Sender<LogicMessage>,
    logic_state_rx: Receiver<LogicState>,
) -> Result<(), GuiError> {
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([config.width, config.height])
            .with_maximized(true),
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
    notification_handler: NotificationHandler,
    behavior_manager: PluginBehaviorManager,

    // UI State Groups
    plotter_preview: state::PlotterPreviewState,
    connection_editor: state::ConnectionEditorState,
    workspace_dialog: state::WorkspaceDialogState,
    build_dialog: state::BuildDialogState,
    confirm_dialog: state::ConfirmDialogState,
    workspace_settings: state::WorkspaceSettingsState,
    help_state: state::HelpState,
    windows: state::WindowState,

    // Remaining UI State
    status: String,
    csv_path_target_plugin_id: Option<u64>,
    plugin_creator_last_path: Option<PathBuf>,
    new_plugin_draft: NewPluginDraft,
    seen_compatibility_warnings: HashSet<String>,
    plugin_positions: HashMap<u64, egui::Pos2>,
    plugin_rects: HashMap<u64, egui::Rect>,
    connections_view_enabled: bool,
    connection_clicked_this_frame: bool,
    available_cores: usize,
    selected_cores: Vec<bool>,
    frequency_value: f64,
    frequency_unit: FrequencyUnit,
    period_value: f64,
    period_unit: PeriodUnit,
    output_refresh_hz: f64,
    plotter_screenshot_target: Option<u64>,
    connection_highlight_plugin_id: Option<u64>,
    highlight_mode: HighlightMode,
    pending_highlight: Option<HighlightMode>,
    plugin_context_menu: Option<(u64, egui::Pos2, u64)>,
    connection_context_menu: Option<(Vec<ConnectionDefinition>, egui::Pos2, u64)>,
    connection_editor_host: ConnectionEditorHost,
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
}

impl eframe::App for GuiApp {
    /// Main GUI update loop called by eframe for each frame.
    /// 
    /// This method implements the core GUI update cycle, handling user input,
    /// processing runtime state updates, managing UI components, and rendering
    /// the complete application interface.
    /// 
    /// # Parameters
    /// 
    /// * `ctx` - egui context providing input handling and rendering capabilities
    /// * `_frame` - eframe frame reference (unused in current implementation)
    /// 
    /// # Update Cycle Overview
    /// 
    /// ## 1. Style Configuration
    /// - Disables selectable labels to prevent unwanted text selection
    /// - Configures UI interaction behavior
    /// 
    /// ## 2. Dialog Polling
    /// - Polls all asynchronous file dialogs for completion
    /// - Handles build, install, import, load, export operations
    /// - Processes CSV path selection and plugin creation dialogs
    /// - Updates plotter screenshot operations
    /// 
    /// ## 3. Runtime State Processing
    /// - Polls logic runtime for state updates via `poll_logic_state()`
    /// - Updates plotter displays with new data
    /// - Synchronizes GUI state with runtime state
    /// 
    /// ## 4. Refresh Rate Management
    /// - Calculates optimal refresh rate based on active plotters
    /// - Requests appropriate repaint timing from egui
    /// - Balances responsiveness with performance
    /// 
    /// ## 5. Workspace Synchronization
    /// - Sends workspace updates to runtime when dirty flag is set
    /// - Ensures runtime has current workspace configuration
    /// - Clears dirty flag after successful synchronization
    /// 
    /// ## 6. Input Handling
    /// - Processes Escape key for dialog dismissal
    /// - Handles global keyboard shortcuts
    /// - Manages dialog state transitions
    /// 
    /// ## 7. UI Rendering
    /// - Renders top menu bar with workspace, plugin, and runtime menus
    /// - Displays main central panel with plugin cards and connections
    /// - Shows all active dialogs and windows
    /// - Handles context menus and popup interactions
    /// 
    /// # Refresh Rate Strategy
    /// 
    /// ## Active Plotter Mode
    /// - Uses maximum refresh rate from open plotters (minimum 1 Hz)
    /// - Ensures smooth real-time data visualization
    /// 
    /// ## Idle Mode
    /// - Uses 250ms refresh interval when window is not focused
    /// - Reduces CPU usage when application is in background
    /// 
    /// # Dialog Management
    /// 
    /// Renders all possible dialogs and windows:
    /// - Workspace management dialogs
    /// - Plugin installation and configuration windows
    /// - Connection editor and management interfaces
    /// - Plotter preview and configuration dialogs
    /// - Help and information displays
    /// - Confirmation and notification overlays
    /// 
    /// # State Management
    /// 
    /// - Maintains window rectangle tracking for layout management
    /// - Handles pending window focus requests
    /// - Manages plugin selection and context menu state
    /// - Coordinates between different UI components
    /// 
    /// # Performance Considerations
    /// 
    /// - Non-blocking runtime communication prevents GUI freezing
    /// - Adaptive refresh rates optimize CPU usage
    /// - Efficient dialog polling minimizes overhead
    /// - Smart repaint requests reduce unnecessary rendering
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
            
            // Reset connection click flag at start of frame
            self.connection_clicked_this_frame = false;
            
            // Three-phase rendering when highlight mode is active
            if !matches!(self.highlight_mode, HighlightMode::None) {
                // Phase 1: Render non-connected plugins
                self.render_plugin_cards_filtered(ctx, panel_rect, Some(false));
                // Phase 2: Render connections (now on Middle layer via Area)
                self.render_connection_view(ctx, panel_rect);
                // Phase 3: Render connected plugins
                self.render_plugin_cards_filtered(ctx, panel_rect, Some(true));
            } else {
                // Normal rendering
                self.render_connection_view(ctx, panel_rect);
                self.render_plugin_cards(ctx, panel_rect);
            }
            if ctx.input(|i| i.pointer.primary_clicked()) {
                if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                    let over_plugin = self.plugin_rects.values().any(|rect| rect.contains(pos));
                    // Check if over connection by testing distance to any connection
                    let over_connection = if !over_plugin {
                        self.workspace_manager.workspace.connections.iter().any(|conn| {
                            if let (Some(from_rect), Some(to_rect)) = (
                                self.plugin_rects.get(&conn.from_plugin),
                                self.plugin_rects.get(&conn.to_plugin)
                            ) {
                                let start = from_rect.center();
                                let end = to_rect.center();
                                distance_to_segment(pos, start, end) <= 10.0
                            } else {
                                false
                            }
                        })
                    } else {
                        false
                    };
                    if !over_plugin && !over_connection && !self.connection_clicked_this_frame {
                        self.highlight_mode = HighlightMode::None;
                    }
                }
            }
        });
        
        // Apply pending highlight at end of frame (after rendering)
        if let Some(pending) = self.pending_highlight.take() {
            self.highlight_mode = pending;
        }

        self.render_workspace_dialog(ctx);
        self.render_load_workspaces_window(ctx);
        self.render_manage_workspaces_window(ctx);
        self.render_manage_plugins_window(ctx);
        self.render_install_plugins_window(ctx);
        self.render_uninstall_plugins_window(ctx);
        self.render_plugins_window(ctx);
        self.render_new_plugin_window(ctx);
        self.render_manage_connections_window(ctx);
        if self.connection_editor_host == ConnectionEditorHost::Main {
            self.render_connection_editor(ctx);
        }
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
