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
/// Opens the manage workspaces window.
    ///
    /// This function opens the workspace management interface that allows users
    /// to view, load, edit, export, and delete existing workspaces. It initializes
    /// the window state and triggers a workspace scan.
    ///
    /// # Side Effects
    /// - Opens the manage workspaces window
    /// - Clears any previously selected workspace
    /// - Initiates workspace directory scanning
    /// - Sets pending window focus for proper UI layering
    ///
    /// # Implementation Details
    /// - Resets selection state to ensure clean interface
    /// - Calls scan_workspaces() to refresh available workspace list
    /// - Sets up window focus handling for modal behavior
    pub(crate) fn open_manage_workspaces(&mut self) {
        self.windows.manage_workspace_open = true;
        self.windows.manage_workspace_selected_index = None;
        self.scan_workspaces();
        self.pending_window_focus = Some(WindowFocus::ManageWorkspaces);
    }

    /// Opens the load workspaces window.
    ///
    /// This function opens the workspace loading interface that allows users
    /// to browse and select workspaces for loading. It provides a simplified
    /// interface focused on workspace selection and loading.
    ///
    /// # Side Effects
    /// - Opens the load workspaces window
    /// - Clears any previously selected workspace
    /// - Initiates workspace directory scanning
    /// - Sets pending window focus for proper UI layering
    ///
    /// # Implementation Details
    /// - Resets selection state to ensure clean interface
    /// - Calls scan_workspaces() to refresh available workspace list
    /// - Sets up window focus handling for modal behavior
    pub(crate) fn open_load_workspaces(&mut self) {
        self.windows.load_workspace_open = true;
        self.windows.load_workspace_selected_index = None;
        self.scan_workspaces();
        self.pending_window_focus = Some(WindowFocus::LoadWorkspaces);
    }

    /// Renders the manage workspaces window for comprehensive workspace management.
    ///
    /// This function displays a two-panel interface for managing existing workspaces.
    /// The left panel shows a searchable list of workspaces, while the right panel
    /// displays detailed information about the selected workspace and provides
    /// management actions (Load, Edit, Export, Delete).
    ///
    /// # Parameters
    /// - `ctx`: egui context for rendering UI elements and handling interactions
    ///
    /// # Side Effects
    /// - Renders fixed-size window (520x520) with two-panel layout
    /// - Updates workspace selection state based on user clicks
    /// - Handles workspace management actions (load, edit, export, delete)
    /// - Opens file dialogs for import operations
    /// - May trigger confirmation dialogs for destructive operations
    /// - Updates window focus and layering for proper modal behavior
    ///
    /// # Implementation Details
    /// - Left panel: Search field and scrollable workspace list with filtering
    /// - Right panel: Detailed workspace information and action buttons
    /// - Search functionality filters workspaces by name (case-insensitive)
    /// - Displays workspace metadata including plugin count and types
    /// - Provides browse functionality for importing external workspace files
    /// - Handles window focus management and prevents interaction conflicts
    /// - Integrates with confirmation system for delete operations
    pub(crate) fn render_manage_workspaces_window(&mut self, ctx: &egui::Context) {
        if !self.windows.manage_workspace_open {
            return;
        }

        let mut open = self.windows.manage_workspace_open;
        let window_size = egui::vec2(520.0, 520.0);
        let default_pos = Self::center_window(ctx, window_size);
        let mut action_load: Option<PathBuf> = None;
        let mut action_export: Option<PathBuf> = None;
        let mut action_delete: Option<PathBuf> = None;
        let mut action_edit: Option<PathBuf> = None;
        let response = egui::Window::new("Manage workspaces")
            .open(&mut open)
            .resizable(false)
            .default_pos(default_pos)
            .default_size(window_size)
            .fixed_size(window_size)
            .show(ctx, |ui| {
                let total_w = ui.available_width();
                let left_w = (total_w * 0.52).max(240.0);
                let right_w = (total_w - left_w - 10.0).max(220.0);
                let full_h = ui.available_height();
                let footer_h = 44.0;
                let search_h = 34.0;
                let list_h = (full_h - search_h - footer_h - 16.0).max(120.0);
                let mut selected: Option<usize> = None;

                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(left_w, full_h),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.scope(|ui| {
                                let mut style = ui.style().as_ref().clone();
                                style.visuals.extreme_bg_color = egui::Color32::from_gray(50);
                                style.visuals.widgets.inactive.bg_fill =
                                    egui::Color32::from_gray(50);
                                style.visuals.widgets.hovered.bg_fill =
                                    egui::Color32::from_gray(55);
                                style.visuals.widgets.active.bg_fill = egui::Color32::from_gray(60);
                                ui.set_style(style);
                                ui.add_sized(
                                    [220.0, 24.0],
                                    egui::TextEdit::singleline(
                                        &mut self.windows.manage_workspace_search,
                                    )
                                    .hint_text("Search workspaces"),
                                );
                            });
                            ui.add_space(6.0);
                            ui.separator();

                            ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), list_h),
                                egui::Layout::top_down(egui::Align::LEFT),
                                |ui| {
                                    egui::ScrollArea::vertical()
                                        .auto_shrink([false, false])
                                        .max_height(list_h)
                                        .min_scrolled_height(list_h)
                                        .show(ui, |ui| {
                                            ui.style_mut().spacing.item_spacing.y = 4.0;
                                            for (idx, entry) in self
                                                .workspace_manager
                                                .workspace_entries
                                                .iter()
                                                .enumerate()
                                            {
                                                if !self
                                                    .windows
                                                    .manage_workspace_search
                                                    .trim()
                                                    .is_empty()
                                                    && !entry.name.to_lowercase().contains(
                                                        &self
                                                            .windows
                                                            .manage_workspace_search
                                                            .to_lowercase(),
                                                    )
                                                {
                                                    continue;
                                                }
                                                let label = entry.name.clone();
                                                let response = ui
                                                    .allocate_ui_with_layout(
                                                        egui::vec2(ui.available_width(), 22.0),
                                                        egui::Layout::left_to_right(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            ui.add(egui::SelectableLabel::new(
                                                                self.windows
                                                                    .manage_workspace_selected_index
                                                                    == Some(idx),
                                                                egui::RichText::new(label)
                                                                    .size(14.0),
                                                            ))
                                                        },
                                                    )
                                                    .inner;
                                                if response.clicked() {
                                                    selected = Some(idx);
                                                }
                                            }
                                        });
                                },
                            );

                            ui.separator();
                            ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), footer_h),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.label("Browse workspace file");
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if styled_button(ui, "Browse...").clicked() {
                                                self.open_import_dialog();
                                            }
                                        },
                                    );
                                },
                            );
                        },
                    );

                    ui.add(egui::Separator::default().vertical());

                    ui.allocate_ui_with_layout(
                        egui::vec2(right_w, full_h),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .max_height(full_h)
                                .min_scrolled_height(full_h)
                                .show(ui, |ui| {
                                    if let Some(idx) = self.windows.manage_workspace_selected_index
                                    {
                                        if let Some(entry) =
                                            self.workspace_manager.workspace_entries.get(idx)
                                        {
                                            ui.add_space(4.0);
                                            ui.horizontal(|ui| {
                                                ui.add_sized(
                                                    [ui.available_width(), 0.0],
                                                    egui::Label::new(
                                                        RichText::new(&entry.name)
                                                            .strong()
                                                            .size(18.0),
                                                    )
                                                    .wrap(true),
                                                );
                                            });
                                            ui.add_space(4.0);
                                            if !entry.description.is_empty() {
                                                ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new(&entry.description)
                                                            .size(13.0)
                                                            .color(egui::Color32::from_gray(200)),
                                                    )
                                                    .wrap(true),
                                                );
                                                ui.add_space(8.0);
                                            }
                                            ui.label(egui::RichText::new("Workspace").strong());
                                            egui::Grid::new(("manage_workspace_preview", idx))
                                                .num_columns(2)
                                                .spacing([8.0, 4.0])
                                                .show(ui, |ui| {
                                                    ui.label("Plugins:");
                                                    ui.label(entry.plugins.to_string());
                                                    ui.end_row();
                                                    ui.label("Path:");
                                                    ui.add(
                                                        egui::Label::new(
                                                            entry.path.to_string_lossy(),
                                                        )
                                                        .wrap(true),
                                                    );
                                                    ui.end_row();
                                                });
                                            if !entry.plugin_kinds.is_empty() {
                                                ui.add_space(4.0);
                                                ui.label(egui::RichText::new("Types:").strong());
                                                ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new(
                                                            entry.plugin_kinds.join(", "),
                                                        )
                                                        .size(12.0)
                                                        .color(egui::Color32::from_gray(180)),
                                                    )
                                                    .wrap(true),
                                                );
                                            }
                                            ui.add_space(6.0);
                                            ui.allocate_ui_with_layout(
                                                egui::vec2(ui.available_width(), BUTTON_SIZE.y),
                                                egui::Layout::left_to_right(egui::Align::Center),
                                                |ui| {
                                                    if styled_button(ui, "Load").clicked() {
                                                        action_load = Some(entry.path.clone());
                                                    }
                                                    if styled_button(ui, "Edit metadata").clicked()
                                                    {
                                                        action_edit = Some(entry.path.clone());
                                                    }
                                                },
                                            );
                                            ui.add_space(2.0);
                                            ui.allocate_ui_with_layout(
                                                egui::vec2(ui.available_width(), BUTTON_SIZE.y),
                                                egui::Layout::left_to_right(egui::Align::Center),
                                                |ui| {
                                                    if styled_button(ui, "Export").clicked() {
                                                        action_export = Some(entry.path.clone());
                                                    }
                                                    if styled_button(ui, "Delete").clicked() {
                                                        action_delete = Some(entry.path.clone());
                                                    }
                                                },
                                            );
                                        }
                                    } else {
                                        ui.add_space(4.0);
                                        ui.label(
                                            RichText::new("No workspace selected")
                                                .color(egui::Color32::from_gray(120)),
                                        );
                                    }
                                });
                        },
                    );
                });

                if let Some(idx) = selected {
                    self.windows.manage_workspace_selected_index = Some(idx);
                    if let Some(entry) = self.workspace_manager.workspace_entries.get(idx) {
                        self.workspace_dialog.name_input = entry.name.clone();
                        self.workspace_dialog.description_input = entry.description.clone();
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
            if self.pending_window_focus == Some(WindowFocus::ManageWorkspaces) {
                ctx.move_to_top(response.response.layer_id);
                self.pending_window_focus = None;
            }
        }

        self.windows.manage_workspace_open = open;

        if let Some(path) = action_load {
            self.workspace_manager.workspace_path = path;
            self.load_workspace();
            self.windows.manage_workspace_open = false;
        }
        if let Some(path) = action_edit {
            self.workspace_dialog.mode = WorkspaceDialogMode::Edit;
            self.workspace_dialog.edit_path = Some(path);
            self.workspace_dialog.open = true;
        }
        if let Some(path) = action_export {
            self.export_workspace_path(&path);
        }
        if let Some(path) = action_delete {
            self.show_confirm(
                "Delete workspace",
                "Delete this workspace?",
                "Delete",
                ConfirmAction::DeleteWorkspace(path),
            );
        }
    }

    /// Renders the load workspaces window for workspace selection and loading.
    ///
    /// This function displays a simplified two-panel interface focused on workspace
    /// loading. The left panel shows a searchable list of workspaces, while the
    /// right panel displays information about the selected workspace with a load action.
    ///
    /// # Parameters
    /// - `ctx`: egui context for rendering UI elements and handling interactions
    ///
    /// # Side Effects
    /// - Renders fixed-size window (520x520) with two-panel layout
    /// - Updates workspace selection state based on user clicks
    /// - Handles workspace loading action and closes window on successful load
    /// - Opens file dialogs for browsing external workspace files
    /// - Updates window focus and layering for proper modal behavior
    ///
    /// # Implementation Details
    /// - Left panel: Search field and scrollable workspace list with filtering
    /// - Right panel: Workspace information display and load button
    /// - Search functionality filters workspaces by name (case-insensitive)
    /// - Displays workspace metadata including plugin count and types
    /// - Provides browse functionality for loading external workspace files
    /// - Automatically closes window after successful workspace loading
    /// - Handles window focus management and prevents interaction conflicts
    pub(crate) fn render_load_workspaces_window(&mut self, ctx: &egui::Context) {
        if !self.windows.load_workspace_open {
            return;
        }

        let mut open = self.windows.load_workspace_open;
        let window_size = egui::vec2(520.0, 520.0);
        let default_pos = Self::center_window(ctx, window_size);
        let mut action_load: Option<PathBuf> = None;
        let response = egui::Window::new("Load workspaces")
            .open(&mut open)
            .resizable(false)
            .default_pos(default_pos)
            .default_size(window_size)
            .fixed_size(window_size)
            .show(ctx, |ui| {
                let total_w = ui.available_width();
                let left_w = (total_w * 0.52).max(240.0);
                let right_w = (total_w - left_w - 10.0).max(220.0);
                let full_h = ui.available_height();
                let footer_h = 44.0;
                let search_h = 34.0;
                let list_h = (full_h - search_h - footer_h - 16.0).max(120.0);
                let mut selected: Option<usize> = None;

                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(left_w, full_h),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.horizontal(|ui| {
                                ui.scope(|ui| {
                                    let mut style = ui.style().as_ref().clone();
                                    style.visuals.extreme_bg_color = egui::Color32::from_gray(50);
                                    style.visuals.widgets.inactive.bg_fill =
                                        egui::Color32::from_gray(50);
                                    style.visuals.widgets.hovered.bg_fill =
                                        egui::Color32::from_gray(55);
                                    style.visuals.widgets.active.bg_fill =
                                        egui::Color32::from_gray(60);
                                    ui.set_style(style);
                                    ui.add_sized(
                                        [220.0, 24.0],
                                        egui::TextEdit::singleline(
                                            &mut self.windows.load_workspace_search,
                                        )
                                        .hint_text("Search workspaces"),
                                    );
                                });
                            });
                            ui.add_space(6.0);
                            ui.separator();
                            ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), list_h),
                                egui::Layout::top_down(egui::Align::LEFT),
                                |ui| {
                                    egui::ScrollArea::vertical()
                                        .auto_shrink([false, false])
                                        .max_height(list_h)
                                        .min_scrolled_height(list_h)
                                        .show(ui, |ui| {
                                            ui.style_mut().spacing.item_spacing.y = 4.0;
                                            for (idx, entry) in self
                                                .workspace_manager
                                                .workspace_entries
                                                .iter()
                                                .enumerate()
                                            {
                                                if !self
                                                    .windows
                                                    .load_workspace_search
                                                    .trim()
                                                    .is_empty()
                                                    && !entry.name.to_lowercase().contains(
                                                        &self
                                                            .windows
                                                            .load_workspace_search
                                                            .to_lowercase(),
                                                    )
                                                {
                                                    continue;
                                                }
                                                let label = entry.name.clone();
                                                let response = ui
                                                    .allocate_ui_with_layout(
                                                        egui::vec2(ui.available_width(), 22.0),
                                                        egui::Layout::left_to_right(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            ui.add(egui::SelectableLabel::new(
                                                                self.windows
                                                                    .load_workspace_selected_index
                                                                    == Some(idx),
                                                                egui::RichText::new(label)
                                                                    .size(14.0),
                                                            ))
                                                        },
                                                    )
                                                    .inner;
                                                if response.clicked() {
                                                    selected = Some(idx);
                                                }
                                            }
                                        });
                                },
                            );
                            if let Some(idx) = selected {
                                self.windows.load_workspace_selected_index = Some(idx);
                                if let Some(entry) =
                                    self.workspace_manager.workspace_entries.get(idx)
                                {
                                    self.workspace_dialog.name_input = entry.name.clone();
                                    self.workspace_dialog.description_input =
                                        entry.description.clone();
                                }
                            }
                            ui.separator();
                            ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), footer_h),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.label("Browse workspace file");
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if styled_button(ui, "Browse...").clicked() {
                                                self.open_load_dialog();
                                            }
                                        },
                                    );
                                },
                            );
                        },
                    );
                    ui.add(egui::Separator::default().vertical());
                    ui.allocate_ui_with_layout(
                        egui::vec2(right_w, full_h),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .max_height(full_h)
                                .min_scrolled_height(full_h)
                                .show(ui, |ui| {
                                    if let Some(idx) = self.windows.load_workspace_selected_index {
                                        if let Some(entry) =
                                            self.workspace_manager.workspace_entries.get(idx)
                                        {
                                            ui.add_space(4.0);
                                            ui.horizontal(|ui| {
                                                ui.add_sized(
                                                    [ui.available_width(), 0.0],
                                                    egui::Label::new(
                                                        RichText::new(&entry.name)
                                                            .strong()
                                                            .size(18.0),
                                                    )
                                                    .wrap(true),
                                                );
                                            });
                                            ui.add_space(4.0);
                                            if !entry.description.is_empty() {
                                                ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new(&entry.description)
                                                            .size(13.0)
                                                            .color(egui::Color32::from_gray(200)),
                                                    )
                                                    .wrap(true),
                                                );
                                                ui.add_space(8.0);
                                            }
                                            ui.label(egui::RichText::new("Workspace").strong());
                                            egui::Grid::new(("load_workspace_preview", idx))
                                                .num_columns(2)
                                                .spacing([8.0, 4.0])
                                                .show(ui, |ui| {
                                                    ui.label("Plugins:");
                                                    ui.label(entry.plugins.to_string());
                                                    ui.end_row();
                                                    ui.label("Path:");
                                                    ui.add(
                                                        egui::Label::new(
                                                            entry.path.to_string_lossy(),
                                                        )
                                                        .wrap(true),
                                                    );
                                                    ui.end_row();
                                                });
                                            if !entry.plugin_kinds.is_empty() {
                                                ui.add_space(4.0);
                                                ui.label(egui::RichText::new("Types:").strong());
                                                ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new(
                                                            entry.plugin_kinds.join(", "),
                                                        )
                                                        .size(12.0)
                                                        .color(egui::Color32::from_gray(180)),
                                                    )
                                                    .wrap(true),
                                                );
                                            }
                                            ui.add_space(12.0);
                                            ui.horizontal(|ui| {
                                                if styled_button(ui, "Load").clicked() {
                                                    action_load = Some(entry.path.clone());
                                                }
                                            });
                                        }
                                    } else {
                                        ui.add_space(4.0);
                                        ui.label(
                                            RichText::new("No workspace selected")
                                                .color(egui::Color32::from_gray(120)),
                                        );
                                    }
                                });
                        },
                    );
                });
            });
        if let Some(response) = response {
            self.window_rects.push(response.response.rect);
            if !self.confirm_dialog.open
                && (response.response.clicked() || response.response.dragged())
            {
                ctx.move_to_top(response.response.layer_id);
            }
            if self.pending_window_focus == Some(WindowFocus::LoadWorkspaces) {
                ctx.move_to_top(response.response.layer_id);
                self.pending_window_focus = None;
            }
        }

        self.windows.load_workspace_open = open;

        if let Some(path) = action_load {
            self.workspace_manager.workspace_path = path;
            self.load_workspace();
            self.windows.load_workspace_open = false;
        }
    }
}
