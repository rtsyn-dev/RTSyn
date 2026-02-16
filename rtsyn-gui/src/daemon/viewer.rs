use crate::plotter::LivePlotter;
use crate::ui::kv_row_wrapped;
use crate::{GuiConfig, GuiError};
use eframe::egui;
use rtsyn_cli::client;
use rtsyn_cli::protocol::{DaemonRequest, DaemonResponse, RuntimePluginState};
use rtsyn_core::plugin::PluginManager;
use std::time::{Duration, Instant};

/// Launches a standalone viewer window for monitoring a specific plugin's runtime state.
///
/// This function creates and runs an egui-based application window that connects
/// to a running RTSyn daemon to display real-time plugin data, including variables,
/// inputs, outputs, and live plotting for supported plugin types.
///
/// # Arguments
/// * `config` - GUI configuration including window dimensions
/// * `plugin_id` - Unique identifier of the plugin to monitor
/// * `socket_path` - Path to the daemon's Unix domain socket
///
/// # Returns
/// `Ok(())` on successful window closure, or `Err(GuiError)` if the window
/// fails to initialize or encounters a fatal error
///
/// # Features
/// - Real-time data visualization for live_plotter plugins
/// - Plugin state monitoring (variables, inputs, outputs)
/// - Automatic refresh based on plugin refresh rate
/// - Font Awesome icon support for enhanced UI
/// - Keyboard shortcuts (Esc to close)
///
/// # Window Behavior
/// - Creates a native window with the specified dimensions
/// - Disables VSync for smoother real-time updates
/// - Automatically handles plugin state changes and reconnection
pub fn run_daemon_plugin_viewer(
    config: GuiConfig,
    plugin_id: u64,
    socket_path: String,
) -> Result<(), GuiError> {
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([config.width, config.height]),
        ..Default::default()
    };
    options.vsync = false;

    eframe::run_native(
        &format!("RTSyn Viewer - {plugin_id}"),
        options,
        Box::new(move |cc| {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "fa".to_string(),
                egui::FontData::from_static(include_bytes!("../../assets/fonts/fa-solid-900.ttf")),
            );
            let family = fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default();
            if !family.contains(&"fa".to_string()) {
                family.push("fa".to_string());
            }
            cc.egui_ctx.set_fonts(fonts);
            Box::new(DaemonPluginViewer::new(plugin_id, socket_path))
        }),
    )
    .map_err(|err| GuiError::Gui(err.to_string()))
}

/// Represents a snapshot of plugin data retrieved from the daemon.
///
/// This structure contains all the information needed to display a plugin's
/// current state, including metadata, runtime variables, and time-series data.
struct DaemonPluginView {
    /// Plugin type identifier (e.g., "live_plotter", "csv_recorder")
    kind: String,
    /// Current runtime state including variables and I/O values
    state: RuntimePluginState,
    /// Sampling period in seconds for time-series data
    period_seconds: f64,
    /// Time scaling factor for display (e.g., 1000.0 for milliseconds)
    time_scale: f64,
    /// Label for the time axis in plots
    time_label: String,
    /// Recent data samples as (tick, values) pairs
    samples: Vec<(u64, Vec<f64>)>,
    /// Display names for data series
    series_names: Vec<String>,
}

/// Main application state for the daemon plugin viewer.
///
/// Manages the connection to the daemon, data fetching, display state,
/// and the live plotter for visualization.
struct DaemonPluginViewer {
    plugin_id: u64,
    socket_path: String,
    last_fetch: Instant,
    last_settings_fetch: Instant,
    view: Option<DaemonPluginView>,
    display_state: Option<RuntimePluginState>,
    display_kind: Option<String>,
    display_running: Option<bool>,
    plotter: LivePlotter,
    error: Option<String>,
    running: Option<bool>,
    last_sample_tick: Option<u64>,
    last_refresh_hz: f64,
}

