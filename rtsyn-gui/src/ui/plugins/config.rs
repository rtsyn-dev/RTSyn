//! Plugin management and UI rendering functionality for RTSyn GUI.
//!
//! This module provides comprehensive plugin management capabilities including:
//! - Plugin card rendering and interaction
//! - Plugin installation, uninstallation, and management windows
//! - Plugin creation wizard with field configuration
//! - Plugin configuration dialogs
//! - Context menus and plugin operations
//!
//! The module handles both built-in app plugins (csv_recorder, live_plotter, etc.)
//! and user-installed plugins with dynamic UI schema support.

use super::*;
use crate::WindowFocus;

impl GuiApp {
/// Determines the appropriate window size for plugin configuration dialogs.
    ///
    /// This function returns optimal window dimensions based on the plugin type,
    /// ensuring that configuration windows are appropriately sized for their
    /// content and provide a good user experience.
    ///
    /// # Parameters
    /// - `plugin_kind`: The plugin type identifier string
    ///
    /// # Returns
    /// An `egui::Vec2` containing the width and height in pixels
    ///
    /// # Size Mapping
    /// - csv_recorder: 520x360 (larger for file path and column configuration)
    /// - live_plotter: 420x240 (medium for plotting parameters)
    /// - Default: 320x180 (compact for basic plugins)
    ///
    /// # Design Rationale
    /// Different plugin types have varying configuration complexity:
    /// - CSV recorder needs space for file paths and column management
    /// - Live plotter requires room for plotting parameters and settings
    /// - Most plugins have minimal configuration needs
    pub(super) fn plugin_config_window_size(plugin_kind: &str) -> egui::Vec2 {
        match plugin_kind {
            "csv_recorder" => egui::vec2(520.0, 360.0),
            "live_plotter" => egui::vec2(420.0, 240.0),
            _ => egui::vec2(320.0, 180.0),
        }
    }

