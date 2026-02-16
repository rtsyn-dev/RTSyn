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
use image::ImageEncoder;
use rtsyn_core::workspace::{
    RUNTIME_MAX_INTEGRATION_STEPS_MAX, RUNTIME_MAX_INTEGRATION_STEPS_MIN,
    RUNTIME_MIN_FREQUENCY_VALUE, RUNTIME_MIN_PERIOD_VALUE,
};
use rtsyn_runtime::LogicSettings;
use std::hash::{Hash, Hasher};
use std::io::Read;

impl GuiApp {
    /// Requests UML diagram rendering from PlantUML web service.
    ///
    /// This function encodes the provided UML text using PlantUML deflate encoding
    /// and sends a request to the PlantUML web service to render the diagram.
    ///
    /// # Parameters
    /// - `uml`: The UML diagram text to be rendered
    /// - `as_svg`: If true, requests SVG format; otherwise requests PNG format
    ///
    /// # Returns
    /// - `Ok(Vec<u8>)`: The rendered diagram as bytes
    /// - `Err(String)`: Error message if encoding, network request, or reading fails
    ///
    /// # Side Effects
    /// - Makes HTTP request to PlantUML web service
    /// - May block for up to 10 seconds waiting for response
    fn request_uml_render(&mut self, uml: &str, as_svg: bool) -> Result<Vec<u8>, String> {
        let encoded = plantuml_encoding::encode_plantuml_deflate(uml)
            .map_err(|err| format!("Failed to encode UML: {err:?}"))?;
        let format_path = if as_svg { "svg" } else { "png" };
        let url = format!("https://www.plantuml.com/plantuml/{format_path}/{encoded}");
        let response = ureq::get(&url)
            .timeout(std::time::Duration::from_secs(10))
            .call()
            .map_err(|err| format!("Failed to render UML: {err}"))?;
        let mut bytes = Vec::new();
        response
            .into_reader()
            .read_to_end(&mut bytes)
            .map_err(|err| format!("Failed to read UML render: {err}"))?;
        Ok(bytes)
    }