impl DaemonPluginViewer {
    /// Creates a new daemon plugin viewer instance.
    ///
    /// # Arguments
    /// * `plugin_id` - Unique identifier of the plugin to monitor
    /// * `socket_path` - Path to the daemon's Unix domain socket
    ///
    /// # Returns
    /// A new viewer instance with default settings and empty state
    ///
    /// # Initial State
    /// - Sets up timers for data fetching with 1-second offset
    /// - Initializes empty view and display state
    /// - Creates a LivePlotter instance for the plugin
    /// - Sets default refresh rate to 60 Hz
    fn new(plugin_id: u64, socket_path: String) -> Self {
        Self {
            plugin_id,
            socket_path,
            last_fetch: Instant::now() - Duration::from_secs(1),
            last_settings_fetch: Instant::now() - Duration::from_secs(1),
            view: None,
            plotter: LivePlotter::new(plugin_id),
            error: None,
            running: None,
            last_sample_tick: None,
            last_refresh_hz: 60.0,
            display_state: None,
            display_kind: None,
            display_running: None,
        }
    }

    /// Fetches the current plugin view data from the daemon.
    ///
    /// This method sends a RuntimePluginView request to the daemon and processes
    /// the response, updating the internal view state and extracting runtime
    /// information like the plugin's running status.
    ///
    /// # Behavior
    /// - Sends request via Unix domain socket
    /// - Updates view state on successful response
    /// - Extracts "running" status from internal variables
    /// - Sets error state on communication or daemon errors
    /// - Ignores unexpected response types silently
    ///
    /// # Error Handling
    /// - Network/socket errors are stored in `self.error`
    /// - Daemon-reported errors are stored in `self.error`
    /// - Successful responses clear any previous error state
    fn fetch_view(&mut self) {
        match client::send_request_to(
            &self.socket_path,
            &DaemonRequest::RuntimePluginView { id: self.plugin_id },
        ) {
            Ok(DaemonResponse::RuntimePluginView {
                kind,
                state,
                samples,
                series_names,
                period_seconds,
                time_scale,
                time_label,
                ..
            }) => {
                self.running = state
                    .internal_variables
                    .iter()
                    .find(|(key, _)| key == "running")
                    .and_then(|(_, value)| value.as_bool());
                self.view = Some(DaemonPluginView {
                    kind,
                    state,
                    period_seconds,
                    time_scale,
                    time_label,
                    samples,
                    series_names,
                });
                self.error = None;
            }
            Ok(DaemonResponse::Error { message }) => {
                self.error = Some(message);
            }
            Err(err) => {
                self.error = Some(err);
            }
            _ => {}
        }
    }

    /// Creates a unified map of all numeric variables from plugin state.
    ///
    /// This utility method combines user variables and internal variables
    /// into a single lookup map, extracting only numeric values for use
    /// in configuration and display calculations.
    ///
    /// # Arguments
    /// * `state` - The plugin's runtime state
    ///
    /// # Returns
    /// A HashMap mapping variable names to their numeric values
    ///
    /// # Behavior
    /// - Processes both `variables` and `internal_variables` collections
    /// - Only includes values that can be converted to f64
    /// - Internal variables can override user variables with same names
    /// - Non-numeric values (strings, booleans, etc.) are ignored
    fn variable_map(state: &RuntimePluginState) -> std::collections::HashMap<&str, f64> {
        let mut map = std::collections::HashMap::new();
        for (key, value) in &state.variables {
            if let Some(num) = value.as_f64() {
                map.insert(key.as_str(), num);
            }
        }
        for (key, value) in &state.internal_variables {
            if let Some(num) = value.as_f64() {
                map.insert(key.as_str(), num);
            }
        }
        map
    }

