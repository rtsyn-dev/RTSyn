use eframe::{egui, egui::RichText};
use rtsyn_runtime::{LogicMessage, LogicSettings, LogicState};
use rtsyn_runtime::spawn_runtime;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use workspace::WorkspaceDefinition;
use workspace::{input_sum, input_sum_any, ConnectionDefinition, WorkspaceSettings};

// Operation modules
mod connection_operations;
mod dialog_polling;
mod helpers;
mod plugin_operations;
mod workspace_operations;

// Core modules
mod daemon_viewer;
mod file_dialogs;
mod formatting;
mod notification_handler;
mod notifications;
mod plotter;
mod plotter_manager;
mod plugin_behavior_manager;
mod state;
mod state_sync;
mod ui;
mod ui_state;
mod utils;
mod workspace_manager;

use file_dialogs::FileDialogManager;
use helpers::{has_rt_capabilities, spawn_file_dialog_thread, zenity_file_dialog, zenity_file_dialog_with_name};
use notification_handler::NotificationHandler;
use plotter::LivePlotter;
use plotter_manager::PlotterManager;
use plugin_behavior_manager::PluginBehaviorManager;
use rtsyn_core::plotter_view::{live_plotter_config, live_plotter_series_names};
use rtsyn_core::plugin::{plugin_display_name as core_plugin_display_name, PluginManager};
use rtsyn_core::workspace::WorkspaceManager;
use state::{
    ConfirmAction, ConnectionEditorHost, FrequencyUnit, PeriodUnit, TimeUnit, WorkspaceDialogMode,
    WorkspaceTimingTab,
};
use state_sync::StateSync;

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
    notification_handler: NotificationHandler,
    behavior_manager: PluginBehaviorManager,

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
    seen_compatibility_warnings: HashSet<String>,
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
    /// Creates a new GuiApp instance with runtime communication channels.
    /// 
    /// This constructor initializes all application state, managers, and UI components
    /// required for the RTSyn GUI. It establishes communication with the logic runtime,
    /// sets up plugin and workspace management, and configures initial application state.
    /// 
    /// # Parameters
    /// 
    /// * `logic_tx` - Sender channel for communicating with the logic runtime
    /// * `logic_state_rx` - Receiver channel for runtime state updates
    /// 
    /// # Returns
    /// 
    /// A fully initialized `GuiApp` instance ready for use with eframe.
    /// 
    /// # Initialization Process
    /// 
    /// ## Manager Setup
    /// 1. **PluginManager**: Manages installed plugins and library paths
    /// 2. **WorkspaceManager**: Handles workspace loading/saving and current workspace state
    /// 3. **FileDialogManager**: Manages asynchronous file dialog operations
    /// 4. **PlotterManager**: Coordinates live plotting windows and data
    /// 5. **StateSync**: Synchronizes state between GUI and logic runtime
    /// 6. **NotificationHandler**: Manages user notifications and messages
    /// 7. **PluginBehaviorManager**: Caches plugin behavior and capabilities
    /// 
    /// ## State Initialization
    /// - Detects available CPU cores for runtime configuration
    /// - Initializes UI state groups for different dialog and window types
    /// - Sets up default timing parameters (1000 Hz frequency, 1ms period)
    /// - Configures core selection (defaults to core 0 if available)
    /// - Initializes empty collections for plugin positions, connections, etc.
    /// 
    /// ## Plugin Library Integration
    /// - Refreshes plugin library paths from installed plugins
    /// - Updates workspace plugins with current library paths
    /// - Ensures plugin configurations include library_path entries
    /// 
    /// ## Post-Initialization Tasks
    /// - Processes and displays plugin compatibility warnings
    /// - Refreshes installed plugin metadata cache
    /// - Applies current workspace settings to runtime
    /// - Synchronizes plugin ID generation with existing workspace
    /// 
    /// # Side Effects
    /// 
    /// - Modifies plugin configurations to include library paths
    /// - Displays compatibility warnings as notifications
    /// - Sends initial settings to logic runtime
    /// - May trigger plugin behavior caching for installed plugins
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
        let notification_handler = NotificationHandler::new();
        let behavior_manager = PluginBehaviorManager::new();

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
            notification_handler,
            behavior_manager,
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
            seen_compatibility_warnings: HashSet::new(),
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
            connection_editor_host: ConnectionEditorHost::Main,
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
        for warning in app.plugin_manager.take_compatibility_warnings() {
            app.show_info("Plugin Compatibility", &warning);
            app.seen_compatibility_warnings.insert(warning);
        }
        app.refresh_installed_plugin_metadata_cache();
        app.apply_workspace_settings();
        app
    }

    /// Calculates the center position for a window of given size within the available screen area.
    /// 
    /// This utility function computes the top-left corner position needed to center a window
    /// of specified dimensions within the current egui context's available rectangle.
    /// 
    /// # Parameters
    /// 
    /// * `ctx` - egui context providing screen dimensions and available area
    /// * `size` - Desired window size as Vec2 (width, height)
    /// 
    /// # Returns
    /// 
    /// * `egui::Pos2` - Top-left corner position to center the window
    /// 
    /// # Calculation
    /// 
    /// 1. Gets the available rectangle from the egui context
    /// 2. Finds the center point of the available area
    /// 3. Subtracts half the window size to get the top-left corner
    /// 
    /// # Usage
    /// 
    /// Typically used when opening modal dialogs or popup windows to ensure
    /// they appear centered on screen regardless of the main window size.
    /// 
    /// ```rust
    /// let window_size = egui::vec2(400.0, 300.0);
    /// let center_pos = GuiApp::center_window(ctx, window_size);
    /// egui::Window::new("Dialog")
    ///     .default_pos(center_pos)
    ///     .show(ctx, |ui| { /* window content */ });
    /// ```
    fn center_window(ctx: &egui::Context, size: egui::Vec2) -> egui::Pos2 {
        let rect = ctx.available_rect();
        let center = rect.center();
        center - size * 0.5
    }

    /// Synchronizes the plugin manager's next ID counter with the current workspace.
    /// 
    /// This method ensures that newly created plugins receive unique IDs by updating
    /// the plugin manager's internal counter to be higher than any existing plugin ID
    /// in the current workspace.
    /// 
    /// # Purpose
    /// 
    /// Prevents ID collisions when:
    /// - Loading workspaces with existing plugins
    /// - Adding new plugins to workspaces with gaps in ID sequences
    /// - Ensuring consistent ID generation across application sessions
    /// 
    /// # Implementation
    /// 
    /// 1. Finds the maximum plugin ID currently used in the workspace
    /// 2. Updates the plugin manager's next_id counter accordingly
    /// 3. Ensures future plugin creation uses non-conflicting IDs
    /// 
    /// # Side Effects
    /// 
    /// - Modifies the plugin manager's internal ID generation state
    /// - May skip ID numbers to avoid conflicts with existing plugins
    /// 
    /// # Usage Context
    /// 
    /// Called during:
    /// - Workspace loading operations
    /// - Application initialization
    /// - Plugin management operations that might affect ID sequences
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

    /// Marks the current workspace as modified and needing to be saved.
    /// 
    /// This method sets the workspace dirty flag, indicating that changes have been made
    /// to the workspace configuration that should be persisted to disk. The dirty flag
    /// is used to trigger automatic saving and to warn users about unsaved changes.
    /// 
    /// # Purpose
    /// 
    /// - Tracks workspace modifications for save operations
    /// - Enables automatic workspace synchronization with runtime
    /// - Provides user feedback about unsaved changes
    /// 
    /// # Triggers Automatic Actions
    /// 
    /// When the workspace is marked dirty:
    /// 1. The main update loop detects the dirty flag
    /// 2. Workspace changes are sent to the logic runtime
    /// 3. The dirty flag is cleared after successful synchronization
    /// 
    /// # Usage Context
    /// 
    /// Called whenever workspace state changes:
    /// - Adding/removing plugins
    /// - Modifying plugin configurations
    /// - Changing connections between plugins
    /// - Updating workspace settings
    /// - Modifying plugin positions or properties
    /// 
    /// # Side Effects
    /// 
    /// - Sets `workspace_manager.workspace_dirty = true`
    /// - Triggers workspace synchronization in next update cycle
    fn mark_workspace_dirty(&mut self) {
        self.workspace_manager.mark_dirty();
    }

    /// Sends a restart command to the logic runtime for the specified plugin.
    /// 
    /// This method requests that the logic runtime restart a specific plugin,
    /// which involves stopping the plugin's execution, cleaning up its state,
    /// and then starting it again with its current configuration.
    /// 
    /// # Parameters
    /// 
    /// * `plugin_id` - Unique identifier of the plugin to restart
    /// 
    /// # Runtime Communication
    /// 
    /// Sends a `LogicMessage::RestartPlugin(plugin_id)` message to the logic runtime
    /// via the `logic_tx` channel. The runtime handles the actual restart process
    /// asynchronously.
    /// 
    /// # Error Handling
    /// 
    /// Channel send errors are silently ignored using `let _ = ...` pattern.
    /// This prevents GUI crashes if the runtime has terminated or the channel
    /// is disconnected.
    /// 
    /// # Plugin Restart Process (Runtime Side)
    /// 
    /// 1. **Stop Phase**: Gracefully stops the plugin's execution
    /// 2. **Cleanup Phase**: Releases plugin resources and clears state
    /// 3. **Reload Phase**: Reloads plugin library and configuration
    /// 4. **Start Phase**: Initializes and starts the plugin with current config
    /// 
    /// # Usage Context
    /// 
    /// Called when:
    /// - User clicks restart button in plugin UI
    /// - Plugin configuration changes require restart
    /// - Plugin encounters errors and needs recovery
    /// - Plugin library is updated and needs reloading
    /// 
    /// # Asynchronous Operation
    /// 
    /// The restart operation is asynchronous - this method returns immediately
    /// while the actual restart happens in the background runtime.
    fn restart_plugin(&mut self, plugin_id: u64) {
        let _ = self
            .state_sync
            .logic_tx
            .send(LogicMessage::RestartPlugin(plugin_id));
    }

    /// Converts a plugin kind identifier to a human-readable display name.
    /// 
    /// This method transforms internal plugin type identifiers (typically snake_case)
    /// into user-friendly display names suitable for UI presentation.
    /// 
    /// # Parameters
    /// 
    /// * `kind` - Internal plugin kind identifier (e.g., "live_plotter", "csv_recorder")
    /// 
    /// # Returns
    /// 
    /// * `String` - Human-readable display name for the plugin kind
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// assert_eq!(GuiApp::display_kind("live_plotter"), "Live Plotter");
    /// assert_eq!(GuiApp::display_kind("csv_recorder"), "CSV Recorder");
    /// assert_eq!(GuiApp::display_kind("pid_controller"), "PID Controller");
    /// ```
    /// 
    /// # Implementation
    /// 
    /// Delegates to `PluginManager::display_kind()` which handles the actual
    /// transformation logic, typically:
    /// - Converting snake_case to Title Case
    /// - Expanding abbreviations (csv -> CSV)
    /// - Adding proper spacing and capitalization
    /// 
    /// # Usage Context
    /// 
    /// Used throughout the UI for:
    /// - Plugin selection dialogs
    /// - Plugin card titles
    /// - Menu items and tooltips
    /// - Error messages and notifications
    fn display_kind(kind: &str) -> String {
        PluginManager::display_kind(kind)
    }

    /// Displays a general information notification to the user.
    /// 
    /// This method creates and displays a non-modal information dialog with the
    /// specified title and message. The notification appears as an overlay that
    /// the user can dismiss.
    /// 
    /// # Parameters
    /// 
    /// * `title` - Title text displayed in the notification header
    /// * `message` - Main message content to display to the user
    /// 
    /// # Notification Behavior
    /// 
    /// - **Non-blocking**: Does not prevent user interaction with the main UI
    /// - **Dismissible**: User can close the notification manually
    /// - **Overlay**: Appears on top of the main application window
    /// - **General scope**: Not associated with any specific plugin
    /// 
    /// # Usage Context
    /// 
    /// Appropriate for:
    /// - Workspace operation results (save/load success/failure)
    /// - Plugin installation/uninstallation status
    /// - General application status updates
    /// - Compatibility warnings
    /// - File operation results
    /// 
    /// # Implementation
    /// 
    /// Delegates to `NotificationHandler::show_info()` which manages the
    /// notification queue and display logic.
    /// 
    /// # Example Usage
    /// 
    /// ```rust
    /// self.show_info("Workspace", "Workspace saved successfully");
    /// self.show_info("Plugin Error", "Failed to load plugin library");
    /// self.show_info("Export Complete", "UML diagram exported to file");
    /// ```
    fn show_info(&mut self, title: &str, message: &str) {
        self.notification_handler.show_info(title, message);
    }

    /// Displays a plugin-specific information notification to the user.
    /// 
    /// This method creates and displays an information notification that is
    /// associated with a specific plugin. The notification includes context
    /// about which plugin generated the message.
    /// 
    /// # Parameters
    /// 
    /// * `plugin_id` - Unique identifier of the plugin associated with this notification
    /// * `title` - Title text displayed in the notification header
    /// * `message` - Main message content to display to the user
    /// 
    /// # Plugin Context
    /// 
    /// The notification system uses the plugin_id to:
    /// - Associate the message with a specific plugin instance
    /// - Potentially group related notifications
    /// - Provide context for debugging and troubleshooting
    /// - Enable plugin-specific notification filtering or handling
    /// 
    /// # Usage Context
    /// 
    /// Appropriate for:
    /// - Plugin-specific error messages
    /// - Plugin state change notifications
    /// - Plugin configuration validation results
    /// - Plugin execution status updates
    /// - Plugin-generated warnings or alerts
    /// 
    /// # Implementation
    /// 
    /// Delegates to `NotificationHandler::show_plugin_info()` which handles
    /// plugin-specific notification logic and may include additional context
    /// such as plugin name or type in the display.
    /// 
    /// # Example Usage
    /// 
    /// ```rust
    /// self.show_plugin_info(plugin_id, "Configuration", "Invalid parameter value");
    /// self.show_plugin_info(plugin_id, "Status", "Plugin started successfully");
    /// self.show_plugin_info(plugin_id, "Warning", "Input signal out of range");
    /// ```
    fn show_plugin_info(&mut self, plugin_id: u64, title: &str, message: &str) {
        self.notification_handler.show_plugin_info(plugin_id, title, message);
    }

    /// Displays a confirmation dialog requiring user action before proceeding.
    /// 
    /// This method creates a modal confirmation dialog that blocks user interaction
    /// with the main UI until the user either confirms or cancels the action.
    /// The dialog presents a clear choice between proceeding with a potentially
    /// destructive or significant operation and canceling it.
    /// 
    /// # Parameters
    /// 
    /// * `title` - Title text displayed in the dialog header
    /// * `message` - Detailed message explaining what will happen if confirmed
    /// * `action_label` - Text for the confirmation button (e.g., "Delete", "Remove")
    /// * `action` - The specific action to perform if user confirms
    /// 
    /// # Dialog Behavior
    /// 
    /// - **Modal**: Blocks interaction with main UI until dismissed
    /// - **Two choices**: User can confirm (perform action) or cancel
    /// - **Persistent**: Remains open until user makes a choice
    /// - **Action-specific**: Button text reflects the specific operation
    /// 
    /// # Confirmation Actions
    /// 
    /// The `action` parameter specifies what happens when confirmed:
    /// - `ConfirmAction::RemovePlugin(id)` - Remove plugin from workspace
    /// - `ConfirmAction::UninstallPlugin(index)` - Uninstall plugin from system
    /// - `ConfirmAction::DeleteWorkspace(path)` - Delete workspace file
    /// 
    /// # Usage Context
    /// 
    /// Used for potentially destructive operations:
    /// - Deleting workspaces or plugins
    /// - Uninstalling plugins
    /// - Removing connections
    /// - Clearing data or resetting state
    /// 
    /// # Example Usage
    /// 
    /// ```rust
    /// self.show_confirm(
    ///     "Delete Plugin",
    ///     "This will permanently remove the plugin from the workspace.",
    ///     "Delete",
    ///     ConfirmAction::RemovePlugin(plugin_id)
    /// );
    /// ```
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

    /// Executes the confirmed action after user approval in a confirmation dialog.
    /// 
    /// This method is called when the user clicks the confirmation button in a
    /// confirmation dialog. It performs the actual operation that was being
    /// confirmed, handling different types of actions appropriately.
    /// 
    /// # Parameters
    /// 
    /// * `action` - The specific action to perform, as confirmed by the user
    /// 
    /// # Supported Actions
    /// 
    /// ## RemovePlugin(plugin_id)
    /// - Finds the plugin in the current workspace by ID
    /// - Removes it from the workspace plugin list
    /// - Updates workspace state and marks as dirty
    /// 
    /// ## UninstallPlugin(index)
    /// - Removes the plugin from the system installation
    /// - Deletes plugin files and metadata
    /// - Updates the installed plugins database
    /// 
    /// ## DeleteWorkspace(path)
    /// - Loads workspace metadata to get the display name
    /// - Deletes the workspace file from disk
    /// - Clears current workspace if it was the deleted one
    /// - Refreshes the workspace list
    /// - Shows success/failure notification
    /// 
    /// # Error Handling
    /// 
    /// - Plugin removal: Silently handles missing plugins
    /// - Workspace deletion: Shows error notifications for failures
    /// - Workspace name extraction: Falls back to filename if loading fails
    /// 
    /// # Side Effects
    /// 
    /// - May modify workspace state and trigger saves
    /// - May clear plotters and reset UI state
    /// - May delete files from disk
    /// - Shows notifications to inform user of results
    /// - Refreshes relevant UI lists and displays
    /// 
    /// # Post-Action Cleanup
    /// 
    /// After workspace deletion:
    /// - Clears plotter windows if no workspace is loaded
    /// - Reapplies workspace settings
    /// - Clears plugin position cache
    /// - Rescans available workspaces
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

    /// Polls and processes state updates from the logic runtime.
    /// 
    /// This method is called every frame to receive and process state updates from
    /// the logic runtime. It handles real-time data flow, plotter updates, and
    /// maintains synchronized state between the GUI and runtime components.
    /// 
    /// # Runtime Communication Flow
    /// 
    /// 1. **Non-blocking Poll**: Uses `try_recv()` to avoid blocking the GUI thread
    /// 2. **State Merging**: Combines multiple state updates received since last poll
    /// 3. **Sample Aggregation**: Merges plotter samples from multiple updates
    /// 4. **Latest State**: Keeps only the most recent state for current values
    /// 
    /// # Data Processing
    /// 
    /// ## Plotter Sample Merging
    /// - Collects samples from all received state updates
    /// - Groups samples by plugin ID
    /// - Preserves chronological order for accurate plotting
    /// 
    /// ## State Update Handling
    /// - Extracts output values, input values, and internal variables
    /// - Updates plotter displays with new sample data
    /// - Applies output refresh rate limiting for performance
    /// 
    /// # Performance Optimization
    /// 
    /// ## Output Refresh Rate Limiting
    /// - Limits GUI updates based on `output_refresh_hz` setting
    /// - Prevents excessive UI updates that could impact performance
    /// - Maintains smooth real-time display without overwhelming the GUI
    /// 
    /// ## Running Plugin Filtering
    /// - Filters displayed values to only include running plugins
    /// - Reduces UI clutter and improves performance
    /// - Ensures stopped plugins don't show stale data
    /// 
    /// # State Synchronization
    /// 
    /// Updates the following synchronized state:
    /// - `computed_outputs`: Current output values from running plugins
    /// - `input_values`: Current input values for running plugins  
    /// - `internal_variable_values`: Internal plugin state variables
    /// - `viewer_values`: Values for plugin viewer displays
    /// - `last_output_update`: Timestamp for rate limiting
    /// 
    /// # Plotter Integration
    /// 
    /// - Calls `update_plotters()` with merged sample data
    /// - Handles real-time plotting updates
    /// - Manages plotter refresh rates and display optimization
    /// 
    /// # Thread Safety
    /// 
    /// This method is designed to be called from the main GUI thread and uses
    /// non-blocking channel operations to avoid interfering with real-time
    /// runtime operations.
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

    /// Retrieves the available input or output ports for a specific plugin kind.
    /// 
    /// This method looks up the port definitions for a given plugin type from
    /// the installed plugin metadata. It provides the list of available ports
    /// that can be used for connections to/from plugins of this type.
    /// 
    /// # Parameters
    /// 
    /// * `kind` - Plugin type identifier (e.g., "live_plotter", "pid_controller")
    /// * `inputs` - If true, returns input ports; if false, returns output ports
    /// 
    /// # Returns
    /// 
    /// * `Vec<String>` - List of available port names for the specified plugin kind
    /// * Empty vector if plugin kind is not found or has no ports of the requested type
    /// 
    /// # Port Discovery Process
    /// 
    /// 1. Searches installed plugins for matching plugin kind
    /// 2. Retrieves cached metadata for input or output ports
    /// 3. Returns the appropriate port list based on the `inputs` parameter
    /// 
    /// # Metadata Source
    /// 
    /// Port information comes from:
    /// - Plugin manifest files
    /// - Cached plugin behavior analysis
    /// - Runtime plugin introspection
    /// 
    /// # Usage Context
    /// 
    /// Used for:
    /// - Connection editor port selection
    /// - Validation of connection endpoints
    /// - UI generation for plugin configuration
    /// - Automatic connection suggestions
    /// 
    /// # Example Usage
    /// 
    /// ```rust
    /// let inputs = self.ports_for_kind("pid_controller", true);
    /// // Returns: ["setpoint", "process_variable", "enable"]
    /// 
    /// let outputs = self.ports_for_kind("pid_controller", false);  
    /// // Returns: ["control_output", "error"]
    /// ```
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
        if let Some(cached) = self.behavior_manager.cached_behaviors.get(kind) {
            return matches!(
                cached.extendable_inputs,
                rtsyn_plugin::ui::ExtendableInputs::Auto { .. }
                    | rtsyn_plugin::ui::ExtendableInputs::Manual
            );
        }
        rtsyn_core::plugin::is_extendable_inputs(kind)
    }

    fn plugin_uses_external_window(&self, kind: &str) -> bool {
        self.behavior_manager
            .cached_behaviors
            .get(kind)
            .map(|b| b.external_window)
            .unwrap_or(false)
    }

    fn plugin_uses_plotter_viewport(&self, kind: &str) -> bool {
        self.plugin_uses_external_window(kind) && DEDICATED_PLOTTER_VIEW_KINDS.contains(&kind)
    }

    fn plugin_uses_external_config_viewport(&self, kind: &str) -> bool {
        self.plugin_uses_external_window(kind) && !self.plugin_uses_plotter_viewport(kind)
    }

    fn auto_extend_inputs(&self, kind: &str) -> Vec<String> {
        if let Some(cached) = self.behavior_manager.cached_behaviors.get(kind) {
            if matches!(
                cached.extendable_inputs,
                rtsyn_plugin::ui::ExtendableInputs::Auto { .. }
            ) {
                return (1..=10).map(|i| format!("in_{}", i)).collect();
            }
        }
        if matches!(kind, "csv_recorder" | "live_plotter") {
            (1..=10).map(|i| format!("in_{}", i)).collect()
        } else {
            Vec::new()
        }
    }

    fn ensure_plugin_behavior_cached_with_path(
        &mut self,
        kind: &str,
        library_path: Option<&PathBuf>,
    ) {
        let path_str = library_path.map(|p| p.to_string_lossy().to_string());
        self.behavior_manager.ensure_behavior_cached(
            kind,
            path_str.as_deref(),
            &self.state_sync.logic_tx,
            &self.plugin_manager,
        );
    }

    fn ensure_plugin_behavior_cached(&mut self, kind: &str) {
        self.behavior_manager.ensure_behavior_cached(
            kind,
            None,
            &self.state_sync.logic_tx,
            &self.plugin_manager,
        );
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

    /// Generates a default CSV file path with timestamp for data recording.
    /// 
    /// This method creates a default file path for CSV data recording that includes
    /// a timestamp to ensure unique filenames and organized data storage.
    /// 
    /// # Returns
    /// 
    /// * `String` - Complete file path for CSV recording with timestamp
    /// 
    /// # Path Generation Strategy
    /// 
    /// ## Base Directory
    /// - Uses `$HOME/rtsyn-recorded` if HOME environment variable is available
    /// - Falls back to `./rtsyn-recorded` in current directory
    /// 
    /// ## Timestamp Format
    /// - Uses Unix timestamp converted to day-hour-minute-second format
    /// - Format: `{day}-{hour:02}-{minute:02}-{second:02}.csv`
    /// - Day counter starts from Unix epoch (days since 1970-01-01)
    /// - Hours, minutes, seconds are zero-padded to 2 digits
    /// 
    /// # Example Output
    /// 
    /// ```
    /// /home/user/rtsyn-recorded/19724-14-30-45.csv
    /// ```
    /// 
    /// This represents:
    /// - Day 19724 since Unix epoch
    /// - 14:30:45 (2:30:45 PM)
    /// 
    /// # Usage Context
    /// 
    /// Used as default filename for:
    /// - CSV recorder plugin configuration
    /// - Data export operations
    /// - Automatic recording session naming
    /// - File dialog default suggestions
    /// 
    /// # Benefits
    /// 
    /// - **Unique filenames**: Timestamp prevents overwrites
    /// - **Chronological ordering**: Files sort naturally by creation time
    /// - **Organized storage**: Dedicated directory for recorded data
    /// - **Cross-platform**: Works on Unix-like systems and Windows
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

    fn plugin_display_name(&self, plugin_id: u64) -> String {
        core_plugin_display_name(
            &self.plugin_manager.installed_plugins,
            &self.workspace_manager.workspace,
            plugin_id,
        )
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

    /// Updates all live plotter instances with new sample data and manages plotter lifecycle.
    /// 
    /// This method processes real-time data for all live plotter plugins, updating their
    /// displays with new samples and managing their configuration. It also handles
    /// plotter lifecycle management and UI refresh rate optimization.
    /// 
    /// # Parameters
    /// 
    /// * `tick` - Current runtime tick counter for timestamp calculation
    /// * `outputs` - Current output values from all plugins
    /// * `samples` - New sample data grouped by plugin ID with tick timestamps
    /// 
    /// # Plotter Processing Pipeline
    /// 
    /// ## 1. Plugin Discovery
    /// - Identifies all "live_plotter" plugins in the workspace
    /// - Tracks active plotter IDs for lifecycle management
    /// 
    /// ## 2. Configuration Update
    /// - Extracts plotter configuration (input count, refresh rate, window size)
    /// - Applies preview settings overrides if available
    /// - Updates series names based on connection topology
    /// 
    /// ## 3. Data Processing
    /// - Calculates input values for each plotter based on connections
    /// - Processes sample data with intelligent decimation for performance
    /// - Updates plotter displays with new data points
    /// 
    /// # Sample Decimation Strategy
    /// 
    /// To maintain performance with high-frequency data:
    /// 
    /// ## Budget-Based Selection
    /// - Open plotters: 8192 sample budget for detailed display
    /// - Closed plotters: 1024 sample budget for background processing
    /// 
    /// ## Extrema Preservation
    /// - Preserves minimum and maximum values in each chunk
    /// - Prevents loss of important signal features (spikes, peaks)
    /// - Maintains visual accuracy while reducing data volume
    /// 
    /// ## Chronological Ordering
    /// - Maintains sample order for accurate time-series display
    /// - Removes duplicates while preserving temporal relationships
    /// 
    /// # Plotter State Management
    /// 
    /// ## Window Size Configuration
    /// - Sets effective window size before configuration updates
    /// - Handles preview settings overrides
    /// - Ensures minimum window size for stability
    /// 
    /// ## Refresh Rate Tracking
    /// - Tracks maximum refresh rate across all open plotters
    /// - Updates UI refresh rate to match fastest plotter
    /// - Optimizes performance by avoiding unnecessary updates
    /// 
    /// # Lifecycle Management
    /// 
    /// ## Plotter Creation
    /// - Creates new plotter instances for new live_plotter plugins
    /// - Initializes with appropriate plugin ID and default settings
    /// 
    /// ## Plotter Cleanup
    /// - Removes plotters for plugins that no longer exist
    /// - Prevents memory leaks and stale references
    /// 
    /// # Performance Considerations
    /// 
    /// - Only processes samples for running plugins
    /// - Applies different processing levels based on plotter visibility
    /// - Uses intelligent decimation to handle high-frequency data
    /// - Updates UI refresh rate based on actual plotter requirements
    fn update_plotters(
        &mut self,
        tick: u64,
        outputs: &HashMap<(u64, String), f64>,
        samples: &HashMap<u64, Vec<(u64, Vec<f64>)>>,
    ) {
        let mut max_refresh = 1.0;
        let mut live_plotter_ids: HashSet<u64> = HashSet::new();

        for plugin in &self.workspace_manager.workspace.plugins {
            if plugin.kind != "live_plotter" {
                continue;
            }
            live_plotter_ids.insert(plugin.id);
            let fallback_sample = samples
                .get(&plugin.id)
                .and_then(|rows| rows.last())
                .map(|(_, values)| values.as_slice());
            let (input_count, refresh_hz, config_window_ms) =
                live_plotter_config(&plugin.config, fallback_sample);
            let preview_window_ms = self
                .plotter_manager
                .plotter_preview_settings
                .get(&plugin.id)
                .map(|settings| settings.11);
            let effective_window_ms = preview_window_ms.unwrap_or(config_window_ms).max(1.0);
            let series_names = live_plotter_series_names(
                &self.workspace_manager.workspace,
                &self.plugin_manager.installed_plugins,
                plugin.id,
                input_count,
            );
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
                // Window size must be set before update_config, because decimation
                // parameters are derived from current window_ms.
                plotter.set_window_ms(effective_window_ms);
                plotter.update_config(
                    input_count,
                    refresh_hz,
                    self.state_sync.logic_period_seconds,
                );
                plotter.set_series_names(series_names);
                if plugin.running {
                    if let Some(samples) = samples.get(&plugin.id) {
                        let sample_budget = if plotter.open { 8192 } else { 1024 };
                        let mut selected_indices: Vec<usize> = if samples.len() <= sample_budget {
                            (0..samples.len()).collect()
                        } else {
                            // Preserve first-channel extrema per chunk to avoid cutting spikes.
                            let chunk = (samples.len() + sample_budget - 1) / sample_budget;
                            let mut idxs = Vec::with_capacity(sample_budget * 2);
                            let mut start = 0usize;
                            while start < samples.len() {
                                let end = (start + chunk).min(samples.len());
                                idxs.push(start);
                                if end - start > 2 {
                                    let mut min_i = start;
                                    let mut max_i = start;
                                    let mut min_v =
                                        samples[start].1.first().copied().unwrap_or(0.0);
                                    let mut max_v = min_v;
                                    for (i, (_, values)) in
                                        samples.iter().enumerate().take(end).skip(start + 1)
                                    {
                                        let v = values.first().copied().unwrap_or(0.0);
                                        if v < min_v {
                                            min_v = v;
                                            min_i = i;
                                        }
                                        if v > max_v {
                                            max_v = v;
                                            max_i = i;
                                        }
                                    }
                                    idxs.push(min_i);
                                    idxs.push(max_i);
                                }
                                idxs.push(end - 1);
                                start = end;
                            }
                            idxs.sort_unstable();
                            idxs.dedup();
                            idxs
                        };
                        if selected_indices.len() > sample_budget * 2 {
                            let step = (selected_indices.len() / (sample_budget * 2)).max(1);
                            selected_indices = selected_indices.into_iter().step_by(step).collect();
                        }
                        for idx in selected_indices {
                            let (sample_tick, values) = &samples[idx];
                            plotter.push_sample_from_tick(
                                *sample_tick,
                                self.state_sync.logic_period_seconds,
                                self.state_sync.logic_time_scale,
                                values,
                            );
                        }
                    } else if plotter.open {
                        plotter.push_sample_from_tick(
                            tick,
                            self.state_sync.logic_period_seconds,
                            self.state_sync.logic_time_scale,
                            &values,
                        );
                    }
                    if plotter.open && refresh_hz > max_refresh {
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

    /// Sends updated runtime settings to the logic runtime and updates local state.
    /// 
    /// This method computes the current runtime configuration from GUI settings
    /// and transmits it to the logic runtime. It handles timing configuration,
    /// CPU core assignment, and UI refresh rate coordination.
    /// 
    /// # Configuration Computation
    /// 
    /// ## Timing Settings
    /// - Computes period in seconds from current frequency/period settings
    /// - Determines time scale and label based on selected units
    /// - Calculates appropriate time scaling for display purposes
    /// 
    /// ## CPU Core Selection
    /// - Converts boolean core selection array to list of enabled core indices
    /// - Ensures at least one core is selected for runtime execution
    /// - Provides core affinity configuration for real-time performance
    /// 
    /// ## Integration Limits
    /// - Sets maximum integration steps for real-time performance
    /// - Prevents runaway calculations that could impact timing
    /// 
    /// # State Synchronization
    /// 
    /// Updates local state to match sent settings:
    /// - `logic_period_seconds`: Period duration for timing calculations
    /// - `logic_time_scale`: Scale factor for time display
    /// - `logic_time_label`: Unit label for time axis displays
    /// 
    /// # Runtime Communication
    /// 
    /// Sends `LogicMessage::UpdateSettings` containing:
    /// - `cores`: List of CPU core indices to use
    /// - `period_seconds`: Execution period in seconds
    /// - `time_scale`: Time scaling factor for displays
    /// - `time_label`: Time unit label string
    /// - `ui_hz`: UI refresh rate for plotter updates
    /// - `max_integration_steps`: Safety limit for numerical integration
    /// 
    /// # Error Handling
    /// 
    /// Channel send errors are silently ignored to prevent GUI crashes
    /// if the runtime has terminated or become disconnected.
    /// 
    /// # Usage Context
    /// 
    /// Called when:
    /// - User changes timing settings in workspace settings dialog
    /// - CPU core selection is modified
    /// - Plotter refresh rates change and require UI rate updates
    /// - Application initialization requires initial settings
    /// - Workspace loading applies saved timing configuration
    /// 
    /// # Real-Time Considerations
    /// 
    /// The settings directly affect real-time performance:
    /// - Period determines execution frequency and timing precision
    /// - Core selection affects CPU affinity and scheduling
    /// - Integration limits prevent timing violations
    /// - UI refresh rate balances responsiveness with performance
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

    /// Applies workspace timing and core settings to the GUI and runtime.
    /// 
    /// This method loads the current workspace settings and applies them to both
    /// the GUI state and the logic runtime. It handles timing configuration,
    /// CPU core selection, and ensures consistent settings across the application.
    /// 
    /// # Settings Application Process
    /// 
    /// ## 1. Timing Configuration
    /// - Sets workspace timing tab to Frequency mode by default
    /// - Applies frequency value and unit from workspace settings
    /// - Applies period value and unit from workspace settings
    /// - Converts string units to enum types for internal use
    /// 
    /// ## 2. CPU Core Selection
    /// - Maps workspace core indices to boolean selection array
    /// - Ensures at least one core is selected (defaults to core 0)
    /// - Handles cases where workspace specifies unavailable cores
    /// 
    /// ## 3. Runtime Synchronization
    /// - Sends updated settings to logic runtime via `send_logic_settings()`
    /// - Ensures runtime uses current timing and core configuration
    /// 
    /// # Unit Conversion
    /// 
    /// ## Frequency Units
    /// - "hz" → `FrequencyUnit::Hz`
    /// - "khz" → `FrequencyUnit::KHz`  
    /// - "mhz" → `FrequencyUnit::MHz`
    /// - Default: Hz for unrecognized values
    /// 
    /// ## Period Units
    /// - "ns" → `PeriodUnit::Ns`
    /// - "us" → `PeriodUnit::Us`
    /// - "ms" → `PeriodUnit::Ms`
    /// - "s" → `PeriodUnit::S`
    /// - Default: Ms for unrecognized values
    /// 
    /// # Core Selection Logic
    /// 
    /// - Creates boolean array matching available CPU cores
    /// - Sets true for cores specified in workspace settings
    /// - Ensures at least core 0 is selected if no cores are specified
    /// - Handles gracefully if workspace specifies more cores than available
    /// 
    /// # Usage Context
    /// 
    /// Called during:
    /// - Application initialization
    /// - Workspace loading operations
    /// - Settings dialog application
    /// - Workspace switching
    /// 
    /// # Side Effects
    /// 
    /// - Updates GUI timing display values
    /// - Modifies core selection checkboxes
    /// - Sends settings to logic runtime
    /// - May trigger runtime reconfiguration
    /// 
    /// # Error Handling
    /// 
    /// - Gracefully handles invalid unit strings with sensible defaults
    /// - Ensures at least one core is always selected
    /// - Handles mismatched core counts between workspace and system
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
                .behavior_manager
                .cached_behaviors
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
            if !self.plugin_uses_plotter_viewport(&plugin.kind) {
                continue;
            }
            let should_open = plugin.running || self.plugin_uses_external_window(&plugin.kind);
            if !should_open {
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
