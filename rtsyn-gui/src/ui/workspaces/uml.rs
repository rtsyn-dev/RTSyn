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
use image::ImageEncoder;
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
    pub(super) fn request_uml_render(
        &mut self,
        uml: &str,
        as_svg: bool,
    ) -> Result<Vec<u8>, String> {
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
    pub(super) fn resize_png(bytes: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
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
    pub(super) fn start_uml_preview_render(&mut self, uml: &str) {
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
    pub(super) fn poll_uml_preview_render(&mut self, ctx: &egui::Context) {
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
}
