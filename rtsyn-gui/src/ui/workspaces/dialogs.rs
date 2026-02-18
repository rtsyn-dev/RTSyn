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
    /// Opens a file dialog for loading workspace files.
    ///
    /// This function initiates an asynchronous file dialog for selecting workspace
    /// files to load. It prevents multiple dialogs from being opened simultaneously
    /// and uses platform-appropriate dialog implementations.
    ///
    /// # Side Effects
    /// - Shows info message if dialog is already open
    /// - Sets up channel receiver for dialog result
    /// - Spawns background thread for file dialog operation
    ///
    /// # Implementation Details
    /// - Uses zenity on systems with RT capabilities for better integration
    /// - Falls back to rfd (Rust File Dialog) on other platforms
    /// - Filters for JSON files (*.json) as workspace format
    /// - Prevents concurrent dialog instances through state checking
    pub(super) fn open_load_dialog(&mut self) {
        if self.file_dialogs.load_dialog_rx.is_some() {
            self.show_info("Workspace", "Load dialog already open.");
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.file_dialogs.load_dialog_rx = Some(rx);
        crate::spawn_file_dialog_thread(move || {
            let file = if crate::has_rt_capabilities() {
                crate::zenity_file_dialog("open", Some("*.json"))
            } else {
                rfd::FileDialog::new().pick_file()
            };
            let _ = tx.send(file);
        });
    }

    /// Opens a file dialog for importing workspace files.
    ///
    /// This function initiates an asynchronous file dialog for selecting workspace
    /// files to import. Similar to load dialog but used for different workflow contexts.
    ///
    /// # Side Effects
    /// - Shows info message if dialog is already open
    /// - Sets up channel receiver for dialog result
    /// - Spawns background thread for file dialog operation
    ///
    /// # Implementation Details
    /// - Uses zenity on systems with RT capabilities for better integration
    /// - Falls back to rfd (Rust File Dialog) on other platforms
    /// - Filters for JSON files (*.json) as workspace format
    /// - Prevents concurrent dialog instances through state checking
    pub(super) fn open_import_dialog(&mut self) {
        if self.file_dialogs.import_dialog_rx.is_some() {
            self.show_info("Workspace", "Import dialog already open.");
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.file_dialogs.import_dialog_rx = Some(rx);
        crate::spawn_file_dialog_thread(move || {
            let file = if crate::has_rt_capabilities() {
                crate::zenity_file_dialog("open", Some("*.json"))
            } else {
                rfd::FileDialog::new().pick_file()
            };
            let _ = tx.send(file);
        });
    }

    /// Opens the workspace dialog in the specified mode.
    ///
    /// This function initializes and displays the workspace dialog for creating,
    /// saving, or editing workspace metadata. It prepares the dialog state based
    /// on the requested mode and sets up window focus handling.
    ///
    /// # Parameters
    /// - `mode`: The dialog mode (New, Save, or Edit) determining dialog behavior
    ///
    /// # Side Effects
    /// - Sets dialog mode and opens the dialog window
    /// - Initializes input fields based on mode:
    ///   - New: Clears name and description inputs
    ///   - Save: Populates with current workspace data
    ///   - Edit: Preserves existing dialog state
    /// - Sets pending window focus for proper UI layering
    ///
    /// # Implementation Details
    /// - New mode: Starts with empty fields for creating new workspace
    /// - Save mode: Pre-fills with current workspace name and description
    /// - Edit mode: Maintains existing dialog state for editing operations
    pub(crate) fn open_workspace_dialog(&mut self, mode: WorkspaceDialogMode) {
        self.workspace_dialog.mode = mode;
        match mode {
            WorkspaceDialogMode::New => {
                self.workspace_dialog.name_input.clear();
                self.workspace_dialog.description_input.clear();
                self.workspace_dialog.edit_path = None;
            }
            WorkspaceDialogMode::Save => {
                self.workspace_dialog.name_input = self.workspace_manager.workspace.name.clone();
                self.workspace_dialog.description_input =
                    self.workspace_manager.workspace.description.clone();
                self.workspace_dialog.edit_path = None;
            }
            WorkspaceDialogMode::Edit => {}
        }
        self.workspace_dialog.open = true;
        self.pending_window_focus = Some(WindowFocus::WorkspaceDialog);
    }

    /// Renders the workspace dialog window for creating, saving, or editing workspaces.
    ///
    /// This function displays a modal dialog that allows users to input workspace
    /// name and description. The dialog behavior changes based on the current mode
    /// (New, Save, or Edit) and handles user interactions for workspace operations.
    ///
    /// # Parameters
    /// - `ctx`: egui context for rendering UI elements and handling interactions
    ///
    /// # Side Effects
    /// - Renders modal dialog window with input fields
    /// - Updates workspace dialog state based on user input
    /// - Handles dialog actions (Cancel, Save) and closes dialog appropriately
    /// - Updates window focus and layering for proper modal behavior
    /// - May trigger workspace creation, saving, or metadata updates
    ///
    /// # Implementation Details
    /// - Creates fixed-size dialog window (420x260) centered on screen
    /// - Provides text input fields for workspace name and description
    /// - Applies custom styling for consistent visual appearance
    /// - Handles window focus management and layer ordering
    /// - Processes user actions through action variable and match statements
    /// - Integrates with workspace manager for actual operations
    pub(crate) fn render_workspace_dialog(&mut self, ctx: &egui::Context) {
        if !self.workspace_dialog.open {
            return;
        }

        let path_preview = self.workspace_file_path(self.workspace_dialog.name_input.trim());
        let _path_display = path_preview.display().to_string();
        let mut open = self.workspace_dialog.open;
        let window_size = egui::vec2(420.0, 260.0);
        let default_pos = Self::center_window(ctx, window_size);
        let mut action = None;
        let response = egui::Window::new("Workspace")
            .open(&mut open)
            .resizable(false)
            .default_pos(default_pos)
            .default_size(window_size)
            .show(ctx, |ui| {
                ui.scope(|ui| {
                    let mut style = ui.style().as_ref().clone();
                    style.visuals.extreme_bg_color = egui::Color32::from_gray(40);
                    style.visuals.widgets.inactive.bg_fill = egui::Color32::from_gray(40);
                    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_gray(45);
                    style.visuals.widgets.active.bg_fill = egui::Color32::from_gray(50);
                    ui.set_style(style);
                    let width = ui.available_width();
                    ui.add_sized(
                        [width, 0.0],
                        egui::TextEdit::singleline(&mut self.workspace_dialog.name_input)
                            .font(egui::FontId::proportional(16.0))
                            .hint_text("Workspace name"),
                    );
                    ui.add_space(6.0);
                    ui.add_sized(
                        [width, 64.0],
                        egui::TextEdit::multiline(&mut self.workspace_dialog.description_input)
                            .hint_text("Description"),
                    );
                });
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    if styled_button(ui, "Cancel").clicked() {
                        action = Some("cancel");
                    }
                    if styled_button(ui, "Save").clicked() {
                        action = Some("save");
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
            if self.pending_window_focus == Some(WindowFocus::WorkspaceDialog) {
                ctx.move_to_top(response.response.layer_id);
                self.pending_window_focus = None;
            }
        }

        self.workspace_dialog.open = open;

        if let Some(action) = action {
            match action {
                "cancel" => self.workspace_dialog.open = false,
                "save" => {
                    let saved = match self.workspace_dialog.mode {
                        WorkspaceDialogMode::New => self.create_workspace_from_dialog(),
                        WorkspaceDialogMode::Save => self.save_workspace_as(),
                        WorkspaceDialogMode::Edit => {
                            if let Some(path) = self.workspace_dialog.edit_path.clone() {
                                self.update_workspace_metadata(&path)
                            } else {
                                false
                            }
                        }
                    };
                    if saved {
                        self.workspace_dialog.open = false;
                    }
                }
                _ => {}
            }
        }
    }

    /// Renders the confirmation dialog for destructive operations.
    ///
    /// This function displays a modal confirmation dialog that blocks user
    /// interaction with the main interface until the user confirms or cancels
    /// a potentially destructive operation (like deleting a workspace).
    ///
    /// # Parameters
    /// - `ctx`: egui context for rendering UI elements and handling interactions
    ///
    /// # Side Effects
    /// - Renders modal overlay blocking main interface interaction
    /// - Displays confirmation dialog with title, message, and action buttons
    /// - Executes confirmed actions through the action system
    /// - Closes dialog and clears action state after user response
    ///
    /// # Implementation Details
    /// - Modal overlay: Semi-transparent background blocking main UI
    /// - Centered dialog: Positioned at screen center with window frame styling
    /// - Action buttons: Cancel and custom action label (e.g., "Delete")
    /// - Action execution: Delegates to perform_confirm_action() for processing
    /// - State management: Clears dialog state after action completion
    pub(crate) fn render_confirm_remove_dialog(&mut self, ctx: &egui::Context) {
        if !self.confirm_dialog.open {
            return;
        }

        let screen_rect = ctx.screen_rect();
        egui::Area::new(egui::Id::new("modal_blocker"))
            .order(egui::Order::Middle)
            .fixed_pos(screen_rect.min)
            .show(ctx, |ui| {
                ui.allocate_rect(screen_rect, egui::Sense::click());
                ui.painter()
                    .rect_filled(screen_rect, 0.0, egui::Color32::from_black_alpha(220));
            });

        let center = screen_rect.center();
        egui::Area::new(egui::Id::new("modal_dialog"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::CENTER_CENTER)
            .fixed_pos(center)
            .show(ctx, |ui| {
                egui::Frame::window(ui.style())
                    .rounding(egui::Rounding::same(6.0))
                    .show(ui, |ui| {
                        ui.heading(&self.confirm_dialog.title);
                        ui.label(&self.confirm_dialog.message);
                        ui.horizontal(|ui| {
                            if styled_button(ui, "Cancel").clicked() {
                                self.confirm_dialog.open = false;
                                self.confirm_dialog.action = None;
                            }
                            if styled_button(ui, &self.confirm_dialog.action_label).clicked() {
                                if let Some(action) = self.confirm_dialog.action.clone() {
                                    self.perform_confirm_action(action);
                                }
                                self.confirm_dialog.open = false;
                                self.confirm_dialog.action = None;
                            }
                        });
                    });
            });
    }

    /// Renders toast-style information notifications.
    ///
    /// This function displays temporary notification messages as toast popups
    /// that slide in from the right side of the screen. Notifications automatically
    /// fade out after a specified duration and support multiple concurrent messages.
    ///
    /// # Parameters
    /// - `ctx`: egui context for rendering UI elements and handling interactions
    ///
    /// # Side Effects
    /// - Renders animated toast notifications with slide and fade effects
    /// - Requests continuous repaints for smooth animations
    /// - Cleans up expired notifications from the notification handler
    ///
    /// # Implementation Details
    /// - Animation: Smooth slide-in/slide-out with easing functions
    /// - Positioning: Right-aligned with vertical stacking for multiple toasts
    /// - Timing: 2.8 second total duration with configurable fade periods
    /// - Styling: Semi-transparent background with customizable opacity
    /// - Cleanup: Automatic removal of expired notifications
    /// - Performance: 60fps animation updates with 16ms repaint intervals
    pub(crate) fn render_info_dialog(&mut self, ctx: &egui::Context) {
        let notifications = self.notification_handler.get_all_notifications();
        if notifications.is_empty() {
            return;
        }

        let now = Instant::now();
        let screen_rect = ctx.screen_rect();
        let max_width = 380.0;
        let mut y = screen_rect.min.y + 32.0;
        let x = screen_rect.max.x - 4.0;
        let total = 2.8;
        let mut idx = 0usize;
        for notification in notifications {
            let age = now.duration_since(notification.created_at).as_secs_f32();
            if age >= total {
                idx += 1;
                continue;
            }
            let alpha = 1.0;
            let slide_in = 0.35;
            let slide_out = 0.45;
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
            let fill_alpha = (200.0 * alpha) as u8;
            let text_alpha = (230.0 * alpha) as u8;
            let fill = egui::Color32::from_rgba_premultiplied(20, 20, 20, fill_alpha);
            let stroke = egui::Color32::from_rgba_premultiplied(80, 80, 80, fill_alpha);
            let text = egui::Color32::from_rgba_premultiplied(235, 235, 235, text_alpha);

            egui::Area::new(egui::Id::new(("info_toast", idx)))
                .order(egui::Order::Foreground)
                .interactable(false)
                .pivot(egui::Align2::RIGHT_TOP)
                .fixed_pos(egui::pos2(x_pos, y))
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style())
                        .fill(fill)
                        .stroke(egui::Stroke::new(1.0, stroke))
                        .rounding(egui::Rounding::same(6.0))
                        .show(ui, |ui| {
                            ui.set_max_width(max_width);
                            ui.add_space(2.0);
                            ui.label(
                                RichText::new(&notification.title)
                                    .color(text)
                                    .strong()
                                    .size(16.0),
                            );
                            ui.label(RichText::new(&notification.message).color(text).size(14.0));
                            ui.add_space(2.0);
                        });
                });
            y += 66.0;
            idx += 1;
        }
        self.notification_handler.cleanup_old_notifications(total);
        ctx.request_repaint_after(Duration::from_millis(16));
    }

    /// Renders the build progress dialog for plugin compilation operations.
    ///
    /// This function displays a modal dialog showing the progress of plugin
    /// build operations. It can show either a progress indicator during active
    /// builds or completion messages with an OK button.
    ///
    /// # Parameters
    /// - `ctx`: egui context for rendering UI elements and handling interactions
    ///
    /// # Side Effects
    /// - Renders modal overlay blocking main interface during builds
    /// - Displays build progress with spinner animation during active builds
    /// - Shows completion message with dismissal button when build finishes
    /// - Closes dialog when user acknowledges completion
    ///
    /// # Implementation Details
    /// - Modal overlay: Semi-transparent background blocking main UI
    /// - Progress state: Shows spinner and message during active builds
    /// - Completion state: Shows final message with OK button for dismissal
    /// - Centered dialog: Positioned at screen center with window frame styling
    /// - State management: Handles both in-progress and completed build states
    pub(crate) fn render_build_dialog(&mut self, ctx: &egui::Context) {
        if !self.build_dialog.open {
            return;
        }

        let screen_rect = ctx.screen_rect();
        egui::Area::new(egui::Id::new("build_blocker"))
            .order(egui::Order::Middle)
            .fixed_pos(screen_rect.min)
            .show(ctx, |ui| {
                ui.allocate_rect(screen_rect, egui::Sense::click());
                ui.painter()
                    .rect_filled(screen_rect, 0.0, egui::Color32::from_black_alpha(140));
            });

        let center = screen_rect.center();
        egui::Area::new(egui::Id::new("build_dialog"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::CENTER_CENTER)
            .fixed_pos(center)
            .show(ctx, |ui| {
                egui::Frame::window(ui.style())
                    .rounding(egui::Rounding::same(6.0))
                    .show(ui, |ui| {
                        ui.heading(
                            egui::RichText::new(&self.build_dialog.title).color(egui::Color32::WHITE),
                        );
                        if self.build_dialog.in_progress {
                            ui.horizontal(|ui| {
                                ui.label(&self.build_dialog.message);
                                ui.add(egui::Spinner::new());
                            });
                            return;
                        }
                        ui.label(&self.build_dialog.message);
                        if styled_button(ui, "OK").clicked() {
                            self.build_dialog.open = false;
                        }
                    });
            });
    }
}