    /// Extracts plotter configuration parameters from plugin state and samples.
    ///
    /// This method analyzes the plugin's runtime state to determine the optimal
    /// configuration for the live plotter, including input count, refresh rate,
    /// and time window settings.
    ///
    /// # Arguments
    /// * `state` - The plugin's runtime state containing variables
    /// * `samples` - Recent data samples for fallback input count detection
    ///
    /// # Returns
    /// A tuple of `(input_count, refresh_hz, window_ms)` configuration values
    ///
    /// # Configuration Sources
    /// - `input_count`: From "input_count" variable or sample array length
    /// - `refresh_hz`: From "refresh_hz" variable (default: 60.0)
    /// - `window_ms`: Multiple sources with fallback hierarchy:
    ///   1. Direct "window_ms" variable
    ///   2. "timebase_ms_div" × "timebase_divisions"
    ///   3. "window_multiplier" × "window_value" (default: 10000.0 × 10.0)
    fn plotter_config(
        state: &RuntimePluginState,
        samples: &[(u64, Vec<f64>)],
    ) -> (usize, f64, f64) {
        let vars = Self::variable_map(state);
        let mut input_count = vars.get("input_count").copied().unwrap_or(0.0) as usize;
        if input_count == 0 {
            if let Some((_, values)) = samples.last() {
                input_count = values.len();
            }
        }
        let refresh_hz = vars.get("refresh_hz").copied().unwrap_or(60.0);
        let window_ms = if let Some(window_ms) = vars.get("window_ms") {
            *window_ms
        } else if let (Some(ms_div), Some(divisions)) =
            (vars.get("timebase_ms_div"), vars.get("timebase_divisions"))
        {
            ms_div * divisions
        } else {
            let multiplier = vars.get("window_multiplier").copied().unwrap_or(10000.0);
            let value = vars.get("window_value").copied().unwrap_or(10.0);
            multiplier * value
        };
        (input_count, refresh_hz, window_ms)
    }

    /// Renders a collapsible section displaying key-value pairs with mixed data types.
    ///
    /// This method creates a formatted section with an icon, title, and list of
    /// variables displayed as read-only text fields. It handles various JSON
    /// value types with appropriate formatting.
    ///
    /// # Arguments
    /// * `ui` - The egui UI context
    /// * `title` - Section title text
    /// * `icon` - Unicode icon character for the section header
    /// * `items` - Array of (name, value) pairs to display
    ///
    /// # Behavior
    /// - Skips rendering if items array is empty
    /// - Creates collapsible header (default open)
    /// - Filters out "library_path" entries for cleaner display
    /// - Formats numbers with appropriate precision
    /// - Uses disabled text fields for read-only display
    /// - Applies consistent spacing and layout
    fn render_section_values(
        ui: &mut egui::Ui,
        title: &str,
        icon: &str,
        items: &[(String, serde_json::Value)],
    ) {
        if items.is_empty() {
            return;
        }
        egui::CollapsingHeader::new(
            egui::RichText::new(format!("{icon}  {title}"))
                .size(13.0)
                .strong(),
        )
        .default_open(true)
        .show(ui, |ui| {
            ui.add_space(4.0);
            let filtered: Vec<_> = items
                .iter()
                .filter(|(name, _)| name != "library_path")
                .collect();
            if !filtered.is_empty() {
                for (name, value) in filtered {
                    let mut value_text = match value {
                        serde_json::Value::Number(num) => {
                            if let Some(i) = num.as_i64() {
                                i.to_string()
                            } else if let Some(u) = num.as_u64() {
                                u.to_string()
                            } else {
                                num.as_f64()
                                    .map(|v| format!("{:.4}", v))
                                    .unwrap_or_else(|| value.to_string())
                            }
                        }
                        _ => value.to_string(),
                    };
                    kv_row_wrapped(ui, name, 140.0, |ui| {
                        ui.add_enabled_ui(false, |ui| {
                            ui.add_sized([80.0, 0.0], egui::TextEdit::singleline(&mut value_text));
                        });
                    });
                    ui.add_space(4.0);
                }
            }
        });
    }