    /// Resizes a PNG image to the specified dimensions.
    ///
    /// This function decodes a PNG image from bytes, resizes it using Lanczos3 filtering
    /// for high quality scaling, and re-encodes it as PNG with RGBA8 format.
    ///
    /// # Parameters
    /// - `bytes`: The original PNG image data as bytes
    /// - `width`: Target width in pixels
    /// - `height`: Target height in pixels
    ///
    /// # Returns
    /// - `Ok(Vec<u8>)`: The resized PNG image as bytes
    /// - `Err(String)`: Error message if decoding, resizing, or encoding fails
    ///
    /// # Implementation Details
    /// - Uses Lanczos3 filter for high-quality resampling
    /// - Converts to RGBA8 format for consistent output
    /// - Preserves alpha channel information
    fn resize_png(bytes: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
        let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
            .map_err(|err| format!("Failed to decode PNG: {err}"))?;
        let resized = image.resize_exact(width, height, image::imageops::FilterType::Lanczos3);
        let rgba = resized.to_rgba8();
        let mut output = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut output);
        encoder
            .write_image(
                &rgba,
                rgba.width(),
                rgba.height(),
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|err| format!("Failed to encode PNG: {err}"))?;
        Ok(output)
    }

    /// Starts asynchronous UML preview rendering in a background thread.
    ///
    /// This function initiates the rendering of a UML diagram preview by spawning
    /// a background thread that handles the network request to PlantUML. It uses
    /// content hashing to avoid redundant renders of the same UML content.
    ///
    /// # Parameters
    /// - `uml`: The UML diagram text to render
    ///
    /// # Side Effects
    /// - Sets `uml_preview_loading` to true
    /// - Updates `uml_preview_hash` with content hash
    /// - Clears any existing preview error and texture
    /// - Spawns background thread for network operation
    /// - Sets up channel receiver for async result handling
    ///
    /// # Implementation Details
    /// - Uses DefaultHasher to generate content hash for caching
    /// - Skips rendering if hash matches current preview and not loading
    /// - Spawns thread to avoid blocking UI during network request
    /// - Uses 10-second timeout for PlantUML service requests
    fn start_uml_preview_render(&mut self, uml: &str) {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        uml.hash(&mut hasher);
        let hash = hasher.finish();
        if self.uml_preview_hash == Some(hash) && !self.uml_preview_loading {
            return;
        }

        self.uml_preview_hash = Some(hash);
        self.uml_preview_error = None;
        self.uml_preview_loading = true;
        self.uml_preview_texture = None;
        let uml_owned = uml.to_string();
        let (tx, rx) = mpsc::channel();
        self.uml_preview_rx = Some(rx);
        std::thread::spawn(move || {
            let result = (|| -> Result<Vec<u8>, String> {
                let encoded = plantuml_encoding::encode_plantuml_deflate(&uml_owned)
                    .map_err(|err| format!("Failed to encode UML: {err:?}"))?;
                let url = format!("https://www.plantuml.com/plantuml/png/{encoded}");
                let response = ureq::get(&url)
                    .timeout(std::time::Duration::from_secs(10))
                    .call()
                    .map_err(|err| format!("Failed to render UML preview: {err}"))?;

                let mut bytes = Vec::new();
                response
                    .into_reader()
                    .read_to_end(&mut bytes)
                    .map_err(|err| format!("Failed to read UML preview: {err}"))?;
                Ok(bytes)
            })();

            let _ = tx.send((hash, result));
        });
    }

    /// Polls for completion of asynchronous UML preview rendering.
    ///
    /// This function checks if the background UML rendering thread has completed
    /// and processes the result by either creating a texture for display or
    /// setting an error message.
    ///
    /// # Parameters
    /// - `ctx`: egui context for texture creation and UI updates
    ///
    /// # Side Effects
    /// - Updates `uml_preview_loading` state
    /// - Creates `uml_preview_texture` on successful render
    /// - Sets `uml_preview_error` on render failure
    /// - Clears receiver channel when processing completes
    ///
    /// # Implementation Details
    /// - Uses try_recv() for non-blocking channel polling
    /// - Validates hash to ensure result matches current request
    /// - Converts PNG bytes to egui ColorImage and texture
    /// - Uses LINEAR texture filtering for smooth scaling
    /// - Handles both network and image decoding errors gracefully
    fn poll_uml_preview_render(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.uml_preview_rx else {
            return;
        };
        let Ok((hash, result)) = rx.try_recv() else {
            return;
        };
        self.uml_preview_loading = false;
        self.uml_preview_rx = None;
        if self.uml_preview_hash != Some(hash) {
            return;
        }

        let bytes = match result {
            Ok(bytes) => bytes,
            Err(_err) => {
                self.uml_preview_error = Some("Render failed, please regenerate UML".to_string());
                self.uml_preview_texture = None;
                return;
            }
        };

        let image = match image::load_from_memory_with_format(&bytes, image::ImageFormat::Png) {
            Ok(image) => image.to_rgba8(),
            Err(_err) => {
                self.uml_preview_error = Some("Render failed, please regenerate UML".to_string());
                self.uml_preview_texture = None;
                return;
            }
        };

        let size = [image.width() as usize, image.height() as usize];
        let rgba = image.into_raw();
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
        let texture = ctx.load_texture(
            format!("uml_preview_{hash}"),
            color_image,
            egui::TextureOptions::LINEAR,
        );
        self.uml_preview_texture = Some(texture);
        self.uml_preview_error = None;
    }

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
    fn open_load_dialog(&mut self) {
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
    fn open_import_dialog(&mut self) {
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

    /// Renders the UML diagram editor and preview window in a separate viewport.
    ///
    /// This function creates a dedicated viewport window for editing and previewing
    /// UML diagrams of the current workspace. It provides a two-panel interface
    /// with a text editor for UML source code and a live preview panel showing
    /// the rendered diagram from PlantUML service.
    ///
    /// # Parameters
    /// - `ctx`: egui context for rendering UI elements and handling interactions
    ///
    /// # Side Effects
    /// - Creates separate viewport window (820x500) for UML editing
    /// - Polls for asynchronous UML preview rendering completion
    /// - Initializes UML text buffer with current workspace diagram
    /// - Starts preview rendering if not already in progress
    /// - Handles clipboard paste operations for UML text input
    /// - Manages zoom functionality for preview panel
    /// - Provides export functionality with format and resolution options
    /// - Shows export dialog for saving rendered diagrams
    ///
    /// # Implementation Details
    /// - Viewport: Uses immediate viewport for separate window management
    /// - Two-panel layout: Text editor (left) and preview (right)
    /// - Text editing: Monospace font with paste support and change detection
    /// - Preview: Async rendering with loading states and error handling
    /// - Zoom: Mouse wheel zoom with configurable limits (0.2x to 6.0x)
    /// - Export: Supports both SVG and PNG formats with custom resolutions
    /// - File dialogs: Platform-appropriate save dialogs with suggested names
    /// - Error handling: Graceful handling of network and rendering failures
    pub(crate) fn render_uml_diagram_window(&mut self, ctx: &egui::Context) {
        if !self.windows.uml_diagram_open {
            return;
        }

        self.poll_uml_preview_render(ctx);
        if self.uml_text_buffer.is_empty() {
            self.uml_text_buffer = self.workspace_manager.current_workspace_uml_diagram();
        }
        if self.uml_preview_hash.is_none() && !self.uml_preview_loading {
            let uml_for_preview = self.uml_text_buffer.clone();
            self.start_uml_preview_render(&uml_for_preview);
        }
        let viewport_id = egui::ViewportId::from_hash_of("uml_diagram");
        let builder = egui::ViewportBuilder::default()
            .with_title("UML diagram")
            .with_inner_size([820.0, 500.0])
            .with_close_button(true);
        ctx.show_viewport_immediate(viewport_id, builder, |ctx, class| {
            if class == egui::ViewportClass::Embedded {
                return;
            }
            if ctx.input(|i| i.viewport().close_requested()) {
                self.windows.uml_diagram_open = false;
            }
            egui::CentralPanel::default().show(ctx, |ui| {
                let export_open_id = egui::Id::new("uml_export_open");
                let mut export_open =
                    ctx.data(|d| d.get_temp::<bool>(export_open_id).unwrap_or(false));
                let controls_h = BUTTON_SIZE.y + 40.0;
                let content_h = (ui.available_height() - controls_h).max(140.0);
                ui.columns(2, |columns| {
                    columns[0].set_height(content_h);
                    egui::Frame::none()
                        .fill(egui::Color32::from_gray(30))
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(64)))
                        .rounding(egui::Rounding::same(6.0))
                        .show(&mut columns[0], |ui| {
                            ui.scope(|ui| {
                                let mut style = ui.style().as_ref().clone();
                                style.visuals.extreme_bg_color = egui::Color32::from_gray(34);
                                style.visuals.code_bg_color = egui::Color32::from_gray(34);
                                style.visuals.widgets.inactive.bg_fill =
                                    egui::Color32::from_gray(34);
                                style.visuals.widgets.hovered.bg_fill =
                                    egui::Color32::from_gray(38);
                                style.visuals.widgets.active.bg_fill = egui::Color32::from_gray(40);
                                ui.set_style(style);
                                let w = (ui.available_width() - 12.0).max(260.0);
                                let h = (ui.available_height() - 8.0).max(180.0);
                                ui.vertical_centered(|ui| {
                                    egui::ScrollArea::both().auto_shrink([false, false]).show(
                                        ui,
                                        |ui| {
                                            let text_response = ui.add_sized(
                                                [w, h],
                                                egui::TextEdit::multiline(
                                                    &mut self.uml_text_buffer,
                                                )
                                                .font(egui::TextStyle::Monospace)
                                                .desired_width(f32::INFINITY)
                                                .desired_rows(22),
                                            );
                                            if text_response.changed() {
                                                self.uml_preview_hash = None;
                                                self.uml_preview_error = None;
                                            }
                                            if text_response.has_focus() {
                                                let mut pasted: Option<String> = None;
                                                ui.input(|i| {
                                                    for ev in &i.events {
                                                        if let egui::Event::Paste(text) = ev {
                                                            pasted = Some(text.clone());
                                                            break;
                                                        }
                                                    }
                                                });
                                                if pasted.is_none() {
                                                    let shortcut = egui::KeyboardShortcut::new(
                                                        egui::Modifiers::COMMAND,
                                                        egui::Key::V,
                                                    );
                                                    let triggered = ui.input_mut(|i| {
                                                        i.consume_shortcut(&shortcut)
                                                    });
                                                    if triggered {
                                                        if let Ok(mut clipboard) =
                                                            arboard::Clipboard::new()
                                                        {
                                                            if let Ok(text) = clipboard.get_text() {
                                                                pasted = Some(text);
                                                            }
                                                        }
                                                    }
                                                }
                                                if let Some(text) = pasted {
                                                    self.uml_text_buffer.push_str(&text);
                                                    self.uml_preview_hash = None;
                                                    self.uml_preview_error = None;
                                                }
                                            }
                                        },
                                    );
                                });
                            });
                        });

                    columns[1].set_height(content_h);
                    egui::Frame::none()
                        .fill(egui::Color32::from_gray(30))
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(64)))
                        .rounding(egui::Rounding::same(6.0))
                        .show(&mut columns[1], |ui| {
                            if ui.rect_contains_pointer(ui.max_rect()) {
                                let zoom_delta = ctx.input(|i| i.zoom_delta());
                                if (zoom_delta - 1.0).abs() > f32::EPSILON {
                                    let base = if self.uml_preview_zoom <= 0.0 {
                                        1.0
                                    } else {
                                        self.uml_preview_zoom
                                    };
                                    self.uml_preview_zoom = (base * zoom_delta).clamp(0.2, 6.0);
                                }
                            }
                            ui.set_min_height((ui.available_height() - 40.0).max(180.0));
                            if let Some(texture) = &self.uml_preview_texture {
                                egui::ScrollArea::both().show(ui, |ui| {
                                    let size = texture.size_vec2();
                                    if self.uml_preview_zoom <= 0.0 {
                                        let avail = ui.available_size();
                                        let fit = (avail.x / size.x).min(avail.y / size.y);
                                        self.uml_preview_zoom = fit.clamp(0.2, 6.0);
                                    }
                                    let render_size = size * self.uml_preview_zoom;
                                    ui.centered_and_justified(|ui| {
                                        ui.image((texture.id(), render_size));
                                    });
                                });
                            } else if let Some(err) = &self.uml_preview_error {
                                ui.centered_and_justified(|ui| {
                                    ui.label(
                                        RichText::new(err)
                                            .color(egui::Color32::from_rgb(220, 120, 120)),
                                    );
                                });
                            } else if self.uml_preview_loading {
                                ui.centered_and_justified(|ui| {
                                    ui.add(egui::Spinner::new().size(24.0));
                                });
                            } else {
                                ui.centered_and_justified(|ui| {
                                    ui.label(
                                        RichText::new("Generating preview...")
                                            .color(egui::Color32::from_gray(180)),
                                    );
                                });
                            }
                        });
                });
                ui.add_space(8.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if styled_button(ui, "Export").clicked() {
                        export_open = true;
                    }
                    if styled_button(ui, "Regenerate UML").clicked() {
                        self.uml_text_buffer =
                            self.workspace_manager.current_workspace_uml_diagram();
                        self.uml_preview_hash = None;
                        self.uml_preview_error = None;
                        self.uml_preview_texture = None;
                        let uml_for_preview = self.uml_text_buffer.clone();
                        self.start_uml_preview_render(&uml_for_preview);
                    }
                });

                if export_open {
                    let mut save_requested = false;
                    egui::Window::new("UML Export")
                        .resizable(false)
                        .default_size(egui::vec2(420.0, 220.0))
                        .open(&mut export_open)
                        .show(ctx, |ui| {
                            ui.checkbox(&mut self.uml_export_svg, "Export as SVG");
                            ui.horizontal(|ui| {
                                ui.label("Resolution:");
                                let old_width = self.uml_export_width;
                                ui.add_enabled(
                                    !self.uml_export_svg,
                                    egui::DragValue::new(&mut self.uml_export_width)
                                        .clamp_range(400..=4000)
                                        .suffix("px"),
                                );
                                if self.uml_export_width != old_width && !self.uml_export_svg {
                                    let ratio = 16.0 / 9.0;
                                    self.uml_export_height =
                                        (self.uml_export_width as f32 / ratio) as u32;
                                }
                                ui.label("x");
                                let old_height = self.uml_export_height;
                                ui.add_enabled(
                                    !self.uml_export_svg,
                                    egui::DragValue::new(&mut self.uml_export_height)
                                        .clamp_range(300..=3000)
                                        .suffix("px"),
                                );
                                if self.uml_export_height != old_height && !self.uml_export_svg {
                                    let ratio = 16.0 / 9.0;
                                    self.uml_export_width =
                                        (self.uml_export_height as f32 * ratio) as u32;
                                }
                            });
                            ui.add_space(8.0);
                            if ui
                                .add_enabled(
                                    !self.uml_preview_loading && self.uml_preview_error.is_none(),
                                    egui::Button::new("Save"),
                                )
                                .clicked()
                            {
                                save_requested = true;
                            }
                        });
                    if save_requested {
                        export_open = false;
                        let ext = if self.uml_export_svg { "svg" } else { "png" };
                        let file_name = format!(
                            "{}.{}",
                            self.workspace_manager.workspace.name.replace(' ', "_"),
                            ext
                        );
                        let file = if crate::has_rt_capabilities() {
                            crate::zenity_file_dialog_with_name("save", None, Some(&file_name))
                        } else {
                            rfd::FileDialog::new().set_file_name(&file_name).save_file()
                        };
                        if let Some(path) = file {
                            let uml_text = self.uml_text_buffer.clone();
                            let export_svg = self.uml_export_svg;
                            let export_width = self.uml_export_width;
                            let export_height = self.uml_export_height;
                            match self.request_uml_render(&uml_text, export_svg) {
                                Ok(bytes) => {
                                    let bytes = if export_svg {
                                        bytes
                                    } else {
                                        match Self::resize_png(&bytes, export_width, export_height)
                                        {
                                            Ok(resized) => resized,
                                            Err(err) => {
                                                self.show_info(
                                                    "UML",
                                                    &format!("Resize failed: {err}"),
                                                );
                                                return;
                                            }
                                        }
                                    };
                                    match std::fs::write(&path, bytes) {
                                        Ok(()) => self.show_info("UML", "Diagram saved"),
                                        Err(err) => {
                                            self.show_info("UML", &format!("Save failed: {err}"))
                                        }
                                    }
                                }
                                Err(err) => self.show_info("UML", &format!("Render failed: {err}")),
                            }
                        }
                    }
                }
                ctx.data_mut(|d| d.insert_temp(export_open_id, export_open));
            });
        });
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
                        ui.heading(&self.build_dialog.title);
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
