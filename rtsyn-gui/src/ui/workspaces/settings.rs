//! Workspace management and UI rendering functionality for RTSyn GUI.
//!
//! This module provides comprehensive workspace management capabilities including:
//! - Workspace creation, loading, saving, and deletion
//! - UML diagram generation and rendering with PlantUML integration
//! - Runtime settings configuration and persistence
//! - File dialog management for workspace operations
//! - Help system and documentation display
//! - Modal dialogs for user confirmation and information display
//!
//! The module handles both the UI rendering and the underlying workspace operations,
//! integrating with the RTSyn core workspace system and runtime engine.

use super::*;
use crate::WindowFocus;
use rtsyn_core::workspace::{
    RUNTIME_MAX_INTEGRATION_STEPS_MAX, RUNTIME_MAX_INTEGRATION_STEPS_MIN,
    RUNTIME_MIN_FREQUENCY_VALUE, RUNTIME_MIN_PERIOD_VALUE,
};
use rtsyn_runtime::LogicSettings;

impl GuiApp {
/// Renders the workspace runtime settings configuration window.
    ///
    /// This function displays a comprehensive interface for configuring runtime
    /// settings including CPU core selection, timing parameters (frequency/period),
    /// and integration step limits. It provides real-time validation and
    /// bidirectional conversion between frequency and period values.
    ///
    /// # Parameters
    /// - `ctx`: egui context for rendering UI elements and handling interactions
    ///
    /// # Side Effects
    /// - Renders fixed-size window (420x240) with settings controls
    /// - Updates runtime settings draft state during user interaction
    /// - Applies settings to runtime engine when Apply button is clicked
    /// - Saves settings to workspace or defaults when Save button is clicked
    /// - Restores default settings when factory reset is requested
    /// - Shows informational messages about operation results
    /// - Updates logic engine settings through message passing
    ///
    /// # Implementation Details
    /// - Core selection: Multi-checkbox interface with automatic fallback to core 0
    /// - Timing controls: Synchronized frequency/period inputs with unit conversion
    /// - Integration steps: Drag value with enforced min/max constraints
    /// - Draft system: Maintains temporary state until apply/save operations
    /// - Validation: Enforces minimum values and automatic unit conversions
    /// - Persistence: Supports both workspace-specific and global default settings
    /// - Real-time updates: Immediately applies changes to runtime engine
    pub(crate) fn render_workspace_settings_window(&mut self, ctx: &egui::Context) {
        if !self.workspace_settings.open {
            return;
        }

        let mut open = self.workspace_settings.open;
        let window_size = egui::vec2(420.0, 240.0);
        let default_pos = Self::center_window(ctx, window_size);
        let mut draft = self
            .workspace_settings
            .draft
            .unwrap_or(WorkspaceSettingsDraft {
                frequency_value: self.frequency_value,
                frequency_unit: self.frequency_unit,
                period_value: self.period_value,
                period_unit: self.period_unit,
                tab: self.workspace_settings.tab,
                max_integration_steps: 10, // Default reasonable limit
            });
        let mut apply_clicked = false;
        let mut save_clicked = false;
        let mut factory_reset_clicked = false;
        let response = egui::Window::new("Runtime settings")
            .open(&mut open)
            .resizable(false)
            .default_pos(default_pos)
            .default_size(window_size)
            .fixed_size(window_size)
            .show(ctx, |ui| {
                ui.label("Cores (select exact cores)");
                let mut any_selected = false;
                egui::ScrollArea::vertical()
                    .max_height(80.0)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| {
                            for idx in 0..self.available_cores {
                                let label = format!("Core {idx}");
                                if idx >= self.selected_cores.len() {
                                    self.selected_cores.push(false);
                                }
                                ui.checkbox(&mut self.selected_cores[idx], label);
                                if self.selected_cores[idx] {
                                    any_selected = true;
                                }
                            }
                        });
                    });
                if !any_selected && !self.selected_cores.is_empty() {
                    self.selected_cores[0] = true;
                }

                ui.separator();