    /// Renders a collapsible section displaying numeric key-value pairs.
    ///
    /// Similar to `render_section_values` but specifically designed for
    /// numeric data with consistent formatting for integer and floating-point values.
    ///
    /// # Arguments
    /// * `ui` - The egui UI context
    /// * `title` - Section title text
    /// * `icon` - Unicode icon character for the section header
    /// * `items` - Array of (name, value) pairs with numeric values
    ///
    /// # Number Formatting
    /// - Whole numbers: displayed without decimal places
    /// - Fractional numbers: displayed with up to 4 decimal places
    /// - Uses epsilon comparison for whole number detection
    fn render_section_numbers(ui: &mut egui::Ui, title: &str, icon: &str, items: &[(String, f64)]) {
        if items.is_empty() {
            return;
        }
        egui::CollapsingHeader::new(
            egui::RichText::new(format!("{icon}  {title}"))
                .size(13.0)
                .strong(),
        )
        .default_open(true)
        .show(ui, |ui| {
            ui.add_space(4.0);
            if !items.is_empty() {
                for (name, value) in items {
                    let mut value_text = if (value.fract() - 0.0).abs() < f64::EPSILON {
                        format!("{value:.0}")
                    } else {
                        format!("{value:.4}")
                    };
                    kv_row_wrapped(ui, name, 140.0, |ui| {
                        ui.add_enabled_ui(false, |ui| {
                            ui.add_sized([80.0, 0.0], egui::TextEdit::singleline(&mut value_text));
                        });
                    });
                    ui.add_space(4.0);
                }
            }
        });
    }

    /// Renders a collapsible section displaying input values with high precision.
    ///
    /// This method is specifically designed for displaying plugin input values,
    /// which typically require higher precision than other numeric displays.
    ///
    /// # Arguments
    /// * `ui` - The egui UI context
    /// * `title` - Section title text
    /// * `icon` - Unicode icon character for the section header
    /// * `items` - Array of (name, value) pairs with input values
    ///
    /// # Formatting
    /// - All values displayed with 4 decimal places for consistency
    /// - Suitable for sensor readings and measurement data
    /// - Maintains precision for small signal variations
    fn render_section_inputs(ui: &mut egui::Ui, title: &str, icon: &str, items: &[(String, f64)]) {
        if items.is_empty() {
            return;
        }
        egui::CollapsingHeader::new(
            egui::RichText::new(format!("{icon}  {title}"))
                .size(13.0)
                .strong(),
        )
        .default_open(true)
        .show(ui, |ui| {
            ui.add_space(4.0);
            if !items.is_empty() {
                for (name, value) in items {
                    let mut value_text = format!("{value:.4}");
                    kv_row_wrapped(ui, name, 140.0, |ui| {
                        ui.add_enabled_ui(false, |ui| {
                            ui.add_sized([80.0, 0.0], egui::TextEdit::singleline(&mut value_text));
                        });
                    });
                    ui.add_space(4.0);
                }
            }
        });
    }

