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
use crate::{
    has_rt_capabilities, spawn_file_dialog_thread, zenity_file_dialog,
    PluginFieldDraft,
};
use rtsyn_cli::plugin_creator::{
    create_plugin, CreatorBehavior, PluginCreateRequest, PluginKindType, PluginLanguage,
};
use std::sync::mpsc;

impl GuiApp {
/// Returns the default value string for a given plugin field type.
    ///
    /// This function maps plugin field types to their appropriate default values
    /// used in the plugin creation wizard. The defaults ensure newly created
    /// plugin fields have sensible initial values.
    ///
    /// # Parameters
    /// - `ty`: The type name as a string (e.g., "bool", "i64", "string")
    ///
    /// # Returns
    /// A static string slice containing the default value:
    /// - "false" for boolean types
    /// - "0" for integer types (i64, i32)
    /// - "" (empty string) for string, file, and path types
    /// - "0.0" for all other types (typically floating-point)
    ///
    /// # Example
    /// ```
    /// assert_eq!(GuiApp::plugin_creator_default_by_type("bool"), "false");
    /// assert_eq!(GuiApp::plugin_creator_default_by_type("i64"), "0");
    /// assert_eq!(GuiApp::plugin_creator_default_by_type("f64"), "0.0");
    /// ```
    pub(super) fn plugin_creator_default_by_type(ty: &str) -> &'static str {
        match ty.trim().to_ascii_lowercase().as_str() {
            "bool" => "false",
            "i64" | "i32" => "0",
            "string" | "file" | "path" => "",
            _ => "0.0",
        }
    }

    /// Opens a file dialog for selecting a plugin installation folder.
    ///
    /// This function initiates an asynchronous file dialog to allow users to browse
    /// and select a folder containing plugins to install. It prevents multiple
    /// dialogs from being opened simultaneously and provides user feedback.
    ///
    /// # Behavior
    /// - Checks if a dialog is already open and shows a status message if so
    /// - Creates a new channel for receiving the dialog result
    /// - Spawns a background thread to handle the file dialog
    /// - Uses platform-appropriate dialog (zenity for RT systems, rfd otherwise)
    /// - Updates the application status to inform the user
    ///
    /// # Side Effects
    /// - Sets `file_dialogs.install_dialog_rx` to track the dialog state
    /// - Updates the application status message
    /// - Spawns a background thread for dialog handling
    pub(super) fn open_install_dialog(&mut self) {
        if self.file_dialogs.install_dialog_rx.is_some() {
            self.status = "Plugin dialog already open".to_string();
            return;
        }

        let (tx, rx) = mpsc::channel();
        self.file_dialogs.install_dialog_rx = Some(rx);
        self.status = "Opening plugin folder dialog...".to_string();

        crate::spawn_file_dialog_thread(move || {
            let folder = if crate::has_rt_capabilities() {
                crate::zenity_file_dialog("folder", None)
            } else {
                rfd::FileDialog::new().pick_folder()
            };
            let _ = tx.send(folder);
        });
    }

    /// Opens a file dialog for selecting the destination folder for a new plugin.
    ///
    /// This function launches a folder selection dialog for the plugin creation wizard,
    /// allowing users to choose where their new plugin should be generated. It maintains
    /// state to prevent multiple dialogs and remembers the last selected path.
    ///
    /// # Behavior
    /// - Prevents opening multiple dialogs simultaneously
    /// - Uses the last selected path as the starting directory if available
    /// - Creates an asynchronous channel for dialog result communication
    /// - Spawns a background thread with platform-appropriate dialog
    /// - Updates status to guide the user
    ///
    /// # Side Effects
    /// - Sets `file_dialogs.plugin_creator_dialog_rx` to track dialog state
    /// - Updates application status message
    /// - Spawns background thread for dialog handling
    pub(super) fn open_plugin_creator_folder_dialog(&mut self) {
        if self.file_dialogs.plugin_creator_dialog_rx.is_some() {
            self.status = "Plugin creator dialog already open".to_string();
            return;
        }

        let (tx, rx) = mpsc::channel();
        self.file_dialogs.plugin_creator_dialog_rx = Some(rx);
        self.status = "Select destination folder for new plugin".to_string();

        let start_dir = self.plugin_creator_last_path.clone();
        spawn_file_dialog_thread(move || {
            let folder = if has_rt_capabilities() {
                zenity_file_dialog("folder", None)
            } else {
                let mut dialog = rfd::FileDialog::new();
                if let Some(dir) = start_dir {
                    dialog = dialog.set_directory(dir);
                }
                dialog.pick_folder()
            };
            let _ = tx.send(folder);
        });
    }

    /// Opens the new plugin creation window.
    ///
    /// This function activates the plugin creation wizard window and sets up
    /// the necessary state for window focus management. The window allows users
    /// to configure and generate new plugin scaffolds.
    ///
    /// # Side Effects
    /// - Sets `windows.new_plugin_open` to true to show the window
    /// - Queues window focus to ensure the new window receives input focus
    /// - The window will be rendered in the next UI frame
    pub(crate) fn open_new_plugin_window(&mut self) {
        self.windows.new_plugin_open = true;
        self.pending_window_focus = Some(WindowFocus::NewPlugin);
    }

    /// Converts plugin field drafts to a specification string.
    ///
    /// This function transforms a collection of plugin field drafts (from the creation wizard)
    /// into a formatted specification string that can be used by the plugin creation system.
    /// Each field is converted to "name:type" format, with empty names filtered out.
    ///
    /// # Parameters
    /// - `entries`: Slice of PluginFieldDraft objects containing field definitions
    ///
    /// # Returns
    /// A string with each field on a separate line in "name:type" format.
    /// Empty or whitespace-only names are excluded from the output.
    ///
    /// # Example
    /// ```
    /// // Input: [PluginFieldDraft{name: "freq", type_name: "f64"}, PluginFieldDraft{name: "enabled", type_name: "bool"}]
    /// // Output: "freq:f64\nenabled:bool"
    /// ```
    pub(super) fn plugin_creator_draft_to_spec(entries: &[PluginFieldDraft]) -> String {
        entries
            .iter()
            .filter_map(|entry| {
                let name = entry.name.trim();
                if name.is_empty() {
                    None
                } else {
                    Some(format!("{name}:{}", entry.type_name.trim()))
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Parses a plugin specification string into name-type pairs.
    ///
    /// This function takes a multi-line specification string where each line contains
    /// a field definition in "name:type" format and converts it into a vector of
    /// (name, type) tuples. It handles malformed lines gracefully and filters out
    /// empty entries.
    ///
    /// # Parameters
    /// - `spec`: Multi-line string with field specifications
    ///
    /// # Returns
    /// Vector of (String, String) tuples representing (field_name, field_type).
    /// Lines without names or empty lines are filtered out.
    /// Missing types default to "f64".
    ///
    /// # Example
    /// ```
    /// let spec = "frequency:f64\nenabled:bool\ncount";
    /// let result = GuiApp::plugin_creator_parse_spec(spec);
    /// // Returns: [("frequency", "f64"), ("enabled", "bool"), ("count", "f64")]
    /// ```
    pub(super) fn plugin_creator_parse_spec(spec: &str) -> Vec<(String, String)> {
        spec.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| {
                let mut parts = line.splitn(2, ':');
                let name = parts.next().unwrap_or("").trim().to_string();
                let ty = parts.next().unwrap_or("f64").trim().to_string();
                (name, ty)
            })
            .filter(|(name, _)| !name.is_empty())
            .collect()
    }

    /// Creates a new plugin from the current draft configuration.
    ///
    /// This function takes the plugin creation wizard's current draft state and
    /// generates a complete plugin scaffold in the specified parent directory.
    /// It validates the draft, converts field specifications, and delegates to
    /// the lower-level creation function.
    ///
    /// # Parameters
    /// - `parent`: The directory path where the plugin should be created
    ///
    /// # Returns
    /// - `Ok(PathBuf)`: Path to the created plugin directory on success
    /// - `Err(String)`: Error message if creation fails or validation fails
    ///
    /// # Validation
    /// - Ensures plugin name is not empty
    /// - Converts all field drafts to specification strings
    /// - Validates field configurations
    ///
    /// # Side Effects
    /// - Creates plugin directory structure and files
    /// - Generates source code scaffolding
    /// - Creates manifest and configuration files
    pub(crate) fn create_plugin_from_draft(&self, parent: &Path) -> Result<PathBuf, String> {
        let name = self.new_plugin_draft.name.trim();
        if name.is_empty() {
            return Err("Plugin name is required".to_string());
        }
        let vars_spec = Self::plugin_creator_draft_to_spec(&self.new_plugin_draft.variables);
        let inputs_spec = Self::plugin_creator_draft_to_spec(&self.new_plugin_draft.inputs);
        let outputs_spec = Self::plugin_creator_draft_to_spec(&self.new_plugin_draft.outputs);
        let internal_spec =
            Self::plugin_creator_draft_to_spec(&self.new_plugin_draft.internal_variables);
        self.create_plugin_from_specs(
            name,
            &self.new_plugin_draft.language,
            &self.new_plugin_draft.main_characteristics,
            self.new_plugin_draft.autostart,
            self.new_plugin_draft.supports_start_stop,
            self.new_plugin_draft.supports_restart,
            self.new_plugin_draft.supports_apply,
            self.new_plugin_draft.external_window,
            self.new_plugin_draft.starts_expanded,
            &self.new_plugin_draft.required_input_ports_csv,
            &self.new_plugin_draft.required_output_ports_csv,
            &vars_spec,
            &inputs_spec,
            &outputs_spec,
            &internal_spec,
            parent,
        )
    }

    /// Creates a plugin from detailed specifications and configuration.
    ///
    /// This is the core plugin creation function that takes all the necessary parameters
    /// and generates a complete plugin scaffold. It handles validation, specification parsing,
    /// and delegates to the plugin creation system.
    ///
    /// # Parameters
    /// - `name`: Plugin name (must not be empty)
    /// - `language`: Programming language ("rust", "c", "cpp")
    /// - `main`: Main description/characteristics text
    /// - `autostart`: Whether plugin should start automatically
    /// - `supports_start_stop`: Whether plugin supports start/stop controls
    /// - `supports_restart`: Whether plugin supports restart functionality
    /// - `supports_apply`: Whether plugin supports apply/modify operations
    /// - `external_window`: Whether plugin opens in external window
    /// - `starts_expanded`: Whether plugin UI starts in expanded state
    /// - `required_input_ports_csv`: Comma-separated list of required input ports
    /// - `required_output_ports_csv`: Comma-separated list of required output ports
    /// - `vars_spec`: Variable specifications in "name:type=default" format
    /// - `inputs_spec`: Input port specifications
    /// - `outputs_spec`: Output port specifications
    /// - `internals_spec`: Internal variable specifications
    /// - `parent`: Parent directory for plugin creation
    ///
    /// # Returns
    /// - `Ok(PathBuf)`: Path to created plugin on success
    /// - `Err(String)`: Detailed error message on failure
    ///
    /// # Validation
    /// - Plugin name must not be empty
    /// - Language must be valid (rust/c/cpp)
    /// - All specifications must parse correctly
    pub(super) fn create_plugin_from_specs(
        &self,
        name: &str,
        language: &str,
        main: &str,
        autostart: bool,
        supports_start_stop: bool,
        supports_restart: bool,
        supports_apply: bool,
        external_window: bool,
        starts_expanded: bool,
        required_input_ports_csv: &str,
        required_output_ports_csv: &str,
        vars_spec: &str,
        inputs_spec: &str,
        outputs_spec: &str,
        internals_spec: &str,
        parent: &Path,
    ) -> Result<PathBuf, String> {
        let title = name.trim();
        if title.is_empty() {
            return Err("Plugin name is required".to_string());
        }
        let parsed_language = PluginLanguage::parse(language)?;

        let vars = Self::plugin_creator_parse_spec(vars_spec);
        let variables = vars
            .iter()
            .map(|(name, ty)| {
                let default = self
                    .new_plugin_draft
                    .variables
                    .iter()
                    .find(|v| v.name.trim() == name)
                    .map(|v| v.default_value.as_str())
                    .unwrap_or_else(|| Self::plugin_creator_default_by_type(ty));
                rtsyn_cli::plugin_creator::parse_variable_line(&format!("{name}:{ty}={default}"))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let inputs: Vec<String> = Self::plugin_creator_parse_spec(inputs_spec)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        let outputs: Vec<String> = Self::plugin_creator_parse_spec(outputs_spec)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        let internals: Vec<String> = Self::plugin_creator_parse_spec(internals_spec)
            .into_iter()
            .map(|(name, _)| name)
            .collect();

        let req = PluginCreateRequest {
            base_dir: parent.to_path_buf(),
            name: title.to_string(),
            description: if main.trim().is_empty() {
                "Generated by plugin_creator".to_string()
            } else {
                main.lines()
                    .next()
                    .unwrap_or("Generated by plugin_creator")
                    .to_string()
            },
            language: parsed_language,
            plugin_type: PluginKindType::Standard,
            behavior: CreatorBehavior {
                autostart,
                supports_start_stop,
                supports_restart,
                supports_apply,
                external_window,
                starts_expanded,
                required_input_ports: required_input_ports_csv
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect(),
                required_output_ports: required_output_ports_csv
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect(),
            },
            inputs,
            outputs,
            internal_variables: internals,
            variables,
        };

        create_plugin(&req)
    }
}