    /// Renders the contents of a plugin configuration dialog.
    ///
    /// This function creates the UI content for plugin configuration windows,
    /// displaying plugin information and editable configuration parameters.
    /// It provides a consistent interface for configuring plugin-specific settings.
    ///
    /// # Parameters
    /// - `ui`: The egui UI context for rendering
    /// - `plugin_id`: Unique identifier of the plugin being configured
    /// - `name_by_kind`: Mapping from plugin kinds to display names
    ///
    /// # Configuration Elements
    /// - Plugin identification: ID badge and display name
    /// - Priority setting: Execution priority with drag value control
    /// - Plugin-specific parameters (extensible for future features)
    ///
    /// # Priority Configuration
    /// - Allows setting plugin execution priority (0-99 range)
    /// - Higher priority plugins execute first in the processing chain
    /// - Uses drag value control for intuitive adjustment
    /// - Automatically clamps values to valid range
    ///
    /// # Error Handling
    /// - Gracefully handles missing plugins with error message
    /// - Validates plugin existence before configuration
    /// - Provides user feedback for invalid states
    ///
    /// # Side Effects
    /// - Updates plugin configuration in workspace
    /// - Marks workspace as dirty when changes are made
    /// - Triggers workspace synchronization for priority changes
    pub(super) fn render_plugin_config_contents(
        &mut self,
        ui: &mut egui::Ui,
        plugin_id: u64,
        name_by_kind: &HashMap<String, String>,
    ) {
        let Some(plugin_index) = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .position(|p| p.id == plugin_id)
        else {
            ui.label("Plugin not found.");
            return;
        };

        let plugin_kind = self.workspace_manager.workspace.plugins[plugin_index]
            .kind
            .clone();
        let display_name = name_by_kind
            .get(&plugin_kind)
            .cloned()
            .unwrap_or_else(|| Self::display_kind(&plugin_kind));
        let mut plugin_changed = false;

        ui.horizontal(|ui| {
            let (id_rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(id_rect, 8.0, egui::Color32::from_gray(60));
            ui.painter().text(
                id_rect.center(),
                egui::Align2::CENTER_CENTER,
                plugin_id.to_string(),
                egui::FontId::proportional(12.0),
                egui::Color32::from_rgb(200, 200, 210),
            );
            ui.label(RichText::new(display_name).strong().size(16.0));
        });
        ui.add_space(6.0);

        let plugin = &mut self.workspace_manager.workspace.plugins[plugin_index];
        let mut priority = plugin.priority;
        kv_row_wrapped(ui, "Priority", 140.0, |ui| {
            if ui
                .add_sized([90.0, 0.0], egui::DragValue::new(&mut priority).speed(1))
                .changed()
            {
                plugin_changed = true;
            }
        });
        priority = priority.clamp(0, 99);
        if plugin.priority != priority {
            plugin.priority = priority;
            plugin_changed = true;
        }

        if plugin_changed {
            self.mark_workspace_dirty();
        }
    }

    /// Renders plugin configuration windows with support for both modal and external viewports.
    ///
    /// This function manages the rendering of plugin configuration interfaces, supporting
    /// both traditional modal windows and external viewport windows for plugins that
    /// require dedicated configuration spaces.
    ///
    /// # Dual Window Support
    /// - External viewports: For plugins requiring dedicated configuration windows
    /// - Modal windows: For standard plugin configuration within the main interface
    /// - Automatic detection based on plugin type and configuration
    ///
    /// # External Viewport Handling
    /// - Creates separate viewports for plugins using external configuration
    /// - Each viewport has a unique ID based on plugin ID
    /// - Viewports are titled with plugin name and ID
    /// - Handles viewport lifecycle and close behavior
    ///
    /// # Modal Window Features
    /// - Fixed size based on plugin type requirements
    /// - Proper focus management and window layering
    /// - Centered positioning for optimal user experience
    /// - Close button and ESC key support
    ///
    /// # Window State Management
    /// - Tracks which plugins should use external viewports
    /// - Manages window open/close state
    /// - Handles window focus transitions
    /// - Cleans up state when windows are closed
    ///
    /// # Side Effects
    /// - Creates and manages external viewports
    /// - Updates window state and focus tracking
    /// - Renders plugin configuration content
    /// - Manages window rectangle tracking for interactions
    pub(crate) fn render_plugin_config_window(&mut self, ctx: &egui::Context) {
        let name_by_kind: HashMap<String, String> = self
            .plugin_manager
            .installed_plugins
            .iter()
            .map(|plugin| (plugin.manifest.kind.clone(), plugin.manifest.name.clone()))
            .collect();

        let external_plugin_ids: Vec<u64> = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .filter(|plugin| self.plugin_uses_external_config_viewport(&plugin.kind))
            .map(|plugin| plugin.id)
            .collect();
        for plugin_id in external_plugin_ids {
            let plugin_kind = self
                .workspace_manager
                .workspace
                .plugins
                .iter()
                .find(|p| p.id == plugin_id)
                .map(|p| p.kind.as_str())
                .unwrap_or("");
            let window_size = Self::plugin_config_window_size(plugin_kind);
            let display_name = name_by_kind
                .get(plugin_kind)
                .cloned()
                .unwrap_or_else(|| Self::display_kind(plugin_kind));
            let viewport_id = egui::ViewportId::from_hash_of(("plugin_config", plugin_id));
            let builder = egui::ViewportBuilder::default()
                .with_title(format!("{display_name} #{plugin_id}"))
                .with_inner_size([window_size.x, window_size.y])
                .with_close_button(false);
            ctx.show_viewport_immediate(viewport_id, builder, |ctx, class| {
                if class == egui::ViewportClass::Embedded {
                    return;
                }
                egui::CentralPanel::default().show(ctx, |ui| {
                    self.render_plugin_config_contents(ui, plugin_id, &name_by_kind);
                });
            });
        }

        if !self.windows.plugin_config_open {
            self.windows.plugin_config_id = None;
            return;
        }

        let plugin_id = match self.windows.plugin_config_id {
            Some(id) => id,
            None => {
                self.windows.plugin_config_open = false;
                return;
            }
        };
        let plugin_kind = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .find(|p| p.id == plugin_id)
            .map(|p| p.kind.as_str())
            .unwrap_or("");
        if self.plugin_uses_external_window(plugin_kind) {
            self.windows.plugin_config_open = false;
            self.windows.plugin_config_id = None;
            return;
        }

        let window_size = Self::plugin_config_window_size(plugin_kind);
        let mut open = self.windows.plugin_config_open;
        let default_pos = Self::center_window(ctx, window_size);
        let response = egui::Window::new("Plugin config")
            .open(&mut open)
            .resizable(false)
            .default_pos(default_pos)
            .default_size(window_size)
            .fixed_size(window_size)
            .show(ctx, |ui| {
                self.render_plugin_config_contents(ui, plugin_id, &name_by_kind);
            });
        if let Some(response) = response {
            self.window_rects.push(response.response.rect);
            if !self.confirm_dialog.open
                && (response.response.clicked() || response.response.dragged())
            {
                ctx.move_to_top(response.response.layer_id);
            }
            if self.pending_window_focus == Some(WindowFocus::PluginConfig) {
                ctx.move_to_top(response.response.layer_id);
                self.pending_window_focus = None;
            }
        }
        self.windows.plugin_config_open = open;
        if !self.windows.plugin_config_open {
            self.windows.plugin_config_id = None;
        }
    }
}
