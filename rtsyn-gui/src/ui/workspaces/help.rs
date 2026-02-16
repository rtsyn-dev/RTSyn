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

impl GuiApp {
/// Renders the help documentation window with topic-based information.
    ///
    /// This function displays a tabbed help interface providing documentation
    /// about different aspects of RTSyn including plugins, workspaces, runtime,
    /// the RTSyn system itself, and CLI usage. Users can switch between topics
    /// to access relevant information.
    ///
    /// # Parameters
    /// - `ctx`: egui context for rendering UI elements and handling interactions
    ///
    /// # Side Effects
    /// - Renders fixed-size window (620x360) with tabbed interface
    /// - Updates help topic selection based on user clicks
    /// - Maintains help window open/closed state
    /// - Updates window focus and layering for proper modal behavior
    ///
    /// # Implementation Details
    /// - Topic tabs: Plugins, Workspaces, Runtime, RTSyn, CLI
    /// - Content areas: Topic-specific documentation with formatted text
    /// - Styling: Uses rich text formatting for headings and code examples
    /// - Navigation: Simple tab-based interface for topic switching
    /// - Code formatting: Monospace styling for CLI commands and examples
    /// - Window management: Handles focus and layer ordering appropriately
    pub(crate) fn render_help_window(&mut self, ctx: &egui::Context) {
        if !self.help_state.open {
            return;
        }

        let mut open = self.help_state.open;
        let window_size = egui::vec2(620.0, 360.0);
        let default_pos = Self::center_window(ctx, window_size);
        let mut topic = self.help_state.topic;
        let response = egui::Window::new("RTSyn Help")
            .open(&mut open)
            .resizable(false)
            .default_pos(default_pos)
            .default_size(window_size)
            .fixed_size(window_size)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(&mut topic, HelpTopic::Plugins, "Plugins");
                    ui.selectable_value(&mut topic, HelpTopic::Workspaces, "Workspaces");
                    ui.selectable_value(&mut topic, HelpTopic::Runtime, "Runtime");
                    ui.selectable_value(&mut topic, HelpTopic::RTSyn, "RTSyn");
                    ui.selectable_value(&mut topic, HelpTopic::CLI, "CLI");
                });

                ui.separator();
                match topic {
                    HelpTopic::Plugins => {
                        ui.heading(RichText::new("Plugins").color(egui::Color32::WHITE));
                        ui.add_space(6.0);
                        ui.label("Plugins are the building blocks of a workspace.");
                        ui.label("Each plugin exposes inputs/outputs and internal variables.");
                        ui.label("You can add, start/stop, configure, and connect plugins.");
                    }
                    HelpTopic::Workspaces => {
                        ui.heading(RichText::new("Workspaces").color(egui::Color32::WHITE));
                        ui.add_space(6.0);
                        ui.label("A workspace stores your plugin graph and its runtime settings.");
                        ui.label("Load/save lets you switch between different experiment setups.");
                        ui.label(
                            "Workspace values are separate from global default runtime values.",
                        );
                    }
                    HelpTopic::Runtime => {
                        ui.heading(RichText::new("Runtime").color(egui::Color32::WHITE));
                        ui.add_space(6.0);
                        ui.label("Runtime executes the loaded workspace in real time.");
                        ui.label("Runtime settings control timing (frequency/period) and cores.");
                        ui.label("Apply updates execution immediately, Save persists values.");
                    }
                    HelpTopic::RTSyn => {
                        ui.heading(RichText::new("RTSyn").color(egui::Color32::WHITE));
                        ui.add_space(6.0);
                        ui.label("RTSyn is a real-time simulation platform for plugin networks.");
                        ui.label("It currently runs in two separate modes/instances.");
                        ui.label("GUI instance: interactive editing, runtime control, and visualization.");
                        ui.label("Daemon + CLI instance: command-line control and automation.");
                        ui.label("The GUI is not daemonized at this stage.");
                    }
                    HelpTopic::CLI => {
                        ui.heading(RichText::new("CLI").color(egui::Color32::WHITE));
                        ui.add_space(6.0);
                        let code_style = |text: &str| {
                            RichText::new(format!(" {text} "))
                                .monospace()
                                .color(egui::Color32::from_rgb(205, 215, 230))
                                .background_color(egui::Color32::from_gray(40))
                        };
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                "RTSyn supports CLI interaction; for that you need to start the daemon with",
                            );
                            ui.label(code_style("rtsyn daemon run"));
                            ui.label(".");
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Use");
                            ui.label(code_style("--detach"));
                            ui.label("to run it in the background and keep your terminal free.");
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.label("To stop it run");
                            ui.label(code_style("rtsyn daemon stop"));
                            ui.label("; see");
                            ui.label(code_style("rtsyn daemon help"));
                            ui.label("for more details.");
                        });
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
            if self.pending_window_focus == Some(WindowFocus::Help) {
                ctx.move_to_top(response.response.layer_id);
                self.pending_window_focus = None;
            }
        }

        self.help_state.topic = topic;
        self.help_state.open = open;
    }
}
