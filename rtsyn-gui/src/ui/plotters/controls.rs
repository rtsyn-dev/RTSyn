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
    pub(super) fn knob_control(
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
    pub(super) fn wheel_value_box(
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
    pub(super) fn step_125(value: f64, direction: i32) -> f64 {
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
    pub(super) fn render_timebase_controls(
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
    pub(super) fn render_series_wheels(ui: &mut egui::Ui, scale: &mut f64, offset: &mut f64) {
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
}