                ui.vertical(|ui| {
                    let mut period_changed = false;
                    let mut frequency_changed = false;
                    let period_seconds_from = |value: f64, unit: PeriodUnit| -> f64 {
                        match unit {
                            PeriodUnit::Ns => value * 1e-9,
                            PeriodUnit::Us => value * 1e-6,
                            PeriodUnit::Ms => value * 1e-3,
                            PeriodUnit::S => value,
                        }
                    };
                    let frequency_hz_from = |value: f64, unit: FrequencyUnit| -> f64 {
                        match unit {
                            FrequencyUnit::Hz => value,
                            FrequencyUnit::KHz => value * 1_000.0,
                            FrequencyUnit::MHz => value * 1_000_000.0,
                        }
                    };
                    let set_period_from_seconds =
                        |draft: &mut WorkspaceSettingsDraft, period_s: f64| {
                            let period_s = period_s.max(0.0);
                            if period_s >= 1.0 {
                                draft.period_unit = PeriodUnit::S;
                                draft.period_value = (period_s / 1.0).round().max(1.0);
                            } else if period_s >= 1e-3 {
                                draft.period_unit = PeriodUnit::Ms;
                                draft.period_value = (period_s / 1e-3).round().max(1.0);
                            } else if period_s >= 1e-6 {
                                draft.period_unit = PeriodUnit::Us;
                                draft.period_value = (period_s / 1e-6).round().max(1.0);
                            } else {
                                draft.period_unit = PeriodUnit::Ns;
                                draft.period_value = (period_s / 1e-9).round().max(1.0);
                            }
                        };
                    let set_frequency_from_hz = |draft: &mut WorkspaceSettingsDraft, hz: f64| {
                        let hz = hz.max(0.0);
                        if hz >= 1_000_000.0 {
                            draft.frequency_unit = FrequencyUnit::MHz;
                            draft.frequency_value = (hz / 1_000_000.0).round().max(1.0);
                        } else if hz >= 1_000.0 {
                            draft.frequency_unit = FrequencyUnit::KHz;
                            draft.frequency_value = (hz / 1_000.0).round().max(1.0);
                        } else {
                            draft.frequency_unit = FrequencyUnit::Hz;
                            draft.frequency_value = hz.round().max(1.0);
                        }
                    };

                    ui.horizontal(|ui| {
                        ui.label("Period");
                        let response = ui.add(
                            egui::DragValue::new(&mut draft.period_value)
                                .speed(1.0)
                                .fixed_decimals(0),
                        );
                        if response.changed() {
                            period_changed = true;
                        }
                        egui::ComboBox::from_id_source("period_unit")
                            .selected_text(match draft.period_unit {
                                PeriodUnit::Ns => "ns",
                                PeriodUnit::Us => "us",
                                PeriodUnit::Ms => "ms",
                                PeriodUnit::S => "s",
                            })
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_value(&mut draft.period_unit, PeriodUnit::Ns, "ns")
                                    .clicked()
                                {
                                    period_changed = true;
                                }
                                if ui
                                    .selectable_value(&mut draft.period_unit, PeriodUnit::Us, "us")
                                    .clicked()
                                {
                                    period_changed = true;
                                }
                                if ui
                                    .selectable_value(&mut draft.period_unit, PeriodUnit::Ms, "ms")
                                    .clicked()
                                {
                                    period_changed = true;
                                }
                                if ui
                                    .selectable_value(&mut draft.period_unit, PeriodUnit::S, "s")
                                    .clicked()
                                {
                                    period_changed = true;
                                }
                            });
                    });

