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

mod cards;
mod config;
mod state;
mod windows;
mod wizard;

impl GuiApp {
    const NEW_PLUGIN_TYPES: [&'static str; 6] = ["f64", "f32", "i64", "i32", "bool", "string"];

/// Opens the plugin management window.
    ///
    /// This function activates the plugin management interface where users can
    /// install, uninstall, and manage plugins. It refreshes the plugin detection
    /// and sets up the window state for proper focus management.
    ///
    /// # Side Effects
    /// - Sets `windows.manage_plugins_open` to true
    /// - Triggers plugin detection scan to refresh available plugins
    /// - Resets selected plugin index
    /// - Queues window focus for the management window
    /// - Window will be rendered in the next UI frame
    pub(crate) fn open_manage_plugins(&mut self) {
        self.windows.manage_plugins_open = true;
        self.scan_detected_plugins();
        self.windows.manage_plugin_selected_index = None;
        self.pending_window_focus = Some(WindowFocus::ManagePlugins);
    }

    /// Opens the plugin installation window.
    ///
    /// This function activates the plugin installation interface where users can
    /// browse and install available plugins. It refreshes plugin detection to
    /// show the most current available plugins and prepares the window state.
    ///
    /// # Side Effects
    /// - Sets `windows.install_plugins_open` to true
    /// - Triggers plugin detection scan to refresh available plugins
    /// - Resets selected plugin index to clear previous selections
    /// - Queues window focus for the installation window
    /// - Window will be rendered in the next UI frame
    pub(crate) fn open_install_plugins(&mut self) {
        self.windows.install_plugins_open = true;
        self.scan_detected_plugins();
        self.windows.install_selected_index = None;
        self.pending_window_focus = Some(WindowFocus::InstallPlugins);
    }

    /// Opens the plugin uninstallation window.
    ///
    /// This function activates the plugin uninstallation interface where users can
    /// remove installed plugins from the system. It prepares the window state
    /// and clears any previous selections.
    ///
    /// # Side Effects
    /// - Sets `windows.uninstall_plugins_open` to true
    /// - Resets selected plugin index to clear previous selections
    /// - Queues window focus for the uninstallation window
    /// - Window will be rendered in the next UI frame
    pub(crate) fn open_uninstall_plugins(&mut self) {
        self.windows.uninstall_plugins_open = true;
        self.windows.uninstall_selected_index = None;
        self.pending_window_focus = Some(WindowFocus::UninstallPlugins);
    }

    /// Opens the plugin addition window.
    ///
    /// This function activates the plugin addition interface where users can
    /// browse installed plugins and add them to the current workspace. It
    /// prepares the window state for plugin selection and addition.
    ///
    /// # Side Effects
    /// - Sets `windows.plugins_open` to true
    /// - Resets selected plugin index to clear previous selections
    /// - Queues window focus for the plugin addition window
    /// - Window will be rendered in the next UI frame
    pub(crate) fn open_plugins(&mut self) {
        self.windows.plugins_open = true;
        self.windows.plugin_selected_index = None;
        self.pending_window_focus = Some(WindowFocus::Plugins);
    }
}
