//! Plotter UI components and window management for RTSyn GUI.
//!
//! This module provides the user interface components for managing and displaying
//! real-time plotting windows in the RTSyn application. It handles:
//!
//! - Plotter window rendering and viewport management
//! - Interactive controls for plot customization (knobs, wheels, timebase)
//! - Series data visualization with scaling and offset controls
//! - Export functionality for plot images (PNG/SVG)
//! - Settings dialogs for plot appearance and behavior
//! - Connection management for plotter plugins
//! - Notification display for plotter-specific messages
//!
//! The module implements a comprehensive plotting interface that allows users to
//! visualize real-time data streams from various plugins, customize appearance,
//! and export plots for documentation or analysis purposes.

use super::*;
use crate::utils::truncate_string;
use crate::state::PlotterPreviewState;
use std::time::Duration;

impl GuiApp {
/// Renders floating notification toasts for plotter-specific messages.
    ///
    /// This function displays temporary notification messages that slide in from the
    /// right side of the screen, providing feedback about plotter operations, errors,
    /// or status changes. The notifications use smooth animations and automatic timing.
    ///
    /// # Parameters
    /// - `ctx`: The egui context for rendering and animation
    /// - `plugin_id`: The ID of the plotter plugin to show notifications for
    ///
    /// # Notification Behavior
    /// - **Slide Animation**: Notifications slide in from off-screen right
    /// - **Timing**: Total display duration of 2.8 seconds
    /// - **Fade Transitions**: 0.35s slide-in, 0.45s slide-out with smooth easing
    /// - **Stacking**: Multiple notifications stack vertically
    /// - **Limit**: Maximum of 4 notifications shown simultaneously
    ///
    /// # Visual Properties
    /// - **Position**: Top-right corner with 12px margin
    /// - **Size**: Maximum width of 360px, auto-height
    /// - **Styling**: Dark semi-transparent background with subtle border
    /// - **Content**: Title (bold, 14pt) and message (regular, 13pt)
    ///
    /// # Animation Details
    /// - Uses smooth step function for natural easing curves
    /// - Calculates position based on age and transition phases
    /// - Requests continuous repaints during active animations
    /// - Handles cleanup automatically through NotificationHandler
    ///
    /// # Implementation Notes
    /// - Non-interactive overlays (click-through)
    /// - Foreground rendering order for visibility
    /// - Efficient early exit when no notifications exist
    /// - Automatic repaint scheduling for smooth animation
    pub(super) fn render_plotter_notifications(&mut self, ctx: &egui::Context, plugin_id: u64) {
        let Some(list) = self.notification_handler.get_plugin_notifications(plugin_id) else {
            return;
        };
        if list.is_empty() {
            return;
        }

        let now = std::time::Instant::now();
        let total = 2.8_f32;
        let max_width = 360.0;
        let mut y = ctx.screen_rect().min.y + 12.0;
        let x = ctx.screen_rect().max.x - 10.0;
        let mut shown = 0usize;

        for (idx, notification) in list.iter().enumerate() {
            let age = now.duration_since(notification.created_at).as_secs_f32();
            if age >= total {
                continue;
            }
            let slide_in = 0.35_f32;
            let slide_out = 0.45_f32;
            let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
            let slide = if age < slide_in {
                smooth((age / slide_in).clamp(0.0, 1.0))
            } else if age > total - slide_out {
                smooth(((total - age) / slide_out).clamp(0.0, 1.0))
            } else {
                1.0
            };

            let offscreen = max_width + 24.0;
            let x_pos = x + (1.0 - slide) * offscreen;
            egui::Area::new(egui::Id::new(("plotter_toast", plugin_id, idx)))
                .order(egui::Order::Foreground)
                .interactable(false)
                .pivot(egui::Align2::RIGHT_TOP)
                .fixed_pos(egui::pos2(x_pos, y))
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style())
                        .fill(egui::Color32::from_rgba_premultiplied(20, 20, 20, 220))
                        .stroke(egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgba_premultiplied(80, 80, 80, 220),
                        ))
                        .rounding(egui::Rounding::same(6.0))
                        .show(ui, |ui| {
                            ui.set_max_width(max_width);
                            ui.label(egui::RichText::new(&notification.title).strong().size(14.0));
                            ui.label(egui::RichText::new(&notification.message).size(13.0));
                        });
                });
            y += 62.0;
            shown += 1;
            if shown >= 4 {
                break;
            }
        }

        // Cleanup is handled by NotificationHandler
        if !list.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    /// Toggles the running state of a plugin from within a plotter window.
    ///
    /// This function handles start/stop operations for plugins that support runtime
    /// control, performing validation checks and updating both the workspace state
    /// and the logic engine. It provides comprehensive error handling for various
    /// failure conditions.
    ///
    /// # Parameters
    /// - `plugin_id`: The ID of the plugin to toggle
    ///
    /// # Returns
    /// - `Ok(bool)`: The new running state (true if started, false if stopped)
    /// - `Err(String)`: Error message describing why the operation failed
    ///
    /// # Validation Checks (for starting)
    /// 1. **Plugin Existence**: Verifies the plugin exists in the workspace
    /// 2. **Start/Stop Support**: Checks if the plugin supports runtime control
    /// 3. **Required Inputs**: Validates all required input connections are present
    /// 4. **Required Outputs**: Validates all required output connections are present
    ///
    /// # State Updates
    /// - Updates the plugin's running flag in the workspace
    /// - Sends state change message to the logic engine
    /// - Marks workspace as dirty for persistence
    /// - Opens plotter viewport if starting a plotter-capable plugin
    /// - Recomputes UI refresh rates for optimal performance
    ///
    /// # Error Conditions
    /// - Plugin not found in workspace
    /// - Plugin doesn't support start/stop operations
    /// - Missing required input connections
    /// - Missing required output connections
    ///
    /// # Side Effects
    /// - May open plotter windows for visualization plugins
    /// - Triggers workspace persistence
    /// - Updates logic engine state
    /// - Recalculates UI refresh rates
    ///
    /// # Use Cases
    /// - Interactive start/stop from plotter window controls
    /// - Batch operations on multiple plotters
    /// - Automated plugin lifecycle management
    pub(super) fn toggle_plugin_running_from_plotter_window(
        &mut self,
        plugin_id: u64,
    ) -> Result<bool, String> {
        let plugin_index = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .position(|plugin| plugin.id == plugin_id)
            .ok_or_else(|| "Plugin not found".to_string())?;

        let plugin_kind = self.workspace_manager.workspace.plugins[plugin_index]
            .kind
            .clone();
        let currently_running = self.workspace_manager.workspace.plugins[plugin_index].running;
        self.ensure_plugin_behavior_cached(&plugin_kind);
        let behavior = self
            .behavior_manager
            .cached_behaviors
            .get(&plugin_kind)
            .cloned()
            .unwrap_or_default();

        if !behavior.supports_start_stop {
            return Err("Plugin does not support start/stop.".to_string());
        }

        if !currently_running {
            let connected_inputs: std::collections::HashSet<String> = self
                .workspace_manager
                .workspace
                .connections
                .iter()
                .filter(|conn| conn.to_plugin == plugin_id)
                .map(|conn| conn.to_port.clone())
                .collect();
            let connected_outputs: std::collections::HashSet<String> = self
                .workspace_manager
                .workspace
                .connections
                .iter()
                .filter(|conn| conn.from_plugin == plugin_id)
                .map(|conn| conn.from_port.clone())
                .collect();

            let missing_inputs: Vec<String> = behavior
                .start_requires_connected_inputs
                .iter()
                .filter(|port| !connected_inputs.contains(*port))
                .cloned()
                .collect();
            if !missing_inputs.is_empty() {
                return Err(format!(
                    "Cannot start: missing input connections on ports: {}",
                    missing_inputs.join(", ")
                ));
            }

            let missing_outputs: Vec<String> = behavior
                .start_requires_connected_outputs
                .iter()
                .filter(|port| !connected_outputs.contains(*port))
                .cloned()
                .collect();
            if !missing_outputs.is_empty() {
                return Err(format!(
                    "Cannot start: missing output connections on ports: {}",
                    missing_outputs.join(", ")
                ));
            }
        }

        let new_running = !currently_running;
        
        self.workspace_manager.workspace.plugins[plugin_index].running = new_running;
        let _ = self
            .state_sync
            .logic_tx
            .send(LogicMessage::SetPluginRunning(plugin_id, new_running));
        self.mark_workspace_dirty();

        if self.plugin_uses_plotter_viewport(&plugin_kind) && new_running {
            if let Some(plotter) = self.plotter_manager.plotters.get(&plugin_id) {
                if let Ok(mut plotter) = plotter.lock() {
                    plotter.open = true;
                }
            }
            self.recompute_plotter_ui_hz();
        }

        Ok(new_running)
    }

    /// Renders all open plotter windows with their controls and settings dialogs.
    ///
    /// This is the main function responsible for rendering the complete plotter interface,
    /// including plot displays, control buttons, settings dialogs, and export functionality.
    /// It manages multiple plotter windows simultaneously and handles all user interactions.
    ///
    /// # Parameters
    /// - `ctx`: The egui context for rendering and viewport management
    ///
    /// # Rendered Components
    /// - **Plot Display**: Real-time data visualization with customizable appearance
    /// - **Control Buttons**: Start/Stop, Add/Remove connections, Settings, Capture
    /// - **Settings Dialog**: Comprehensive plot customization interface
    /// - **Export Dialog**: Image/SVG export with resolution controls
    /// - **Connection Editor**: Embedded connection management (when active)
    /// - **Notifications**: Floating status messages and alerts
    ///
    /// # Window Management
    /// - Creates separate viewports for each plotter (or embedded windows)
    /// - Handles window close events and cleanup
    /// - Manages dialog state persistence across frames
    /// - Coordinates between main window and popup dialogs
    ///
    /// # User Interactions Handled
    /// - **Start/Stop**: Plugin execution control with validation
    /// - **Settings**: Plot appearance and behavior configuration
    /// - **Export**: Image capture with format and resolution options
    /// - **Connections**: Add/remove data source connections
    /// - **Series Control**: Individual series scaling, offset, and naming
    /// - **Timebase**: Time window and division configuration
    ///
    /// # State Management
    /// - Synchronizes settings between dialogs and live plots
    /// - Preserves user customizations across sessions
    /// - Handles dynamic series count changes
    /// - Manages temporary dialog state in egui data storage
    ///
    /// # Performance Considerations
    /// - Requests repaints at appropriate refresh rates
    /// - Efficiently handles multiple concurrent plotters
    /// - Minimizes unnecessary UI updates
    /// - Optimizes viewport rendering for smooth animation
    ///
    /// # Implementation Details
    /// - Uses viewport system for native window management
    /// - Falls back to embedded windows when viewports unavailable
    /// - Implements comprehensive error handling for all operations
    /// - Maintains consistent UI layout across different screen sizes
    pub(crate) fn render_plotter_windows(&mut self, ctx: &egui::Context) {
        let mut closed = Vec::new();
        let mut export_saved: Vec<u64> = Vec::new();
        let mut settings_saved: Vec<u64> = Vec::new();
        let mut start_toggle_requested: Vec<u64> = Vec::new();
        let mut add_connection_requested: Vec<u64> = Vec::new();
        let mut remove_connection_requested: Vec<u64> = Vec::new();
        let name_by_id: HashMap<u64, String> = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .map(|plugin| (plugin.id, self.plugin_display_name(plugin.id)))
            .collect();
        let plotter_ids: Vec<u64> = self
            .plotter_manager
            .plotters
            .iter()
            .filter_map(|(id, plotter)| {
                plotter
                    .lock()
                    .ok()
                    .and_then(|plotter| {
                        if plotter.open { Some(*id) } else { None }
                    })
            })
            .collect();
        

        for plugin_id in plotter_ids {
            let settings_seed = self.build_plotter_preview_state(plugin_id);
            let plugin_running = self
                .workspace_manager
                .workspace
                .plugins
                .iter()
                .find(|plugin| plugin.id == plugin_id)
                .map(|plugin| plugin.running)
                .unwrap_or(false);
            let display_name = name_by_id
                .get(&plugin_id)
                .cloned()
                .unwrap_or_else(|| "plotter".to_string());
            let title = format!("Plotter #{} {}", plugin_id, display_name);
            let viewport_id = egui::ViewportId::from_hash_of(("plotter", plugin_id));
            let builder = egui::ViewportBuilder::default()
                .with_title(title.clone())
                .with_min_inner_size([1100.0, 650.0])
                .with_resizable(true)
                .with_close_button(false);

            let plotter = self
                .plotter_manager
                .plotters
                .get(&plugin_id)
                .cloned()
                .expect("plotter exists");
            let plotter_for_viewport = plotter.clone();
            let preview_settings = self
                .plotter_manager
                .plotter_preview_settings
                .get(&plugin_id)
                .cloned();
            let time_label = self.state_sync.logic_time_label.clone();
            let logic_period_seconds = self.state_sync.logic_period_seconds;

            ctx.show_viewport_immediate(viewport_id, builder, |ctx, class| {
                if class == egui::ViewportClass::Embedded {
                    return;
                }

                egui::CentralPanel::default().show(ctx, |ui| {
                    if let Ok(mut plotter) = plotter_for_viewport.lock() {
                        let button_h = BUTTON_SIZE.y;
                        let gap_h = 6.0;
                        let plot_margin = 10.0;
                        let available = ui.available_size();
                        let plot_h = (available.y - button_h - gap_h).max(0.0);
                        let plot_rect = egui::Rect::from_min_size(
                            ui.min_rect().min,
                            egui::vec2(available.x, plot_h),
                        );
                        let inner_rect = plot_rect.shrink2(egui::vec2(plot_margin, plot_margin));
                        ui.allocate_ui_at_rect(inner_rect, |ui| {
                            if let Some((
                                show_axes,
                                show_legend,
                                show_grid,
                                series_names,
                                series_scales,
                                series_offsets,
                                colors,
                                title,
                                dark_theme,
                                x_axis,
                                y_axis,
                                window_ms,
                                _timebase_divisions,
                                _high_quality,
                                _export_svg,
                            )) = preview_settings.clone()
                            {
                                let series_transforms = Self::build_series_transforms(
                                    &series_scales,
                                    &series_offsets,
                                    series_names.len(),
                                );
                                plotter.render_with_settings(
                                    ui,
                                    "",
                                    &time_label,
                                    show_axes,
                                    show_legend,
                                    show_grid,
                                    Some(&title),
                                    Some(&series_names),
                                    Some(&series_transforms),
                                    Some(&colors),
                                    dark_theme,
                                    Some(&x_axis),
                                    Some(&y_axis),
                                    Some(window_ms),
                                );
                            } else {
                                plotter.render(ui, "", &time_label);
                            }
                        });
                        ui.allocate_space(egui::vec2(available.x, plot_h));
                        ui.add_space(gap_h);
                        let button_rect = egui::Rect::from_min_size(
                            egui::pos2(plot_rect.left() + plot_margin, plot_rect.bottom() + gap_h),
                            egui::vec2(
                                (plot_rect.width() - plot_margin * 2.0).max(0.0),
                                BUTTON_SIZE.y,
                            ),
                        );
                        ui.allocate_ui_at_rect(button_rect, |ui| {
                            ui.horizontal(|ui| {
                                let start_label = if plugin_running { "Stop" } else { "Start" };
                                if styled_button(ui, egui::RichText::new(start_label).size(12.0))
                                    .on_hover_text("Start/stop this live plotter plugin")
                                    .clicked()
                                {
                                    ctx.data_mut(|d| {
                                        d.insert_temp(
                                            egui::Id::new(("plotter_start_toggle", plugin_id)),
                                            true,
                                        )
                                    });
                                }
                                if styled_button(
                                    ui,
                                    egui::RichText::new("Add connections").size(12.0),
                                )
                                .on_hover_text("Add connections for this live plotter")
                                .clicked()
                                {
                                    ctx.data_mut(|d| {
                                        d.insert_temp(
                                            egui::Id::new(("plotter_add_connections", plugin_id)),
                                            true,
                                        )
                                    });
                                }
                                if styled_button(
                                    ui,
                                    egui::RichText::new("Remove connections").size(12.0),
                                )
                                .on_hover_text("Remove connections for this live plotter")
                                .clicked()
                                {
                                    ctx.data_mut(|d| {
                                        d.insert_temp(
                                            egui::Id::new((
                                                "plotter_remove_connections",
                                                plugin_id,
                                            )),
                                            true,
                                        )
                                    });
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let capture = styled_button(
                                            ui,
                                            egui::RichText::new("Capture").size(12.0),
                                        )
                                        .on_hover_text("Save plot image");
                                        if capture.clicked() {
                                            let export_open =
                                                egui::Id::new(("plotter_export_open", plugin_id));
                                            let export_state =
                                                egui::Id::new(("plotter_export_state", plugin_id));
                                            ctx.data_mut(|d| {
                                                d.insert_temp(export_open, true);
                                                if d.get_temp::<PlotterPreviewState>(export_state)
                                                    .is_none()
                                                {
                                                    d.insert_temp(
                                                        export_state,
                                                        settings_seed.clone(),
                                                    );
                                                }
                                            });
                                        }
                                        let settings = styled_button(
                                            ui,
                                            egui::RichText::new("Settings").size(12.0),
                                        )
                                        .on_hover_text("Plot settings");
                                        if settings.clicked() {
                                            let open_id =
                                                egui::Id::new(("plotter_settings_open", plugin_id));
                                            let state_id = egui::Id::new((
                                                "plotter_settings_state",
                                                plugin_id,
                                            ));
                                            ctx.data_mut(|d| {
                                                d.insert_temp(open_id, true);
                                                if d.get_temp::<PlotterPreviewState>(state_id)
                                                    .is_none()
                                                {
                                                    d.insert_temp(state_id, settings_seed.clone());
                                                }
                                            });
                                        }
                                    },
                                );
                            });
                        });

                        let export_open = egui::Id::new(("plotter_export_open", plugin_id));
                        let export_state = egui::Id::new(("plotter_export_state", plugin_id));
                        let export_save = egui::Id::new(("plotter_export_save", plugin_id));
                        let export_close = egui::Id::new(("plotter_export_close", plugin_id));
                        let mut export_is_open =
                            ctx.data(|d| d.get_temp::<bool>(export_open).unwrap_or(false));
                        if export_is_open {
                            let mut state = ctx
                                .data(|d| d.get_temp::<PlotterPreviewState>(export_state))
                                .unwrap_or_else(|| settings_seed.clone());
                            let mut save_requested = false;
                            egui::Window::new("Plot Export")
                                .resizable(false)
                                .default_size(egui::vec2(360.0, 180.0))
                                .open(&mut export_is_open)
                                .show(ctx, |ui| {
                                    ui.checkbox(&mut state.export_svg, "Export as SVG");
                                    ui.horizontal(|ui| {
                                        ui.label("Resolution:");
                                        let old_width = state.width;
                                        ui.add_enabled(
                                            !state.export_svg,
                                            egui::DragValue::new(&mut state.width)
                                                .clamp_range(400..=4000)
                                                .suffix("px"),
                                        );
                                        if state.width != old_width && !state.export_svg {
                                            let ratio = 16.0 / 9.0;
                                            state.height = (state.width as f32 / ratio) as u32;
                                        }
                                        ui.label("×");
                                        let old_height = state.height;
                                        ui.add_enabled(
                                            !state.export_svg,
                                            egui::DragValue::new(&mut state.height)
                                                .clamp_range(300..=3000)
                                                .suffix("px"),
                                        );
                                        if state.height != old_height && !state.export_svg {
                                            let ratio = 16.0 / 9.0;
                                            state.width = (state.height as f32 * ratio) as u32;
                                        }
                                    });
                                    if styled_button(ui, "Save").clicked() {
                                        save_requested = true;
                                    }
                                });
                            if save_requested {
                                export_is_open = false;
                                ctx.data_mut(|d| d.insert_temp(export_save, true));
                            }
                            ctx.data_mut(|d| {
                                d.insert_temp(export_state, state);
                                d.insert_temp(export_open, export_is_open);
                                if !export_is_open {
                                    d.insert_temp(export_close, true);
                                }
                            });
                        }

                        let open_id = egui::Id::new(("plotter_settings_open", plugin_id));
                        let state_id = egui::Id::new(("plotter_settings_state", plugin_id));
                        let save_id = egui::Id::new(("plotter_settings_save", plugin_id));
                        let mut open = ctx.data(|d| d.get_temp::<bool>(open_id).unwrap_or(false));
                        if open {
                            let mut state = ctx
                                .data(|d| d.get_temp::<PlotterPreviewState>(state_id))
                                .unwrap_or_else(|| settings_seed.clone());
                            Self::sync_series_controls_from_seed(&mut state, &settings_seed);
                            while state.series_scales.len() < state.series_names.len() {
                                state.series_scales.push(1.0);
                            }
                            while state.series_offsets.len() < state.series_names.len() {
                                state.series_offsets.push(0.0);
                            }
                            if state.series_names.is_empty() {
                                state.selected_series_tab = 0;
                                state.series_tab_start = 0;
                            } else {
                                state.selected_series_tab =
                                    state.selected_series_tab.min(state.series_names.len() - 1);
                                state.series_tab_start =
                                    state.series_tab_start.min(state.series_names.len() - 1);
                            }
                            let viewport_size = ctx.screen_rect().size();
                            let width_limit = (viewport_size.x - 24.0).max(420.0);
                            let height_limit = (viewport_size.y - 24.0).max(320.0);
                            // Keep a stable "dense" settings layout (no huge blank area on large monitors),
                            // while still fitting smaller windows. Target profile is close to the
                            // visually good minimized case.
                            let width_cap = width_limit.min(980.0);
                            let height_cap = height_limit.min(560.0);
                            let k = (width_cap / 16.0).min(height_cap / 9.0);
                            let fixed_size =
                                egui::vec2((k * 16.0).max(600.0), (k * 9.0).max(480.0));
                            egui::Window::new("Plot Settings")
                                .resizable(false)
                                .fixed_size(fixed_size)
                                .open(&mut open)
                                .show(ctx, |ui| {
                                    let apply_bar_h = BUTTON_SIZE.y + 6.0;
                                    let scroll_h = (ui.available_height() - apply_bar_h).max(120.0);
                                    egui::ScrollArea::vertical()
                                        .auto_shrink([false, false])
                                        .max_height(scroll_h)
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                ui.label("Title:");
                                                ui.text_edit_singleline(&mut state.title);
                                            });
                                            ui.horizontal(|ui| {
                                                ui.checkbox(&mut state.show_axes, "Show axes");
                                                ui.checkbox(&mut state.show_legend, "Show legend");
                                                ui.checkbox(&mut state.show_grid, "Show grid");
                                                ui.checkbox(&mut state.dark_theme, "Dark theme");
                                            });
                                            ui.horizontal(|ui| {
                                                ui.label("X-axis:");
                                                ui.text_edit_singleline(&mut state.x_axis_name);
                                                ui.label("Y-axis:");
                                                ui.text_edit_singleline(&mut state.y_axis_name);
                                            });
                                            Self::render_timebase_controls(
                                                ui,
                                                &mut state.window_ms,
                                                &mut state.timebase_divisions,
                                            );
                                            ui.horizontal(|ui| {
                                                ui.label("Refresh Hz:");
                                                ui.add(
                                                    egui::DragValue::new(&mut state.refresh_hz)
                                                        .clamp_range(1.0..=1000.0)
                                                        .speed(1.0),
                                                );
                                                ui.separator();
                                                ui.label("Priority:");
                                                ui.add(
                                                    egui::DragValue::new(&mut state.priority)
                                                        .clamp_range(-100..=1000)
                                                        .speed(1.0),
                                                );
                                            });
                                            ui.separator();

                                            ui.horizontal(|ui| {
                                                let total = state.series_names.len();
                                                let tab_w = 180.0_f32;
                                                let arrow_w = 28.0_f32;
                                                let available = ui.available_width().max(tab_w);
                                                let can_page = (total as f32 * tab_w) > available;
                                                let tabs_width = if can_page {
                                                    (available - arrow_w).max(tab_w)
                                                } else {
                                                    available
                                                };
                                                let visible =
                                                    ((tabs_width / tab_w).floor() as usize).max(1);
                                                let end =
                                                    (state.series_tab_start + visible).min(total);
                                                for i in state.series_tab_start..end {
                                                    let full = state.series_names[i].clone();
                                                    let text = truncate_string(&full, 20);
                                                    let selected = state.selected_series_tab == i;
                                                    let resp = ui.add_sized(
                                                        [180.0, 24.0],
                                                        egui::SelectableLabel::new(selected, text),
                                                    );
                                                    if resp.clicked() {
                                                        state.selected_series_tab = i;
                                                    }
                                                    if !selected {
                                                        let _ = resp.on_hover_text(full);
                                                    }
                                                }
                                                if can_page
                                                    && ui
                                                        .add_enabled(
                                                            state.series_tab_start + visible
                                                                < total,
                                                            egui::Button::new(">"),
                                                        )
                                                        .clicked()
                                                {
                                                    state.series_tab_start =
                                                        (state.series_tab_start + 1)
                                                            .min(total.saturating_sub(1));
                                                }
                                            });

                                            if !state.series_names.is_empty() {
                                                let i = state.selected_series_tab;
                                                ui.horizontal(|ui| {
                                                    ui.add_sized(
                                                        [
                                                            (ui.available_width() - 28.0)
                                                                .max(120.0),
                                                            22.0,
                                                        ],
                                                        egui::TextEdit::singleline(
                                                            &mut state.series_names[i],
                                                        ),
                                                    );
                                                    ui.color_edit_button_srgba(
                                                        &mut state.colors[i],
                                                    );
                                                });
                                            }
                                            ui.separator();
                                            let preview_height =
                                                (ui.available_height() * 0.5).clamp(170.0, 360.0);
                                            let preview_size =
                                                egui::vec2(ui.available_width(), preview_height);
                                            let series_transforms = Self::build_series_transforms(
                                                &state.series_scales,
                                                &state.series_offsets,
                                                state.series_names.len(),
                                            );
                                            ui.allocate_ui(preview_size, |ui| {
                                                let input_count = plotter.input_count;
                                                let refresh_hz = plotter.refresh_hz;
                                                plotter.set_window_ms(state.window_ms);
                                                plotter.update_config(
                                                    input_count,
                                                    refresh_hz,
                                                    logic_period_seconds,
                                                );
                                                plotter.render_with_settings(
                                                    ui,
                                                    "",
                                                    &time_label,
                                                    state.show_axes,
                                                    state.show_legend,
                                                    state.show_grid,
                                                    Some(&state.title),
                                                    Some(&state.series_names),
                                                    Some(&series_transforms),
                                                    Some(&state.colors),
                                                    state.dark_theme,
                                                    Some(&state.x_axis_name),
                                                    Some(&state.y_axis_name),
                                                    Some(state.window_ms),
                                                );
                                            });
                                            ui.separator();
                                            if !state.series_names.is_empty() {
                                                let i = state.selected_series_tab;
                                                let row_w = ui.available_width();
                                                let row_h = (ui.available_height() - 56.0)
                                                    .clamp(120.0, 180.0);
                                                ui.allocate_ui_with_layout(
                                                    egui::vec2(row_w, row_h),
                                                    egui::Layout::left_to_right(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        let left_w =
                                                            (row_w * 0.18).clamp(110.0, 180.0);
                                                        let right_w =
                                                            (row_w * 0.18).clamp(110.0, 180.0);
                                                        let center_w =
                                                            (row_w - left_w - right_w).max(260.0);

                                                        ui.allocate_ui_with_layout(
                                                            egui::vec2(left_w, row_h),
                                                            egui::Layout::top_down(
                                                                egui::Align::Center,
                                                            ),
                                                            |ui| {
                                                                Self::knob_control(
                                                                    ui,
                                                                    "DC Offset",
                                                                    &mut state.series_offsets[i],
                                                                    -1_000_000_000.0,
                                                                    1_000_000_000.0,
                                                                    1.0,
                                                                    1,
                                                                );
                                                            },
                                                        );

                                                        ui.allocate_ui_with_layout(
                                                            egui::vec2(center_w, row_h),
                                                            egui::Layout::top_down(
                                                                egui::Align::Center,
                                                            ),
                                                            |ui| {
                                                                ui.vertical_centered(|ui| {
                                                                    Self::wheel_value_box(
                                                                        ui,
                                                                        "Offset Value",
                                                                        &mut state.series_offsets
                                                                            [i],
                                                                        1,
                                                                        -1_000_000_000.0,
                                                                        1_000_000_000.0,
                                                                    );
                                                                    ui.add_space(8.0);
                                                                    Self::wheel_value_box(
                                                                        ui,
                                                                        "Scale Value",
                                                                        &mut state.series_scales[i],
                                                                        3,
                                                                        0.001,
                                                                        1_000_000.0,
                                                                    );
                                                                });
                                                            },
                                                        );

                                                        ui.allocate_ui_with_layout(
                                                            egui::vec2(right_w, row_h),
                                                            egui::Layout::top_down(
                                                                egui::Align::Center,
                                                            ),
                                                            |ui| {
                                                                Self::knob_control(
                                                                    ui,
                                                                    "Gain",
                                                                    &mut state.series_scales[i],
                                                                    0.001,
                                                                    1_000_000.0,
                                                                    0.08,
                                                                    3,
                                                                );
                                                            },
                                                        );
                                                    },
                                                );
                                            }
                                        });
                                    ui.add_space(2.0);
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.add_space(4.0);
                                        if styled_button(ui, "Apply").clicked() {
                                            ctx.data_mut(|d| d.insert_temp(save_id, true));
                                            ctx.request_repaint();
                                        }
                                    });
                                    ui.add_space(4.0);
                                });
                            ctx.data_mut(|d| {
                                d.insert_temp(state_id, state);
                                d.insert_temp(open_id, open);
                            });
                        }
                        let refresh_hz = plotter.refresh_hz.max(1.0);
                        ctx.request_repaint_after(Duration::from_secs_f64(1.0 / refresh_hz));
                    }
                });
                if self.connection_editor_host == ConnectionEditorHost::PluginWindow(plugin_id) {
                    self.notification_handler.set_active_plugin(Some(plugin_id));
                    self.render_connection_editor(ctx);
                    self.notification_handler.set_active_plugin(None);
                }
                self.render_plotter_notifications(ctx, plugin_id);
            });

            // Check for capture request
            if ctx.data(|d| {
                d.get_temp::<bool>(egui::Id::new(("plotter_export_save", plugin_id)))
                    .unwrap_or(false)
            }) {
                export_saved.push(plugin_id);
                ctx.data_mut(|d| {
                    d.remove::<bool>(egui::Id::new(("plotter_export_save", plugin_id)))
                });
            }
            if ctx.data(|d| {
                d.get_temp::<bool>(egui::Id::new(("plotter_settings_save", plugin_id)))
                    .unwrap_or(false)
            }) {
                settings_saved.push(plugin_id);
                ctx.data_mut(|d| {
                    d.remove::<bool>(egui::Id::new(("plotter_settings_save", plugin_id)))
                });
            }
            if ctx.data(|d| {
                d.get_temp::<bool>(egui::Id::new(("plotter_start_toggle", plugin_id)))
                    .unwrap_or(false)
            }) {
                start_toggle_requested.push(plugin_id);
                ctx.data_mut(|d| {
                    d.remove::<bool>(egui::Id::new(("plotter_start_toggle", plugin_id)))
                });
            }
            if ctx.data(|d| {
                d.get_temp::<bool>(egui::Id::new(("plotter_add_connections", plugin_id)))
                    .unwrap_or(false)
            }) {
                add_connection_requested.push(plugin_id);
                ctx.data_mut(|d| {
                    d.remove::<bool>(egui::Id::new(("plotter_add_connections", plugin_id)));
                });
            }
            if ctx.data(|d| {
                d.get_temp::<bool>(egui::Id::new(("plotter_remove_connections", plugin_id)))
                    .unwrap_or(false)
            }) {
                remove_connection_requested.push(plugin_id);
                ctx.data_mut(|d| {
                    d.remove::<bool>(egui::Id::new(("plotter_remove_connections", plugin_id)));
                });
            }
            if ctx.embed_viewports() {
                let response = egui::Window::new(title)
                    .resizable(true)
                    .default_size(egui::vec2(900.0, 520.0))
                    .show(ctx, |ui| {
                        if let Ok(mut plotter) = plotter.lock() {
                            if let Some((
                                show_axes,
                                show_legend,
                                show_grid,
                                series_names,
                                series_scales,
                                series_offsets,
                                colors,
                                title,
                                dark_theme,
                                x_axis,
                                y_axis,
                                window_ms,
                                _timebase_divisions,
                                _high_quality,
                                _export_svg,
                            )) = self
                                .plotter_manager
                                .plotter_preview_settings
                                .get(&plugin_id)
                                .cloned()
                            {
                                let series_transforms = Self::build_series_transforms(
                                    &series_scales,
                                    &series_offsets,
                                    series_names.len(),
                                );
                                plotter.render_with_settings(
                                    ui,
                                    "",
                                    &self.state_sync.logic_time_label,
                                    show_axes,
                                    show_legend,
                                    show_grid,
                                    Some(&title),
                                    Some(&series_names),
                                    Some(&series_transforms),
                                    Some(&colors),
                                    dark_theme,
                                    Some(&x_axis),
                                    Some(&y_axis),
                                    Some(window_ms),
                                );
                            } else {
                                plotter.render(ui, "", &self.state_sync.logic_time_label);
                            }
                        }
                    });
                if let Some(response) = response {
                    self.window_rects.push(response.response.rect);
                    if !self.confirm_dialog.open
                        && (response.response.clicked() || response.response.dragged())
                    {
                        ctx.move_to_top(response.response.layer_id);
                    }
                }
            }

            let close_requested = ctx.input_for(viewport_id, |i| i.viewport().close_requested());
            if close_requested {
                closed.push(plugin_id);
            }
        }

        for id in closed {
            let open_id = egui::Id::new(("plotter_settings_open", id));
            let state_id = egui::Id::new(("plotter_settings_state", id));
            let save_id = egui::Id::new(("plotter_settings_save", id));
            let export_open_id = egui::Id::new(("plotter_export_open", id));
            let export_state_id = egui::Id::new(("plotter_export_state", id));
            let export_save_id = egui::Id::new(("plotter_export_save", id));
            let export_close_id = egui::Id::new(("plotter_export_close", id));
            ctx.data_mut(|d| {
                d.remove::<bool>(open_id);
                d.remove::<PlotterPreviewState>(state_id);
                d.remove::<bool>(save_id);
                d.remove::<bool>(export_open_id);
                d.remove::<PlotterPreviewState>(export_state_id);
                d.remove::<bool>(export_save_id);
                d.remove::<bool>(export_close_id);
            });
            if self.connection_editor_host == ConnectionEditorHost::PluginWindow(id) {
                self.connection_editor.open = false;
                self.connection_editor.plugin_id = None;
                self.connection_editor_host = ConnectionEditorHost::Main;
            }
            // Just close the plotter window, don't remove the plugin
            if let Some(plotter) = self.plotter_manager.plotters.get(&id) {
                if let Ok(mut plotter) = plotter.lock() {
                    plotter.open = false;
                }
            }
            self.recompute_plotter_ui_hz();
        }

        for plugin_id in export_saved {
            let export_state = egui::Id::new(("plotter_export_state", plugin_id));
            if let Some(state) = ctx.data(|d| d.get_temp::<PlotterPreviewState>(export_state)) {
                self.plotter_preview = state.clone();
                self.apply_plotter_preview_state(plugin_id, &state);
                self.request_plotter_screenshot(plugin_id);
            }
        }
        for plugin_id in settings_saved.iter().copied() {
            let state_id = egui::Id::new(("plotter_settings_state", plugin_id));
            if let Some(state) = ctx.data(|d| d.get_temp::<PlotterPreviewState>(state_id)) {
                self.apply_plotter_preview_state(plugin_id, &state);
            }
        }
        for plugin_id in start_toggle_requested {
            if let Err(err) = self.toggle_plugin_running_from_plotter_window(plugin_id) {
                self.show_plugin_info(plugin_id, "Plugin", &err);
            }
        }
        for plugin_id in add_connection_requested {
            self.selected_plugin_id = Some(plugin_id);
            self.open_connection_editor_in_plugin_window(
                plugin_id,
                plugin_id,
                ConnectionEditMode::Add,
            );
        }
        for plugin_id in remove_connection_requested {
            self.selected_plugin_id = Some(plugin_id);
            self.open_connection_editor_in_plugin_window(
                plugin_id,
                plugin_id,
                ConnectionEditMode::Remove,
            );
        }
    }
}