                    ui.horizontal(|ui| {
                        ui.label("Frequency");
                        let response = ui.add(
                            egui::DragValue::new(&mut draft.frequency_value)
                                .speed(1.0)
                                .fixed_decimals(0),
                        );
                        if response.changed() {
                            frequency_changed = true;
                        }
                        egui::ComboBox::from_id_source("freq_unit")
                            .selected_text(match draft.frequency_unit {
                                FrequencyUnit::Hz => "Hz",
                                FrequencyUnit::KHz => "kHz",
                                FrequencyUnit::MHz => "MHz",
                            })
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_value(
                                        &mut draft.frequency_unit,
                                        FrequencyUnit::Hz,
                                        "Hz",
                                    )
                                    .clicked()
                                {
                                    frequency_changed = true;
                                }
                                if ui
                                    .selectable_value(
                                        &mut draft.frequency_unit,
                                        FrequencyUnit::KHz,
                                        "kHz",
                                    )
                                    .clicked()
                                {
                                    frequency_changed = true;
                                }
                                if ui
                                    .selectable_value(
                                        &mut draft.frequency_unit,
                                        FrequencyUnit::MHz,
                                        "MHz",
                                    )
                                    .clicked()
                                {
                                    frequency_changed = true;
                                }
                            });
                    });

                    if draft.period_value < RUNTIME_MIN_PERIOD_VALUE {
                        draft.period_value = RUNTIME_MIN_PERIOD_VALUE;
                        period_changed = true;
                    }
                    if draft.frequency_value < RUNTIME_MIN_FREQUENCY_VALUE {
                        draft.frequency_value = RUNTIME_MIN_FREQUENCY_VALUE;
                        frequency_changed = true;
                    }

                    if period_changed {
                        draft.tab = WorkspaceTimingTab::Period;
                        let period_s = period_seconds_from(draft.period_value, draft.period_unit);
                        if period_s > 0.0 {
                            set_frequency_from_hz(&mut draft, 1.0 / period_s);
                        }
                    } else if frequency_changed {
                        draft.tab = WorkspaceTimingTab::Frequency;
                        let hz = frequency_hz_from(draft.frequency_value, draft.frequency_unit);
                        if hz > 0.0 {
                            set_period_from_seconds(&mut draft, 1.0 / hz);
                        }
                    }
                });

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label("Max Integration Steps");
                    ui.add(
                        egui::DragValue::new(&mut draft.max_integration_steps)
                            .speed(1.0)
                            .clamp_range(
                                RUNTIME_MAX_INTEGRATION_STEPS_MIN
                                    ..=RUNTIME_MAX_INTEGRATION_STEPS_MAX,
                            )
                            .fixed_decimals(0),
                    );
                    ui.label("(per plugin per tick)");
                });
                ui.label(
                    "Lower values improve real-time performance but may reduce numerical accuracy.",
                );

                ui.separator();
                ui.horizontal(|ui| {
                    if styled_button(ui, "Apply").clicked() {
                        apply_clicked = true;
                    }
                    if styled_button(ui, "Save").clicked() {
                        save_clicked = true;
                    }
                    if styled_button(ui, "Restore default").clicked() {
                        factory_reset_clicked = true;
                    }
                });
            });
        if let Some(response) = response {
            self.window_rects.push(response.response.rect);
            if !self.confirm_dialog.open
                && (response.response.clicked() || response.response.dragged())
            {
                ctx.move_to_top(response.response.layer_id);
            }
            if self.pending_window_focus == Some(WindowFocus::WorkspaceSettings) {
                ctx.move_to_top(response.response.layer_id);
                self.pending_window_focus = None;
            }
        }

        if apply_clicked || save_clicked {
            self.frequency_value = draft.frequency_value;
            self.frequency_unit = draft.frequency_unit;
            self.period_value = draft.period_value;
            self.period_unit = draft.period_unit;
            self.workspace_settings.tab = draft.tab;
            self.workspace_manager.workspace.settings = self.current_workspace_settings();
            self.mark_workspace_dirty();

            // Update the logic settings with the new max integration steps
            let period_seconds = self.compute_period_seconds();
            let (_, time_scale, time_label) = GuiApp::time_settings_from_selection(
                draft.tab,
                draft.frequency_unit,
                draft.period_unit,
            );
            let selected_cores: Vec<usize> = self
                .selected_cores
                .iter()
                .enumerate()
                .filter_map(|(idx, enabled)| if *enabled { Some(idx) } else { None })
                .collect();
            let cores = if selected_cores.is_empty() {
                vec![0]
            } else {
                selected_cores
            };

            let _ = self
                .state_sync
                .logic_tx
                .send(LogicMessage::UpdateSettings(LogicSettings {
                    cores,
                    period_seconds,
                    time_scale,
                    time_label,
                    ui_hz: self.state_sync.logic_ui_hz,
                    max_integration_steps: draft.max_integration_steps,
                }));

            if apply_clicked && !save_clicked {
                self.show_info("Runtime settings", "Sampling rate applied");
            }
        }
        if save_clicked {
            match self
                .workspace_manager
                .persist_runtime_settings_current_context()
            {
                Ok(rtsyn_core::workspace::RuntimeSettingsSaveTarget::Defaults) => {
                    self.show_info("Runtime settings", "Default values saved");
                }
                Ok(rtsyn_core::workspace::RuntimeSettingsSaveTarget::Workspace) => {
                    self.show_info("Runtime settings", "Workspace values saved");
                }
                Err(err) => {
                    self.show_info("Runtime settings", &format!("Failed to save values: {err}"));
                }
            }
        }
        if factory_reset_clicked {
            match self
                .workspace_manager
                .restore_runtime_settings_current_context()
            {
                Ok(_) => {
                    self.apply_workspace_settings();
                    draft = WorkspaceSettingsDraft {
                        frequency_value: self.frequency_value,
                        frequency_unit: self.frequency_unit,
                        period_value: self.period_value,
                        period_unit: self.period_unit,
                        tab: self.workspace_settings.tab,
                        max_integration_steps: draft.max_integration_steps,
                    };
                    self.show_info("Runtime settings", "Default values restored");
                }
                Err(err) => {
                    self.show_info("Runtime settings", &format!("Restore failed: {err}"));
                }
            }
        }

        self.workspace_settings.open = open;
        if open {
            self.workspace_settings.draft = Some(draft);
        } else {
            self.workspace_settings.draft = None;
        }
    }
}
