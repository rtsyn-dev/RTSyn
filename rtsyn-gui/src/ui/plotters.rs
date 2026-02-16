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
use crate::ui_state::PlotterPreviewState;
use std::time::Duration;

impl GuiApp {
    /// Renders an interactive circular knob control for parameter adjustment.
    ///
    /// This function creates a rotary knob widget that allows users to adjust numeric
    /// values through mouse dragging and scroll wheel interaction. The knob provides
    /// visual feedback with a circular indicator and supports fine control modes.
    ///
    /// # Parameters
    /// - `ui`: Mutable reference to the egui UI context for rendering
    /// - `label`: Display label shown above the knob
    /// - `value`: Mutable reference to the value being controlled
    /// - `min`: Minimum allowed value for the parameter
    /// - `max`: Maximum allowed value for the parameter
    /// - `sensitivity`: Base sensitivity multiplier for value changes
    /// - `_decimals`: Number of decimal places (currently unused in implementation)
    ///
    /// # Behavior
    /// - **Drag Control**: Horizontal mouse dragging adjusts the value with variable sensitivity
    /// - **Scroll Wheel**: Mouse wheel provides alternative adjustment method
    /// - **Fine Control**: Holding Shift reduces sensitivity by 80% for precise adjustments
    /// - **Visual Feedback**: Circular knob with position indicator showing current value
    /// - **Deadzone**: Small deadzone prevents accidental adjustments from minor movements
    ///
    /// # Implementation Details
    /// - Uses temporary data storage for drag state management
    /// - Implements progressive sensitivity scaling based on drag distance
    /// - Clamps all values to the specified min/max range
    /// - Requests UI repaints during active adjustment for smooth feedback
    /// - Handles both active pointer tracking and fallback drag state
    fn knob_control(
        ui: &mut egui::Ui,
        label: &str,
        value: &mut f64,
        min: f64,
        max: f64,
        sensitivity: f64,
        _decimals: usize,
    ) {
        ui.vertical_centered(|ui| {
            ui.add_space(2.0);
            ui.label(egui::RichText::new(label).strong());
            let size = egui::vec2(96.0, 96.0);
            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::drag());
            let center = rect.center();
            let radius = rect.width().min(rect.height()) * 0.46;
            let start = -std::f32::consts::PI * 0.75;
            let end = std::f32::consts::PI * 0.75;

            let origin_key = response.id.with("drag_origin");
            let dx_key = response.id.with("drag_dx");
            if response.drag_started() {
                if let Some(pos) = response.interact_pointer_pos() {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(origin_key, pos);
                        d.insert_temp(dx_key, 0.0_f32);
                    });
                }
            }
            if response.dragged() {
                let pointer_pos = response
                    .interact_pointer_pos()
                    .or_else(|| ui.ctx().input(|i| i.pointer.latest_pos()));
                if let Some(pos) = pointer_pos {
                    let origin = ui
                        .ctx()
                        .data(|d| d.get_temp::<egui::Pos2>(origin_key))
                        .unwrap_or(pos);
                    let dx = pos.x - origin.x;
                    ui.ctx().data_mut(|d| d.insert_temp(dx_key, dx));
                    let deadzone = 2.0_f32;
                    if dx.abs() > deadzone {
                        let dir = dx.signum() as f64;
                        let strength = (dx.abs() - deadzone) as f64;
                        // Very fine control near the knob, progressively faster farther away.
                        let t = (strength / 120.0).clamp(0.0, 1.0);
                        let ramp = (0.03 + t.powf(2.2) * 18.0).clamp(0.03, 18.0);
                        let dt = ui.ctx().input(|i| i.stable_dt).max(1.0 / 240.0) as f64;
                        let fine = if ui.ctx().input(|i| i.modifiers.shift) {
                            0.2
                        } else {
                            1.0
                        };
                        *value =
                            (*value + dir * sensitivity * ramp * dt * 60.0 * fine).clamp(min, max);
                        ui.ctx().request_repaint();
                    }
                } else {
                    let dx = ui.ctx().data(|d| d.get_temp::<f32>(dx_key)).unwrap_or(0.0);
                    let deadzone = 2.0_f32;
                    if dx.abs() > deadzone {
                        let dir = dx.signum() as f64;
                        let strength = (dx.abs() - deadzone) as f64;
                        let t = (strength / 120.0).clamp(0.0, 1.0);
                        let ramp = (0.03 + t.powf(2.2) * 18.0).clamp(0.03, 18.0);
                        let dt = ui.ctx().input(|i| i.stable_dt).max(1.0 / 240.0) as f64;
                        let fine = if ui.ctx().input(|i| i.modifiers.shift) {
                            0.2
                        } else {
                            1.0
                        };
                        *value =
                            (*value + dir * sensitivity * ramp * dt * 60.0 * fine).clamp(min, max);
                        ui.ctx().request_repaint();
                    }
                }
            }
            if response.drag_stopped() {
                ui.ctx().data_mut(|d| {
                    d.remove::<egui::Pos2>(origin_key);
                    d.remove::<f32>(dx_key);
                });
            }

            if response.hovered() {
                let wheel = ui.ctx().input(|i| i.smooth_scroll_delta.y);
                if wheel.abs() > f32::EPSILON {
                    let strength = wheel.abs() as f64;
                    let ramp = (1.0 + (strength / 4.0).powf(1.1)).clamp(1.0, 10.0);
                    let dir = wheel.signum() as f64;
                    let fine = if ui.ctx().input(|i| i.modifiers.shift) {
                        0.2
                    } else {
                        1.0
                    };
                    *value = (*value + dir * sensitivity * 0.6 * ramp * fine).clamp(min, max);
                    ui.ctx().request_repaint();
                }
            }