    /// Renders a comprehensive plugin information card with all state details.
    ///
    /// This method creates a styled card displaying the plugin's complete state,
    /// including identification, status, and all variable categories in an
    /// organized, scrollable layout.
    ///
    /// # Arguments
    /// * `ui` - The egui UI context
    /// * `view` - The plugin view data to display
    ///
    /// # Card Layout
    /// - Header: Plugin ID badge, display name, and running status
    /// - Body: Scrollable sections for different variable types
    /// - Styling: Rounded corners, background fill, and border
    /// - Status indicator: Color-coded running/stopped status
    ///
    /// # Sections Displayed
    /// 1. Variables: User-configurable plugin parameters
    /// 2. Outputs: Plugin output values and results
    /// 3. Inputs: Current input readings and measurements
    /// 4. Internal variables: Plugin internal state and diagnostics
    fn render_plugin_card(&self, ui: &mut egui::Ui, view: &DaemonPluginView) {
        let frame = egui::Frame::none()
            .fill(egui::Color32::from_gray(30))
            .rounding(egui::Rounding::same(6.0))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(50)))
            .inner_margin(egui::Margin::same(12.0))
            .outer_margin(egui::Margin::ZERO);

        frame.show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let (id_rect, _) =
                        ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(id_rect, 8.0, egui::Color32::from_gray(60));
                    ui.painter().text(
                        id_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        self.plugin_id.to_string(),
                        egui::FontId::proportional(12.0),
                        egui::Color32::from_rgb(200, 200, 210),
                    );

                    ui.add_space(8.0);

                    let display_name = PluginManager::display_kind(&view.kind);
                    let title_w = (ui.available_width() - 70.0).max(80.0);
                    ui.add_sized(
                        [title_w, 0.0],
                        egui::Label::new(egui::RichText::new(display_name).size(15.0).strong())
                            .truncate(true),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(running) = self.running {
                            let status = if running { "Running" } else { "Stopped" };
                            let color = if running {
                                egui::Color32::from_rgb(80, 200, 120)
                            } else {
                                egui::Color32::from_rgb(220, 100, 100)
                            };
                            ui.label(egui::RichText::new(status).color(color));
                        }
                    });
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                ui.style_mut().spacing.item_spacing = egui::vec2(0.0, 6.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        Self::render_section_values(
                            ui,
                            "Variables",
                            "\u{f013}",
                            &view.state.variables,
                        );
                        Self::render_section_numbers(
                            ui,
                            "Outputs",
                            "\u{f08b}",
                            &view.state.outputs,
                        );
                        Self::render_section_inputs(ui, "Inputs", "\u{f090}", &view.state.inputs);
                        Self::render_section_values(
                            ui,
                            "Internal variables",
                            "\u{f085}",
                            &view.state.internal_variables,
                        );
                    });
            });
        });
    }
}

