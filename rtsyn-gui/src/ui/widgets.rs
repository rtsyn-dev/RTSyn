//! Shared UI widgets and components used across the RTSyn GUI.
//!
//! This module provides reusable UI components that are used in multiple
//! parts of the application, promoting consistency and reducing duplication.

use eframe::egui;
use crate::utils::truncate_string;

/// Renders a key-value row with wrapped label text and custom value UI.
///
/// Creates a horizontal layout with a fixed-width label area that supports text wrapping,
/// followed by a custom UI element for the value. This is commonly used in plugin configuration
/// panels to create consistent label-value pairs.
///
/// # Parameters
/// - `ui`: The egui UI context to render into
/// - `label`: The text label to display (will wrap if too long)
/// - `label_w`: Fixed width allocated for the label area in pixels
/// - `value_ui`: Closure that renders the value UI component
///
/// # Layout
/// The function creates a horizontal layout with:
/// - Fixed-width label area with text wrapping enabled
/// - 8px spacing between label and value
/// - Remaining width allocated to the value UI
pub fn kv_row_wrapped(
    ui: &mut egui::Ui,
    label: &str,
    label_w: f32,
    value_ui: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        let max_chars = ((label_w / 7.0).floor() as usize).max(10);
        let display_label = truncate_string(label, max_chars);
        ui.allocate_ui_with_layout(
            egui::vec2(label_w, 0.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                let response = ui.add_sized(
                    [label_w, 0.0],
                    egui::Label::new(display_label).wrap(false),
                );
                response.on_hover_text(label);
            },
        );
        ui.add_space(8.0);
        value_ui(ui);
    });
}

/// Creates a styled button with consistent appearance.
///
/// # Parameters
/// - `ui`: The egui UI context
/// - `label`: The button label text
///
/// # Returns
/// The response from the button interaction
pub fn styled_button(ui: &mut egui::Ui, label: impl Into<egui::WidgetText>) -> egui::Response {
    ui.add_sized(
        super::BUTTON_SIZE,
        egui::Button::new(label).min_size(super::BUTTON_SIZE),
    )
}