            let t = if max > min {
                ((*value - min) / (max - min)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let angle = start + (end - start) * t as f32;
            let tip = center + egui::vec2(angle.cos(), angle.sin()) * (radius * 0.75);

            let stroke = egui::Stroke::new(1.25, ui.visuals().widgets.active.bg_fill);
            ui.painter()
                .circle_filled(center, radius, ui.visuals().widgets.inactive.bg_fill);
            ui.painter().circle_stroke(center, radius, stroke);
            ui.painter().line_segment(
                [center, tip],
                egui::Stroke::new(2.0, ui.visuals().widgets.active.fg_stroke.color),
            );
            ui.add_space(2.0);
        });
    }

    /// Renders a text input box with scroll wheel support for numeric value editing.
    ///
    /// This function creates a grouped text input field that allows direct text entry
    /// of numeric values while also supporting mouse wheel adjustments. It provides
    /// a clean, centered layout with custom styling.
    ///
    /// # Parameters
    /// - `ui`: Mutable reference to the egui UI context for rendering
    /// - `title`: Display title shown above the input box
    /// - `value`: Mutable reference to the numeric value being edited
    /// - `decimals`: Number of decimal places to display in the formatted value
    /// - `min`: Minimum allowed value (used for clamping parsed input)
    /// - `max`: Maximum allowed value (used for clamping parsed input)
    ///
    /// # Behavior
    /// - **Text Input**: Direct editing of the numeric value as formatted text
    /// - **Input Validation**: Parses text input and clamps to min/max range
    /// - **Formatting**: Displays value with specified decimal precision
    /// - **Styling**: Custom background colors for different interaction states
    /// - **Layout**: Centered text alignment within a fixed-width container
    ///
    /// # Implementation Details
    /// - Uses custom color scheme (dark grays) for visual consistency
    /// - Handles parsing errors gracefully by ignoring invalid input
    /// - Maintains value within specified bounds automatically
    /// - Provides 180px width for consistent layout across different controls
    fn wheel_value_box(
        ui: &mut egui::Ui,
        title: &str,
        value: &mut f64,
        decimals: usize,
        min: f64,
        max: f64,
    ) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_width(220.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new(title).strong());
                ui.add_space(4.0);
                let mut text = format!("{:.*}", decimals, *value);
                ui.scope(|ui| {
                    ui.style_mut().visuals.widgets.inactive.bg_fill = egui::Color32::from_gray(45);
                    ui.style_mut().visuals.widgets.hovered.bg_fill = egui::Color32::from_gray(55);
                    ui.style_mut().visuals.widgets.active.bg_fill = egui::Color32::from_gray(60);
                    if ui
                        .add_sized(
                            [180.0, 0.0],
                            egui::TextEdit::singleline(&mut text)
                                .horizontal_align(egui::Align::Center),
                        )
                        .changed()
                    {
                        if let Ok(parsed) = text.trim().parse::<f64>() {
                            *value = parsed.clamp(min, max);
                        }
                    }
                });
            });
        });
    }

    /// Implements 1-2-5 sequence stepping for timebase and measurement values.
    ///
    /// This function provides standard engineering stepping through values using the
    /// 1-2-5 sequence (1, 2, 5, 10, 20, 50, 100, etc.), which is commonly used in
    /// oscilloscopes and measurement instruments for intuitive value selection.
    ///
    /// # Parameters
    /// - `value`: Current value to step from
    /// - `direction`: Step direction (positive for up, negative for down, zero for no change)
    ///
    /// # Returns
    /// The next value in the 1-2-5 sequence, clamped to the range [0.1, 60000.0]
    ///
    /// # Algorithm
    /// 1. Normalizes the input value to find the current decade (power of 10)
    /// 2. Identifies the closest value in the 1-2-5 sequence within that decade
    /// 3. Steps to the next/previous value in the sequence based on direction
    /// 4. Handles decade transitions when stepping beyond sequence boundaries
    /// 5. Clamps the result to prevent extreme values
    ///
    /// # Examples
    /// - `step_125(1.5, 1)` → `2.0` (next in sequence)
    /// - `step_125(5.0, 1)` → `10.0` (next decade)
    /// - `step_125(2.0, -1)` → `1.0` (previous in sequence)
    ///
    /// # Use Cases
    /// - Timebase control in oscilloscope-like interfaces
    /// - Measurement range selection
    /// - Any application requiring standard engineering value stepping
    fn step_125(value: f64, direction: i32) -> f64 {
        let seq = [1.0_f64, 2.0, 5.0];
        let mut v = value.max(0.1);
        let mut exp = v.log10().floor() as i32;
        let decade = 10_f64.powi(exp);
        v /= decade;

        let mut idx = 0usize;
        let mut best = f64::INFINITY;
        for (i, candidate) in seq.iter().enumerate() {
            let d = (v - *candidate).abs();
            if d < best {
                best = d;
                idx = i;
            }
        }

        if direction > 0 {
            if idx + 1 < seq.len() {
                idx += 1;
            } else {
                idx = 0;
                exp += 1;
            }
        } else if direction < 0 {
            if idx > 0 {
                idx -= 1;
            } else {
                idx = seq.len() - 1;
                exp -= 1;
            }
        }

        (seq[idx] * 10_f64.powi(exp)).clamp(0.1, 60_000.0)
    }

    /// Renders timebase control interface for plot time window configuration.
    ///
    /// This function creates a horizontal control panel that allows users to adjust
    /// the time window displayed in plots using oscilloscope-style timebase controls.
    /// It provides both coarse stepping and fine adjustment capabilities.
    ///
    /// # Parameters
    /// - `ui`: Mutable reference to the egui UI context for rendering
    /// - `window_ms`: Mutable reference to the total time window in milliseconds
    /// - `timebase_divisions`: Mutable reference to the number of time divisions
    ///
    /// # Controls Provided
    /// - **Step Buttons**: Left/right arrows for 1-2-5 sequence stepping
    /// - **Direct Input**: Drag value widget for precise ms/div adjustment
    /// - **Division Count**: Adjustable number of time divisions (1-200)
    /// - **Total Display**: Shows calculated total time window
    ///
    /// # Behavior
    /// - Calculates ms/div from total window and division count
    /// - Uses `step_125()` for standard timebase stepping
    /// - Clamps division count to reasonable range (1-200)
    /// - Updates total window when either parameter changes
    /// - Maintains total window within bounds (100ms to 600s)
    ///
    /// # Layout
    /// The controls are arranged horizontally: Label | ◀ | ▶ | DragValue | "ms/div" | "x" | Divisions | "div" | Separator | Total
    ///
    /// # Implementation Details
    /// - Synchronizes changes between ms/div and total window calculations
    /// - Provides immediate visual feedback for all adjustments
    /// - Uses consistent clamping to prevent invalid configurations
    fn render_timebase_controls(
        ui: &mut egui::Ui,
        window_ms: &mut f64,
        timebase_divisions: &mut u32,
    ) {
        let mut divisions = (*timebase_divisions).clamp(1, 200);
        let mut ms_div = (*window_ms / divisions as f64).clamp(0.1, 60_000.0);
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.label("Timebase:");

            if ui.small_button("◀").clicked() {
                ms_div = Self::step_125(ms_div, -1);
                changed = true;
            }
            if ui.small_button("▶").clicked() {
                ms_div = Self::step_125(ms_div, 1);
                changed = true;
            }

            let mut drag = ms_div;
            if ui
                .add(
                    egui::DragValue::new(&mut drag)
                        .clamp_range(0.1..=60_000.0)
                        .speed((ms_div * 0.05).max(0.1)),
                )
                .changed()
            {
                ms_div = drag.clamp(0.1, 60_000.0);
                changed = true;
            }

            ui.label("ms/div");
            ui.label("x");
            let mut div_drag = i64::from(divisions);
            if ui
                .add(
                    egui::DragValue::new(&mut div_drag)
                        .clamp_range(1..=200)
                        .speed(1.0),
                )
                .changed()
            {
                divisions = div_drag.clamp(1, 200) as u32;
                changed = true;
            }
            ui.label("div");
            ui.separator();
            ui.monospace(format!("{:.1} ms", ms_div * divisions as f64));
        });

        if changed {
            *timebase_divisions = divisions;
            *window_ms = (ms_div * divisions as f64).clamp(100.0, 600_000.0);
        }
    }

    /// Renders scale and offset control knobs for a data series.
    ///
    /// This function creates two interactive knob controls that allow users to adjust
    /// the scaling and DC offset of a data series in real-time. These controls are
    /// essential for normalizing and positioning different data streams for optimal
    /// visualization.
    ///
    /// # Parameters
    /// - `ui`: Mutable reference to the egui UI context for rendering
    /// - `scale`: Mutable reference to the scaling factor (gain) for the series
    /// - `offset`: Mutable reference to the DC offset value for the series
    ///
    /// # Controls
    /// - **Scale Knob**: Adjusts multiplicative scaling from 0.001 to 1,000,000
    /// - **Offset Knob**: Adjusts additive offset from -1 billion to +1 billion
    ///
    /// # Behavior
    /// - Scale control uses logarithmic-style sensitivity for wide range coverage
    /// - Offset control uses linear sensitivity appropriate for typical signal ranges
    /// - Both knobs support fine adjustment mode (Shift key) and scroll wheel input
    /// - Scale is automatically clamped to prevent zero or negative values
    ///
    /// # Use Cases
    /// - Normalizing signals with different amplitude ranges
    /// - Removing DC bias from AC signals
    /// - Scaling engineering units to display units
    /// - Vertically positioning multiple traces for comparison
    ///
    /// # Implementation Details
    /// - Ensures scale never goes to zero (minimum 0.001)
    /// - Uses appropriate sensitivity values for each parameter type
    /// - Provides immediate visual feedback through knob position indicators
    fn render_series_wheels(ui: &mut egui::Ui, scale: &mut f64, offset: &mut f64) {
        Self::knob_control(ui, "Scale", scale, 0.001, 1_000_000.0, 0.08, 3);
        Self::knob_control(
            ui,
            "Offset",
            offset,
            -1_000_000_000.0,
            1_000_000_000.0,
            1.0,
            1,
        );
        if *scale <= 0.0 {
            *scale = 0.001;
        }
    }

    /// Determines if a series name is a placeholder generated by the system.
    ///
    /// This function identifies automatically generated series names that follow the
    /// pattern "Series N" where N is a number. This distinction is important for
    /// deciding whether to replace placeholder names with more meaningful names
    /// derived from actual data connections.
    ///
    /// # Parameters
    /// - `name`: The series name string to check
    ///
    /// # Returns
    /// `true` if the name matches the placeholder pattern "Series N", `false` otherwise
    ///
    /// # Pattern Recognition
    /// - Trims whitespace from the input name
    /// - Checks for exact "Series " prefix
    /// - Validates that the suffix is a parseable positive integer
    /// - Case-sensitive matching (requires exact capitalization)
    ///
    /// # Use Cases
    /// - Determining when to auto-update series names from connection metadata
    /// - Preserving user-customized series names while updating defaults
    /// - Managing the transition from placeholder to meaningful names
    ///
    /// # Examples
    /// - `"Series 1"` → `true`
    /// - `"Series 42"` → `true`
    /// - `"Custom Name"` → `false`
    /// - `"series 1"` → `false` (case mismatch)
    /// - `"Series ABC"` → `false` (non-numeric suffix)
    fn is_placeholder_series_name(name: &str) -> bool {
        let trimmed = name.trim();
        if !trimmed.starts_with("Series ") {
            return false;
        }
        trimmed["Series ".len()..].parse::<usize>().is_ok()
    }

    /// Synchronizes series control arrays with a seed state, preserving user customizations.
    ///
    /// This function ensures that the series control arrays (names, scales, offsets, colors)
    /// in the target state match the length and structure of a seed state, while intelligently
    /// preserving user customizations and replacing only placeholder values with meaningful
    /// data from the seed.
    ///
    /// # Parameters
    /// - `state`: Mutable reference to the state being synchronized
    /// - `seed`: Reference to the seed state providing the target structure and default values
    ///
    /// # Synchronization Logic
    /// 1. **Size Matching**: Adjusts array lengths to match the seed state
    /// 2. **Expansion**: Adds new entries with seed values when target is smaller
    /// 3. **Truncation**: Removes excess entries when target is larger
    /// 4. **Smart Name Updates**: Replaces placeholder names with seed names while preserving custom names
    ///
    /// # Preservation Rules
    /// - User-defined custom series names are never overwritten
    /// - Placeholder names (e.g., "Series 1") are replaced with meaningful seed names
    /// - Empty names are always replaced with seed names when available
    /// - Scale, offset, and color values are preserved for existing series
    ///
    /// # Use Cases
    /// - Updating plotter settings when connection topology changes
    /// - Maintaining user customizations across plugin reconfigurations
    /// - Initializing new series with sensible defaults
    /// - Handling dynamic changes in the number of data series
    ///
    /// # Implementation Details
    /// - Uses `is_placeholder_series_name()` to identify replaceable names
    /// - Maintains array consistency across all series-related vectors
    /// - Handles edge cases like empty seed states gracefully
    fn sync_series_controls_from_seed(state: &mut PlotterPreviewState, seed: &PlotterPreviewState) {
        let target = seed.series_names.len();
        if state.series_names.len() < target {
            for idx in state.series_names.len()..target {
                state.series_names.push(
                    seed.series_names
                        .get(idx)
                        .cloned()
                        .unwrap_or_else(|| format!("Series {}", idx + 1)),
                );
                state.series_scales.push(1.0);
                state.series_offsets.push(0.0);
                state.colors.push(
                    seed.colors
                        .get(idx)
                        .copied()
                        .unwrap_or(egui::Color32::from_rgb(86, 156, 214)),
                );
            }
        } else {
            state.series_names.truncate(target);
            state.series_scales.truncate(target);
            state.series_offsets.truncate(target);
            state.colors.truncate(target);
        }
        // Keep user-defined custom labels, but replace placeholder fallback labels
        // with concrete seed names once real connections/series metadata is available.
        for idx in 0..target {
            if let (Some(current), Some(seed_name)) =
                (state.series_names.get_mut(idx), seed.series_names.get(idx))
            {
                if (current.is_empty() || Self::is_placeholder_series_name(current))
                    && !seed_name.is_empty()
                {
                    *current = seed_name.clone();
                }
            }
        }
    }

    /// Generates meaningful series names based on plugin connections.
    ///
    /// This function creates descriptive names for data series by examining the
    /// connection topology and generating names that reflect the actual data sources.
    /// It provides much more informative labels than generic placeholder names.
    ///
    /// # Parameters
    /// - `plugin_id`: The ID of the plugin for which to generate series names
    /// - `count`: The number of series names to generate
    ///
    /// # Returns
    /// A `Vec<String>` containing generated series names, one for each requested series
    ///
    /// # Name Generation Logic
    /// For each series index `i`:
    /// 1. Constructs the expected input port name as `"in_{i}"`
    /// 2. Searches workspace connections for a connection to this port
    /// 3. If found, creates name as `"{source_plugin}:{source_port}"`
    /// 4. If not found, falls back to placeholder `"Series {i+1}"`
    ///
    /// # Examples
    /// - Connected input: `"SignalGen:output"` (meaningful)
    /// - Unconnected input: `"Series 2"` (placeholder)
    ///
    /// # Use Cases
    /// - Providing context-aware series labels in plot legends
    /// - Helping users identify data sources in multi-series plots
    /// - Automatically updating labels when connections change
    /// - Improving plot readability and documentation value
    ///
    /// # Implementation Details
    /// - Uses workspace connection metadata for name resolution
    /// - Handles missing connections gracefully with fallback names
    /// - Maintains consistent indexing (1-based for user display)
    /// - Leverages `plugin_display_name()` for readable plugin names
    fn aligned_series_names(&self, plugin_id: u64, count: usize) -> Vec<String> {
        (0..count)
            .map(|i| {
                let port = format!("in_{i}");
                self.workspace_manager
                    .workspace
                    .connections
                    .iter()
                    .find(|c| c.to_plugin == plugin_id && c.to_port == port)
                    .map(|c| {
                        format!(
                            "{}:{}",
                            self.plugin_display_name(c.from_plugin),
                            c.from_port
                        )
                    })
                    .unwrap_or_else(|| format!("Series {}", i + 1))
            })
            .collect()
    }

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
    fn render_plotter_notifications(&mut self, ctx: &egui::Context, plugin_id: u64) {
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
    fn toggle_plugin_running_from_plotter_window(
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
                .with_inner_size([900.0, 520.0])
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
                                egui::vec2((k * 16.0).max(420.0), (k * 9.0).max(320.0));
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
                                    if styled_button(ui, "Apply").clicked() {
                                        ctx.data_mut(|d| d.insert_temp(save_id, true));
                                        ctx.request_repaint();
                                    }
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

    /// Constructs a complete plotter preview state from current plugin configuration.
    ///
    /// This function builds a comprehensive state object that contains all the settings
    /// and metadata needed to render and configure a plotter. It intelligently merges
    /// saved settings with live plugin data and connection information.
    ///
    /// # Parameters
    /// - `plugin_id`: The ID of the plugin to build preview state for
    ///
    /// # Returns
    /// A `PlotterPreviewState` containing all plotter configuration and display settings
    ///
    /// # State Construction Process
    /// 1. **Initialize**: Creates default state with target plugin ID
    /// 2. **Live Data**: Extracts current series count and connection names
    /// 3. **Saved Settings**: Loads previously saved plotter configuration if available
    /// 4. **Series Sync**: Synchronizes series arrays with current connection topology
    /// 5. **Plugin Config**: Incorporates plugin-specific settings (priority, refresh rate)
    /// 6. **Defaults**: Applies sensible defaults for missing configuration
    ///
    /// # Data Sources
    /// - **Plotter Manager**: Saved preview settings and live plotter state
    /// - **Workspace**: Plugin configuration and connection topology
    /// - **Connection Names**: Generated from `aligned_series_names()`
    /// - **Default Values**: Fallback settings for new or unconfigured plotters
    ///
    /// # Series Management
    /// - Automatically adjusts series count to match live connections
    /// - Preserves user-customized series names and settings
    /// - Assigns default colors using a predefined palette
    /// - Ensures minimum of one series for UI consistency
    ///
    /// # Default Configuration
    /// - **Visual**: Axes, legend, and grid enabled; dark theme
    /// - **Timebase**: 10 divisions, window from live plotter
    /// - **Series**: Auto-generated names, 1.0 scale, 0.0 offset
    /// - **Colors**: 8-color palette cycling for multiple series
    /// - **Export**: 1920x1080 PNG format
    ///
    /// # Use Cases
    /// - Initializing settings dialogs with current configuration
    /// - Preparing export operations with live data
    /// - Synchronizing UI state with plugin changes
    /// - Providing consistent defaults for new plotters
    fn build_plotter_preview_state(&self, plugin_id: u64) -> PlotterPreviewState {
        let mut state = PlotterPreviewState::default();
        state.target = Some(plugin_id);
        let live_count = self
            .plotter_manager
            .plotters
            .get(&plugin_id)
            .and_then(|p| p.lock().ok().map(|p| p.input_count))
            .unwrap_or(0);
        let live_names = self.aligned_series_names(plugin_id, live_count);
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
            timebase_divisions,
            high_quality,
            export_svg,
        )) = self
            .plotter_manager
            .plotter_preview_settings
            .get(&plugin_id)
            .cloned()
        {
            state.show_axes = show_axes;
            state.show_legend = show_legend;
            state.show_grid = show_grid;
            state.series_names = series_names;
            state.series_scales = series_scales;
            state.series_offsets = series_offsets;
            state.colors = colors;
            state.title = title;
            state.dark_theme = dark_theme;
            state.x_axis_name = x_axis;
            state.y_axis_name = y_axis;
            state.window_ms = window_ms;
            state.timebase_divisions = timebase_divisions.clamp(1, 200);
            state.high_quality = high_quality;
            state.export_svg = export_svg;
            if live_count > 0 {
                if state.series_names.len() < live_count {
                    for i in state.series_names.len()..live_count {
                        state.series_names.push(
                            live_names
                                .get(i)
                                .cloned()
                                .unwrap_or_else(|| format!("Series {}", i + 1)),
                        );
                        state.series_scales.push(1.0);
                        state.series_offsets.push(0.0);
                        state.colors.push(match i % 8 {
                            0 => egui::Color32::from_rgb(86, 156, 214),
                            1 => egui::Color32::from_rgb(220, 122, 95),
                            2 => egui::Color32::from_rgb(181, 206, 168),
                            3 => egui::Color32::from_rgb(220, 220, 170),
                            4 => egui::Color32::from_rgb(197, 134, 192),
                            5 => egui::Color32::from_rgb(78, 201, 176),
                            6 => egui::Color32::from_rgb(156, 220, 254),
                            _ => egui::Color32::from_rgb(255, 206, 84),
                        });
                    }
                } else if state.series_names.len() > live_count {
                    state.series_names.truncate(live_count);
                    state.series_scales.truncate(live_count);
                    state.series_offsets.truncate(live_count);
                    state.colors.truncate(live_count);
                }
                for (i, n) in live_names.into_iter().enumerate() {
                    if i < state.series_names.len() {
                        state.series_names[i] = n;
                    }
                }
            }
            if state.series_names.is_empty() {
                state.series_names.push("Series 1".to_string());
                state.series_scales.push(1.0);
                state.series_offsets.push(0.0);
                state.colors.push(egui::Color32::from_rgb(86, 156, 214));
            }
            return state;
        }
        if let Some(plotter) = self.plotter_manager.plotters.get(&plugin_id) {
            if let Ok(plotter) = plotter.lock() {
                state.show_axes = true;
                state.show_legend = true;
                state.show_grid = true;
                state.title = String::new();
                state.dark_theme = true;
                state.x_axis_name = self.state_sync.logic_time_label.clone();
                state.y_axis_name = "value".to_string();
                state.high_quality = false;
                state.series_names = (0..plotter.input_count)
                    .map(|i| {
                        live_names
                            .get(i)
                            .cloned()
                            .unwrap_or_else(|| format!("Series {}", i + 1))
                    })
                    .collect();
                state.series_scales = vec![1.0; plotter.input_count];
                state.series_offsets = vec![0.0; plotter.input_count];
                state.window_ms = plotter.window_ms;
                state.timebase_divisions = 10;
                state.refresh_hz = plotter.refresh_hz;
                state.colors = (0..plotter.input_count)
                    .map(|i| match i % 8 {
                        0 => egui::Color32::from_rgb(86, 156, 214),
                        1 => egui::Color32::from_rgb(220, 122, 95),
                        2 => egui::Color32::from_rgb(181, 206, 168),
                        3 => egui::Color32::from_rgb(220, 220, 170),
                        4 => egui::Color32::from_rgb(197, 134, 192),
                        5 => egui::Color32::from_rgb(78, 201, 176),
                        6 => egui::Color32::from_rgb(156, 220, 254),
                        _ => egui::Color32::from_rgb(255, 206, 84),
                    })
                    .collect();
            }
        }
        if let Some(plugin) = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .find(|plugin| plugin.id == plugin_id)
        {
            state.priority = plugin.priority;
            state.refresh_hz = plugin
                .config
                .get("refresh_hz")
                .and_then(|v| v.as_f64())
                .unwrap_or(state.refresh_hz);
        }
        if state.series_names.is_empty() {
            state.series_names.push("Series 1".to_string());
            state.series_scales.push(1.0);
            state.series_offsets.push(0.0);
            state.colors.push(egui::Color32::from_rgb(86, 156, 214));
        }
        state
    }

    /// Applies a plotter preview state to persistent storage and plugin configuration.
    ///
    /// This function takes a complete plotter preview state and persists it to the
    /// appropriate storage locations, updating both the plotter manager's settings
    /// and the plugin's workspace configuration. It ensures that user customizations
    /// are preserved across application sessions.
    ///
    /// # Parameters
    /// - `plugin_id`: The ID of the plugin to apply settings to
    /// - `state`: The preview state containing all settings to apply
    ///
    /// # Storage Updates
    /// 1. **Plotter Manager**: Stores visual and series settings in preview settings map
    /// 2. **Plugin Config**: Updates plugin priority and refresh rate in workspace
    /// 3. **Workspace Persistence**: Marks workspace as dirty for automatic saving
    /// 4. **Logic Engine**: Sends updated workspace to background processing
    ///
    /// # Settings Applied
    /// - **Visual Settings**: Axes, legend, grid visibility, theme, titles
    /// - **Series Configuration**: Names, scales, offsets, colors
    /// - **Timebase Settings**: Window duration, division count
    /// - **Export Options**: Quality settings, format preferences
    /// - **Plugin Parameters**: Priority level, refresh rate
    ///
    /// # Data Validation
    /// - Clamps timebase divisions to valid range (1-200)
    /// - Ensures refresh rate is at least 1.0 Hz
    /// - Validates all numeric parameters are within reasonable bounds
    ///
    /// # Side Effects
    /// - Triggers workspace persistence mechanism
    /// - Updates live plugin configuration in logic engine
    /// - May affect plugin execution priority and timing
    /// - Influences plotter rendering performance and quality
    ///
    /// # Use Cases
    /// - Saving user customizations from settings dialogs
    /// - Applying export configurations before image capture
    /// - Persisting changes made through interactive controls
    /// - Batch updating multiple plotter configurations
    ///
    /// # Implementation Notes
    /// - Uses tuple storage format for efficient serialization
    /// - Handles missing plugins gracefully without errors
    /// - Maintains backward compatibility with existing settings
    /// - Ensures atomic updates to prevent inconsistent state
    fn apply_plotter_preview_state(&mut self, plugin_id: u64, state: &PlotterPreviewState) {
        self.plotter_manager.plotter_preview_settings.insert(
            plugin_id,
            (
                state.show_axes,
                state.show_legend,
                state.show_grid,
                state.series_names.clone(),
                state.series_scales.clone(),
                state.series_offsets.clone(),
                state.colors.clone(),
                state.title.clone(),
                state.dark_theme,
                state.x_axis_name.clone(),
                state.y_axis_name.clone(),
                state.window_ms,
                state.timebase_divisions.clamp(1, 200),
                state.high_quality,
                state.export_svg,
            ),
        );
        if let Some(plugin) = self
            .workspace_manager
            .workspace
            .plugins
            .iter_mut()
            .find(|plugin| plugin.id == plugin_id)
        {
            plugin.priority = state.priority;
            if let Some(config) = plugin.config.as_object_mut() {
                config.insert(
                    "refresh_hz".to_string(),
                    Value::from(state.refresh_hz.max(1.0)),
                );
            }
            self.mark_workspace_dirty();
            let _ = self.state_sync.logic_tx.send(LogicMessage::UpdateWorkspace(
                self.workspace_manager.workspace.clone(),
            ));
        }
    }

    /// Constructs series transform objects from scale and offset arrays.
    ///
    /// This utility function converts separate arrays of scaling factors and offset
    /// values into a vector of `SeriesTransform` objects that can be used by the
    /// plotter rendering system. It handles array length mismatches gracefully.
    ///
    /// # Parameters
    /// - `scales`: Array of scaling factors for each series
    /// - `offsets`: Array of offset values for each series
    /// - `count`: Number of transform objects to create
    ///
    /// # Returns
    /// A `Vec<SeriesTransform>` containing transform objects for plotter rendering
    ///
    /// # Transform Construction
    /// For each series index from 0 to `count`:
    /// - Uses the corresponding scale value, or defaults to 1.0 if array is too short
    /// - Uses the corresponding offset value, or defaults to 0.0 if array is too short
    /// - Creates a `SeriesTransform` object with these values
    ///
    /// # Default Behavior
    /// - **Missing Scale**: Defaults to 1.0 (no scaling)
    /// - **Missing Offset**: Defaults to 0.0 (no offset)
    /// - **Empty Arrays**: Creates transforms with default values
    ///
    /// # Use Cases
    /// - Converting UI control values to plotter-compatible format
    /// - Preparing transform data for plot rendering
    /// - Handling dynamic series count changes
    /// - Providing consistent transform objects regardless of input array lengths
    ///
    /// # Mathematical Application
    /// Each transform applies the formula: `output = (input * scale) + offset`
    /// - Scale adjusts the amplitude/magnitude of the signal
    /// - Offset shifts the signal vertically (DC bias adjustment)
    ///
    /// # Implementation Details
    /// - Uses safe array indexing with fallback defaults
    /// - Creates exactly `count` transform objects regardless of input lengths
    /// - Maintains consistent behavior for edge cases
    /// - Optimized for frequent calls during UI updates
    fn build_series_transforms(
        scales: &[f64],
        offsets: &[f64],
        count: usize,
    ) -> Vec<crate::plotter::SeriesTransform> {
        (0..count)
            .map(|i| crate::plotter::SeriesTransform {
                scale: *scales.get(i).unwrap_or(&1.0),
                offset: *offsets.get(i).unwrap_or(&0.0),
            })
            .collect()
    }

    /// Renders the plotter preview and export dialog window.
    ///
    /// This function displays a comprehensive dialog that allows users to customize
    /// plot appearance, configure series settings, and export plots as images.
    /// It provides real-time preview of changes and extensive customization options.
    ///
    /// # Parameters
    /// - `ctx`: The egui context for rendering the dialog
    ///
    /// # Dialog Sections
    /// 1. **Basic Settings**: Title, axes visibility, legend, grid, theme
    /// 2. **Axis Configuration**: X and Y axis labels and formatting
    /// 3. **Timebase Controls**: Time window and division settings
    /// 4. **Series Customization**: Individual series names, colors, and transforms
    /// 5. **Live Preview**: Real-time plot preview with current settings
    /// 6. **Export Options**: Resolution, format, and quality settings
    ///
    /// # Interactive Features
    /// - **Real-time Preview**: Shows immediate feedback for all setting changes
    /// - **Series Tuning**: Individual scale and offset controls for each series
    /// - **Color Picker**: Full color customization for each data series
    /// - **Resolution Control**: Automatic aspect ratio maintenance for exports
    /// - **Format Selection**: PNG or SVG export options
    ///
    /// # State Management
    /// - Synchronizes with live plugin data automatically
    /// - Preserves user customizations across dialog sessions
    /// - Handles dynamic series count changes gracefully
    /// - Maintains consistent state between preview and export
    ///
    /// # Export Configuration
    /// - **Resolution**: Configurable width/height with aspect ratio locking
    /// - **Format**: PNG (raster) or SVG (vector) output options
    /// - **Quality**: High-quality rendering options for publication use
    /// - **Aspect Ratio**: Automatic 16:9 ratio maintenance with manual override
    ///
    /// # User Experience
    /// - **Responsive Layout**: Adapts to different window sizes
    /// - **Scrollable Content**: Handles large numbers of series gracefully
    /// - **Immediate Feedback**: All changes reflected in preview instantly
    /// - **Intuitive Controls**: Familiar UI patterns for all interactions
    ///
    /// # Implementation Details
    /// - Uses `build_plotter_preview_state()` for initialization
    /// - Applies `sync_series_controls_from_seed()` for consistency
    /// - Triggers screenshot requests through `request_plotter_screenshot()`
    /// - Maintains dialog state in `self.plotter_preview`
    ///
    /// # Performance Considerations
    /// - Efficient preview rendering with minimal overhead
    /// - Optimized for real-time interaction feedback
    /// - Handles multiple concurrent dialogs efficiently
    pub(crate) fn render_plotter_preview_dialog(&mut self, ctx: &egui::Context) {
        if !self.plotter_preview.open {
            return;
        }

        let Some(plugin_id) = self.plotter_preview.target else {
            return;
        };
        let seed = self.build_plotter_preview_state(plugin_id);
        Self::sync_series_controls_from_seed(&mut self.plotter_preview, &seed);

        let mut save_requested = false;

        egui::Window::new("Plot Preview & Export")
            .resizable(true)
            .default_size(egui::vec2(600.0, 500.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Title:");
                    ui.text_edit_singleline(&mut self.plotter_preview.title);
                });

                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.plotter_preview.show_axes, "Show axes");
                    ui.checkbox(&mut self.plotter_preview.show_legend, "Show legend");
                    ui.checkbox(&mut self.plotter_preview.show_grid, "Show grid");
                    ui.checkbox(&mut self.plotter_preview.dark_theme, "Dark theme");
                });

                ui.horizontal(|ui| {
                    ui.label("X-axis:");
                    ui.text_edit_singleline(&mut self.plotter_preview.x_axis_name);
                    ui.label("Y-axis:");
                    ui.text_edit_singleline(&mut self.plotter_preview.y_axis_name);
                });
                Self::render_timebase_controls(
                    ui,
                    &mut self.plotter_preview.window_ms,
                    &mut self.plotter_preview.timebase_divisions,
                );

                ui.separator();
                ui.label("Series customization:");

                egui::ScrollArea::vertical()
                    .max_height(150.0)
                    .show(ui, |ui| {
                        while self.plotter_preview.series_scales.len()
                            < self.plotter_preview.series_names.len()
                        {
                            self.plotter_preview.series_scales.push(1.0);
                        }
                        while self.plotter_preview.series_offsets.len()
                            < self.plotter_preview.series_names.len()
                        {
                            self.plotter_preview.series_offsets.push(0.0);
                        }
                        for i in 0..self.plotter_preview.series_names.len() {
                            ui.horizontal(|ui| {
                                ui.label(format!("Series {}:", i + 1));
                                ui.text_edit_singleline(&mut self.plotter_preview.series_names[i]);
                                ui.menu_button("Tune", |ui| {
                                    ui.set_min_width(220.0);
                                    Self::render_series_wheels(
                                        ui,
                                        &mut self.plotter_preview.series_scales[i],
                                        &mut self.plotter_preview.series_offsets[i],
                                    );
                                });
                                ui.color_edit_button_srgba(&mut self.plotter_preview.colors[i]);
                            });
                        }
                    });

                ui.separator();

                // Preview area
                ui.label("Preview:");
                let preview_rect = ui.available_rect_before_wrap();
                let preview_size = egui::vec2(preview_rect.width(), 200.0);

                if let Some(plotter) = self.plotter_manager.plotters.get(&plugin_id) {
                    if let Ok(mut plotter) = plotter.lock() {
                        let series_transforms = Self::build_series_transforms(
                            &self.plotter_preview.series_scales,
                            &self.plotter_preview.series_offsets,
                            self.plotter_preview.series_names.len(),
                        );
                        ui.allocate_ui(preview_size, |ui| {
                            let input_count = plotter.input_count;
                            let refresh_hz = plotter.refresh_hz;
                            plotter.set_window_ms(self.plotter_preview.window_ms);
                            plotter.update_config(
                                input_count,
                                refresh_hz,
                                self.state_sync.logic_period_seconds,
                            );
                            plotter.render_with_settings(
                                ui,
                                "Preview",
                                &self.state_sync.logic_time_label,
                                self.plotter_preview.show_axes,
                                self.plotter_preview.show_legend,
                                self.plotter_preview.show_grid,
                                Some(&self.plotter_preview.title),
                                Some(&self.plotter_preview.series_names),
                                Some(&series_transforms),
                                Some(&self.plotter_preview.colors),
                                self.plotter_preview.dark_theme,
                                Some(&self.plotter_preview.x_axis_name),
                                Some(&self.plotter_preview.y_axis_name),
                                Some(self.plotter_preview.window_ms),
                            );
                        });
                    }
                }

                ui.separator();

                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.plotter_preview.export_svg, "Export as SVG");
                });

                ui.horizontal(|ui| {
                    ui.label("Resolution:");
                    let old_width = self.plotter_preview.width;
                    ui.add_enabled(
                        !self.plotter_preview.export_svg,
                        egui::DragValue::new(&mut self.plotter_preview.width)
                            .clamp_range(400..=4000)
                            .suffix("px"),
                    );

                    // Update height proportionally if width changed
                    if self.plotter_preview.width != old_width && !self.plotter_preview.export_svg {
                        let ratio = 16.0 / 9.0;
                        self.plotter_preview.height =
                            (self.plotter_preview.width as f32 / ratio) as u32;
                    }

                    ui.label("×");
                    let old_height = self.plotter_preview.height;
                    ui.add_enabled(
                        !self.plotter_preview.export_svg,
                        egui::DragValue::new(&mut self.plotter_preview.height)
                            .clamp_range(300..=3000)
                            .suffix("px"),
                    );

                    // Update width proportionally if height changed
                    if self.plotter_preview.height != old_height && !self.plotter_preview.export_svg
                    {
                        let ratio = 16.0 / 9.0;
                        self.plotter_preview.width =
                            (self.plotter_preview.height as f32 * ratio) as u32;
                    }

                    if styled_button(ui, "16:9").clicked() && !self.plotter_preview.export_svg {
                        let ratio = 16.0 / 9.0;
                        self.plotter_preview.height =
                            (self.plotter_preview.width as f32 / ratio) as u32;
                    }
                });

                ui.horizontal(|ui| {
                    if styled_button(ui, "Save").clicked() {
                        save_requested = true;
                    }
                    if styled_button(ui, "Cancel").clicked() {
                        self.plotter_preview.open = false;
                    }
                });
            });

        // Save settings only on explicit action
        if save_requested {
            if let Some(plugin_id) = self.plotter_preview.target {
                self.plotter_manager.plotter_preview_settings.insert(
                    plugin_id,
                    (
                        self.plotter_preview.show_axes,
                        self.plotter_preview.show_legend,
                        self.plotter_preview.show_grid,
                        self.plotter_preview.series_names.clone(),
                        self.plotter_preview.series_scales.clone(),
                        self.plotter_preview.series_offsets.clone(),
                        self.plotter_preview.colors.clone(),
                        self.plotter_preview.title.clone(),
                        self.plotter_preview.dark_theme,
                        self.plotter_preview.x_axis_name.clone(),
                        self.plotter_preview.y_axis_name.clone(),
                        self.plotter_preview.window_ms,
                        self.plotter_preview.timebase_divisions.clamp(1, 200),
                        self.plotter_preview.high_quality,
                        self.plotter_preview.export_svg,
                    ),
                );
            }
        }

        if save_requested {
            self.request_plotter_screenshot(plugin_id);
            // Keep dialog open after saving
        }
    }
}