impl eframe::App for DaemonPluginViewer {
    /// Main update loop for the daemon plugin viewer application.
    ///
    /// This method handles the complete application lifecycle including data fetching,
    /// UI rendering, input processing, and display updates. It's called by the egui
    /// framework on each frame.
    ///
    /// # Arguments
    /// * `ctx` - The egui context for UI operations and input handling
    /// * `_frame` - The eframe frame (unused in current implementation)
    ///
    /// # Update Cycle
    /// 1. **Data Fetching**: Retrieves plugin data based on refresh rate
    /// 2. **Settings Update**: Caches display state periodically
    /// 3. **Input Handling**: Processes keyboard shortcuts (Esc to exit)
    /// 4. **Repaint Scheduling**: Requests redraws at appropriate intervals
    /// 5. **UI Rendering**: Displays plugin data and live plots
    ///
    /// # UI Layout
    /// - Bottom panel: Exit instructions
    /// - Error display: Shows connection/daemon errors
    /// - Loading state: "Waiting for runtime data..." message
    /// - Plugin-specific views:
    ///   - live_plotter: Side panel + central plot
    ///   - Other types: Central plugin card only
    ///
    /// # Live Plotting Features
    /// - Automatic plotter configuration from plugin state
    /// - Series name assignment and data ingestion
    /// - Tick-based sample processing with reset detection
    /// - Real-time plot rendering with time-based windowing
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let fetch_interval = if self.last_refresh_hz > 0.0 {
            Duration::from_secs_f64(1.0 / self.last_refresh_hz.max(1.0))
        } else {
            Duration::from_millis(50)
        };
        if self.last_fetch.elapsed() >= fetch_interval {
            self.fetch_view();
            self.last_fetch = Instant::now();
        }
        if self.last_settings_fetch.elapsed() >= Duration::from_secs(1) {
            if let Some(view) = self.view.as_ref() {
                self.display_state = Some(view.state.clone());
                self.display_kind = Some(view.kind.clone());
                self.display_running = self.running;
            }
            self.last_settings_fetch = Instant::now();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let refresh_hz = self.last_refresh_hz.max(1.0);
        ctx.request_repaint_after(Duration::from_secs_f64(1.0 / refresh_hz));

        egui::TopBottomPanel::bottom("viewer_bottom")
            .min_height(48.0)
            .show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new("Press Esc to exit view")
                            .size(18.0)
                            .strong()
                            .color(egui::Color32::from_rgb(220, 220, 220)),
                    );
                });
            });

        if let Some(err) = self.error.as_ref() {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.colored_label(egui::Color32::LIGHT_RED, err);
            });
            return;
        }
        let Some(view) = self.view.as_ref() else {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("Waiting for runtime data...");
            });
            return;
        };

        if view.kind == "live_plotter" {
            if let Some(state) = self.display_state.as_ref() {
                let display_view = DaemonPluginView {
                    kind: self
                        .display_kind
                        .clone()
                        .unwrap_or_else(|| view.kind.clone()),
                    state: state.clone(),
                    period_seconds: view.period_seconds,
                    time_scale: view.time_scale,
                    time_label: view.time_label.clone(),
                    samples: Vec::new(),
                    series_names: Vec::new(),
                };
                egui::SidePanel::left("viewer_card")
                    .default_width(320.0)
                    .resizable(false)
                    .show(ctx, |ui| {
                        self.render_plugin_card(ui, &display_view);
                    });
            }

            egui::CentralPanel::default().show(ctx, |ui| {
                let (input_count, refresh_hz, window_ms) =
                    Self::plotter_config(&view.state, &view.samples);
                self.last_refresh_hz = refresh_hz;
                let period_seconds = view.period_seconds;
                // Window size must be set before update_config, because decimation
                // parameters are derived from current window_ms.
                self.plotter.set_window_ms(window_ms);
                self.plotter
                    .update_config(input_count, refresh_hz, period_seconds);
                if !view.series_names.is_empty() {
                    self.plotter.set_series_names(view.series_names.clone());
                } else {
                    let series_names = (0..input_count).map(|i| format!("in_{i}")).collect();
                    self.plotter.set_series_names(series_names);
                }
                let time_scale = view.time_scale;
                if let Some(latest_tick) = view.samples.last().map(|(tick, _)| *tick) {
                    if let Some(last_tick) = self.last_sample_tick {
                        if latest_tick < last_tick {
                            self.plotter = LivePlotter::new(self.plugin_id);
                            self.last_sample_tick = None;
                        }
                    }
                }
                let mut latest_tick = self.last_sample_tick.unwrap_or(0);
                let mut has_samples = false;
                for (tick, values) in &view.samples {
                    if self.last_sample_tick.map_or(true, |last| *tick > last) {
                        self.plotter.push_sample_from_tick(
                            *tick,
                            period_seconds,
                            time_scale,
                            values,
                        );
                        if *tick > latest_tick {
                            latest_tick = *tick;
                        }
                        has_samples = true;
                    }
                }
                if has_samples {
                    self.last_sample_tick = Some(latest_tick);
                }
                self.plotter.render(ui, "", &view.time_label);
            });
        } else {
            if let Some(state) = self.display_state.as_ref() {
                let display_view = DaemonPluginView {
                    kind: self
                        .display_kind
                        .clone()
                        .unwrap_or_else(|| view.kind.clone()),
                    state: state.clone(),
                    period_seconds: view.period_seconds,
                    time_scale: view.time_scale,
                    time_label: view.time_label.clone(),
                    samples: Vec::new(),
                    series_names: Vec::new(),
                };
                egui::CentralPanel::default().show(ctx, |ui| {
                    self.render_plugin_card(ui, &display_view);
                });
            }
        }
    }
}
