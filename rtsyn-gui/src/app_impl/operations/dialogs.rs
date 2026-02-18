use crate::{spawn_file_dialog_thread, BuildAction, BuildResult, GuiApp};
use rtsyn_core::plugin::PluginManager;
use rtsyn_runtime::LogicMessage;
use std::sync::mpsc;

impl GuiApp {
    /// Polls the build dialog for completion and handles the build result.
    ///
    /// This function checks if a plugin build operation has completed by polling
    /// the build dialog's receiver channel. When a build completes, it processes
    /// the result based on the build action type (Install or Reinstall) and
    /// updates the UI accordingly.
    ///
    /// # Behavior
    /// - For successful Install actions: installs the plugin, shows success notification
    /// - For successful Reinstall actions: refreshes the plugin, shows completion message
    /// - For failed builds: shows error notification and updates status
    /// - Automatically closes the build dialog when processing is complete
    pub(crate) fn poll_build_dialog(&mut self) {
        let result = match &self.build_dialog.rx {
            Some(rx) => rx.try_recv().ok(),
            None => None,
        };
        if let Some(result) = result {
            self.build_dialog.rx = None;
            self.build_dialog.in_progress = false;
            if result.success {
                match result.action {
                    BuildAction::Install {
                        path,
                        removable,
                        persist,
                    } => {
                        let prev_count = self.plugin_manager.installed_plugins.len();
                        self.install_plugin_from_folder(path, removable, persist);
                        let was_installed =
                            self.plugin_manager.installed_plugins.len() > prev_count;
                        if was_installed {
                            self.show_info("Plugin", "Plugin built and installed");
                        } else {
                            let msg = self.status.clone();
                            self.show_info("Plugin", &msg);
                        }
                        self.scan_detected_plugins();
                    }
                    BuildAction::Reinstall { kind, path } => {
                        self.refresh_installed_plugin(kind.clone(), &path);
                        self.scan_detected_plugins();
                        let msg = if path.as_os_str().is_empty() {
                            "Plugin refreshed"
                        } else {
                            "Plugin rebuilt"
                        };
                        self.status = msg.to_string();
                        self.show_info("Plugin", msg);
                    }
                }
            } else {
                self.status = "Plugin build failed".to_string();
                self.show_info("Plugin", "Plugin build failed");
            }
            self.build_dialog.open = false;
        }
    }

    /// Polls the install dialog for folder selection and initiates plugin installation.
    ///
    /// This function monitors the install dialog's receiver channel for user folder
    /// selection. When a folder is selected, it extracts the folder name as a label
    /// and starts the plugin build process with installation parameters.
    ///
    /// # Parameters
    /// - Sets `removable: true` - allows the plugin to be uninstalled later
    /// - Sets `persist: true` - saves the plugin installation persistently
    ///
    /// # Behavior
    /// - On folder selection: starts plugin build with Install action
    /// - On cancellation: updates status to indicate cancellation
    /// - Cleans up the receiver channel after processing
    pub(crate) fn poll_install_dialog(&mut self) {
        let result = match &self.file_dialogs.install_dialog_rx {
            Some(rx) => rx.try_recv().ok(),
            None => None,
        };

        if let Some(selection) = result {
            self.file_dialogs.install_dialog_rx = None;
            if let Some(folder) = selection {
                let label = folder
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("plugin")
                    .to_string();
                self.start_plugin_build(
                    BuildAction::Install {
                        path: folder,
                        removable: true,
                        persist: true,
                    },
                    label,
                );
            } else {
                self.status = "Plugin install cancelled".to_string();
            }
        }
    }

    /// Polls the import dialog for workspace file selection and imports the workspace.
    ///
    /// This function monitors the import dialog's receiver channel for user file
    /// selection. When a workspace file is selected, it initiates the workspace
    /// import process using the selected path.
    ///
    /// # Behavior
    /// - On file selection: calls `import_workspace_from_path` with the selected path
    /// - On cancellation: silently handles the cancellation without status updates
    /// - Cleans up the receiver channel after processing
    pub(crate) fn poll_import_dialog(&mut self) {
        let result = match &self.file_dialogs.import_dialog_rx {
            Some(rx) => rx.try_recv().ok(),
            None => None,
        };
        if let Some(selection) = result {
            self.file_dialogs.import_dialog_rx = None;
            if let Some(path) = selection {
                self.import_workspace_from_path(&path);
            }
        }
    }

    /// Polls the load dialog for workspace file selection and loads the workspace.
    ///
    /// This function monitors the load dialog's receiver channel for user file
    /// selection. When a workspace file is selected, it updates the workspace
    /// manager's path and initiates the workspace loading process.
    ///
    /// # Behavior
    /// - On file selection: updates `workspace_manager.workspace_path` and calls `load_workspace`
    /// - On cancellation: silently handles the cancellation without status updates
    /// - Cleans up the receiver channel after processing
    pub(crate) fn poll_load_dialog(&mut self) {
        let result = match &self.file_dialogs.load_dialog_rx {
            Some(rx) => rx.try_recv().ok(),
            None => None,
        };
        if let Some(selection) = result {
            self.file_dialogs.load_dialog_rx = None;
            if let Some(path) = selection {
                self.workspace_manager.workspace_path = path;
                self.load_workspace();
            }
        }
    }

    /// Polls the CSV path dialog for file selection and updates plugin configuration.
    ///
    /// This function monitors the CSV path dialog's receiver channel for user file
    /// selection. When a CSV file is selected, it updates the target plugin's
    /// configuration with the selected path and sends a variable update message
    /// to the logic thread.
    ///
    /// # Behavior
    /// - On file selection: updates plugin config with path and sets `path_autogen: false`
    /// - Sends `SetPluginVariable` message to logic thread with the new path
    /// - Marks the workspace as dirty to indicate unsaved changes
    /// - Requires a valid `csv_path_target_plugin_id` to process the selection
    /// - Cleans up both the receiver channel and target plugin ID after processing
    pub(crate) fn poll_csv_path_dialog(&mut self) {
        let result = match &self.file_dialogs.csv_path_dialog_rx {
            Some(rx) => rx.try_recv().ok(),
            None => None,
        };
        if let Some(selection) = result {
            self.file_dialogs.csv_path_dialog_rx = None;
            let plugin_id = self.csv_path_target_plugin_id.take();
            if let (Some(path), Some(id)) = (selection, plugin_id) {
                let path_str = path.to_string_lossy().to_string();
                let _ = self
                    .state_sync
                    .logic_tx
                    .send(LogicMessage::SetPluginVariable(
                        id,
                        "path".to_string(),
                        serde_json::Value::String(path_str.clone()),
                    ));
                if let Some(plugin) = self
                    .workspace_manager
                    .workspace
                    .plugins
                    .iter_mut()
                    .find(|p| p.id == id)
                {
                    if let serde_json::Value::Object(ref mut map) = plugin.config {
                        map.insert("path".to_string(), serde_json::Value::String(path_str));
                        map.insert("path_autogen".to_string(), serde_json::Value::Bool(false));
                        self.mark_workspace_dirty();
                    }
                }
            }
        }
    }

    /// Polls the plugin creator dialog for directory selection and creates a new plugin scaffold.
    ///
    /// This function monitors the plugin creator dialog's receiver channel for user
    /// directory selection. When a parent directory is selected, it creates a new
    /// plugin scaffold using the current plugin draft configuration.
    ///
    /// # Behavior
    /// - On directory selection: saves the path and calls `create_plugin_from_draft`
    /// - On success: updates status with the created plugin path and shows info notification
    /// - On error: updates status with error message and shows error notification
    /// - On cancellation: updates status to indicate cancellation
    /// - Stores the last used path in `plugin_creator_last_path` for future reference
    /// - Cleans up the receiver channel after processing
    pub(crate) fn poll_plugin_creator_dialog(&mut self) {
        let result = match &self.file_dialogs.plugin_creator_dialog_rx {
            Some(rx) => rx.try_recv().ok(),
            None => None,
        };
        if let Some(selection) = result {
            self.file_dialogs.plugin_creator_dialog_rx = None;
            if let Some(parent) = selection {
                self.plugin_creator_last_path = Some(parent.clone());
                match self.create_plugin_from_draft(&parent) {
                    Ok(path) => {
                        self.status = format!("Plugin scaffold created at {}", path.display());
                        self.show_info("Plugin Creator", &self.status.clone());
                        self.windows.new_plugin_open = false;
                    }
                    Err(err) => {
                        self.status = err.clone();
                        self.show_info("Plugin Creator", &err);
                    }
                }
            } else {
                self.status = "Plugin creation cancelled".to_string();
            }
        }
    }

    /// Polls the export dialog for file destination and exports the workspace.
    ///
    /// This function monitors the export dialog's receiver channel for user file
    /// destination selection. When a destination is selected, it copies the
    /// workspace file to the chosen location.
    ///
    /// # Parameters
    /// The receiver provides a tuple of `(source, dest)` where:
    /// - `source`: The current workspace file path to copy from
    /// - `dest`: The user-selected destination path (optional)
    ///
    /// # Behavior
    /// - On destination selection: copies the workspace file and shows success notification
    /// - On cancellation: silently handles the cancellation without status updates
    /// - Uses `std::fs::copy` for the file operation, ignoring copy errors
    /// - Cleans up the receiver channel after processing
    pub(crate) fn poll_export_dialog(&mut self) {
        let result = match &self.file_dialogs.export_dialog_rx {
            Some(rx) => rx.try_recv().ok(),
            None => None,
        };
        if let Some((source, dest)) = result {
            self.file_dialogs.export_dialog_rx = None;
            if let Some(dest) = dest {
                let _ = std::fs::copy(source, dest);
                self.show_info("Workspace", "Workspace exported");
            }
        }
    }

    /// Polls the plotter screenshot dialog for file destination and exports plotter image.
    ///
    /// This function monitors the plotter screenshot dialog's receiver channel for
    /// user file destination selection. When a destination is selected, it exports
    /// the target plotter's current visualization as an image file using the
    /// configured preview settings.
    ///
    /// # Behavior
    /// - Retrieves the target plugin ID from `plotter_screenshot_target`
    /// - Gets plotter preview settings for the target plugin
    /// - Exports as SVG or PNG based on the `export_svg` setting
    /// - Uses high-quality rendering if `high_quality` setting is enabled
    /// - Applies all visualization settings: axes, legend, grid, colors, transforms, etc.
    /// - Shows error notification if export fails
    /// - Cleans up both the receiver channel and target plugin ID after processing
    ///
    /// # Export Formats
    /// - SVG: Vector format using `export_svg_with_settings`
    /// - PNG (high quality): High-resolution raster using `export_png_hq_with_settings`
    /// - PNG (standard): Standard resolution using `export_png_with_settings`
    pub(crate) fn poll_plotter_screenshot_dialog(&mut self) {
        let result = match &self.file_dialogs.plotter_screenshot_rx {
            Some(rx) => rx.try_recv().ok(),
            None => None,
        };
        if let Some(selection) = result {
            self.file_dialogs.plotter_screenshot_rx = None;
            let target = self.plotter_screenshot_target.take();
            if let (Some(path), Some(plugin_id)) = (selection, target) {
                let settings = self
                    .plotter_manager
                    .plotter_preview_settings
                    .get(&plugin_id)
                    .cloned();
                let export_result = self
                    .plotter_manager
                    .plotters
                    .get(&plugin_id)
                    .and_then(|plotter| plotter.lock().ok())
                    .and_then(|mut plotter| {
                        if let Some((
                            show_axes,
                            show_legend,
                            show_grid,
                            series_names,
                            series_scales,
                            series_offsets,
                            colors,
                            title,
                            dark_theme,
                            x_axis,
                            y_axis,
                            window_ms,
                            _timebase_divisions,
                            high_quality,
                            export_svg,
                        )) = settings
                        {
                            let series_transforms: Vec<crate::plotter::SeriesTransform> = (0
                                ..series_names.len())
                                .map(|i| crate::plotter::SeriesTransform {
                                    scale: *series_scales.get(i).unwrap_or(&1.0),
                                    offset: *series_offsets.get(i).unwrap_or(&0.0),
                                })
                                .collect();
                            if export_svg {
                                plotter
                                    .export_svg_with_settings(
                                        &path,
                                        &self.state_sync.logic_time_label,
                                        show_axes,
                                        show_legend,
                                        show_grid,
                                        &title,
                                        &series_names,
                                        &series_transforms,
                                        &colors,
                                        dark_theme,
                                        &x_axis,
                                        &y_axis,
                                        window_ms,
                                        self.plotter_preview.width,
                                        self.plotter_preview.height,
                                    )
                                    .err()
                            } else if high_quality {
                                plotter
                                    .export_png_hq_with_settings(
                                        &path,
                                        &self.state_sync.logic_time_label,
                                        show_axes,
                                        show_legend,
                                        show_grid,
                                        &title,
                                        &series_names,
                                        &series_transforms,
                                        &colors,
                                        dark_theme,
                                        &x_axis,
                                        &y_axis,
                                        window_ms,
                                    )
                                    .err()
                            } else {
                                plotter
                                    .export_png_with_settings(
                                        &path,
                                        &self.state_sync.logic_time_label,
                                        show_axes,
                                        show_legend,
                                        show_grid,
                                        &title,
                                        &series_names,
                                        &series_transforms,
                                        &colors,
                                        dark_theme,
                                        &x_axis,
                                        &y_axis,
                                        window_ms,
                                        self.plotter_preview.width,
                                        self.plotter_preview.height,
                                    )
                                    .err()
                            }
                        } else {
                            plotter
                                .export_png(&path, &self.state_sync.logic_time_label)
                                .err()
                        }
                    });
                if let Some(err) = export_result {
                    self.show_info("Plotter", &err);
                }
            }
        }
    }

    /// Initiates a plugin build operation with the specified action and label.
    ///
    /// This function starts an asynchronous plugin build process in a separate thread.
    /// It handles both installation of new plugins and reinstallation of existing ones,
    /// with special handling for plugins that don't require compilation.
    ///
    /// # Parameters
    /// - `action`: The build action to perform (Install or Reinstall)
    /// - `label`: Display label for the build progress dialog
    ///
    /// # Behavior
    /// - Prevents multiple concurrent builds by checking for existing receiver
    /// - For empty paths in Reinstall: performs immediate refresh without building
    /// - For non-Cargo projects: performs immediate installation/refresh without building
    /// - For Cargo projects: spawns background thread to compile and build
    /// - Updates build dialog UI with progress information
    /// - Uses `PluginManager::build_plugin` for the actual compilation process
    ///
    /// # Build Dialog Updates
    /// - Sets dialog as open and in progress
    /// - Updates title and message with build information
    /// - Creates receiver channel for build result communication
    pub(crate) fn start_plugin_build(&mut self, action: BuildAction, label: String) {
        if self.build_dialog.rx.is_some() {
            self.status = "Plugin build already running".to_string();
            return;
        }
        let path = match &action {
            BuildAction::Install { path, .. } => path.clone(),
            BuildAction::Reinstall { path, .. } => path.clone(),
        };

        if path.as_os_str().is_empty() {
            if let BuildAction::Reinstall { kind, path } = action {
                self.refresh_installed_plugin(kind, &path);
                self.show_info("Plugin", "Plugin refreshed");
            }
            return;
        }

        if !path.join("Cargo.toml").is_file() {
            match action {
                BuildAction::Install {
                    path,
                    removable,
                    persist,
                } => {
                    self.install_plugin_from_folder(path, removable, persist);
                    self.scan_detected_plugins();
                    return;
                }
                BuildAction::Reinstall { kind, path } => {
                    self.refresh_installed_plugin(kind, &path);
                    self.show_info("Plugin", "Plugin refreshed");
                    return;
                }
            }
        }
        let (tx, rx) = mpsc::channel();
        self.build_dialog.rx = Some(rx);
        self.build_dialog.open = true;
        self.build_dialog.in_progress = true;
        self.build_dialog.title = "Building plugin".to_string();
        self.build_dialog.message = format!("Building {label}");
        std::thread::spawn(move || {
            let success = PluginManager::build_plugin(&path);
            let _ = tx.send(BuildResult { success, action });
        });
    }

    /// Initiates a file dialog for saving a plotter screenshot.
    ///
    /// This function opens a file save dialog for exporting a plotter's visualization
    /// as an image file. It generates a default filename based on the plotter's title
    /// and current timestamp, and configures the dialog for the appropriate file format.
    ///
    /// # Parameters
    /// - `plugin_id`: The ID of the plugin whose plotter should be exported
    ///
    /// # Behavior
    /// - Prevents multiple concurrent screenshot dialogs
    /// - Generates default filename from plotter title or uses "live_plotter"
    /// - Creates timestamp-based filename with format: `{title}-{day}-{hour}-{minute}-{second}`
    /// - Determines file format (PNG/SVG) from plotter preview settings
    /// - Uses zenity dialog for RT-capable systems, rfd dialog otherwise
    /// - Stores the target plugin ID for later processing by `poll_plotter_screenshot_dialog`
    ///
    /// # Filename Generation
    /// - Sanitizes title by replacing spaces and slashes with underscores
    /// - Converts title to lowercase for consistency
    /// - Appends timestamp to ensure unique filenames
    /// - Uses appropriate file extension (.png or .svg)
    pub(crate) fn request_plotter_screenshot(&mut self, plugin_id: u64) {
        if self.file_dialogs.plotter_screenshot_rx.is_some() {
            return;
        }

        let base_name = self
            .plotter_manager
            .plotter_preview_settings
            .get(&plugin_id)
            .and_then(|(_, _, _, _, _, _, _, title, _, _, _, _, _, _, _)| {
                if title.trim().is_empty() {
                    None
                } else {
                    Some(
                        title
                            .trim()
                            .replace(' ', "_")
                            .replace('/', "_")
                            .to_lowercase(),
                    )
                }
            })
            .unwrap_or_else(|| "live_plotter".to_string());

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let day = now / 86_400;
        let hour = (now % 86_400) / 3_600;
        let minute = (now % 3_600) / 60;
        let second = now % 60;
        let default_name = format!("{}-{day}-{hour:02}-{minute:02}-{second:02}.png", base_name);

        let (tx, rx) = mpsc::channel();
        self.file_dialogs.plotter_screenshot_rx = Some(rx);
        self.plotter_screenshot_target = Some(plugin_id);

        let is_svg = self
            .plotter_manager
            .plotter_preview_settings
            .get(&plugin_id)
            .map(|(_, _, _, _, _, _, _, _, _, _, _, _, _, _, svg)| *svg)
            .unwrap_or(false);
        let extension = if is_svg { "svg" } else { "png" };
        let filter_name = if is_svg { "SVG" } else { "PNG" };

        spawn_file_dialog_thread(move || {
            let file = if crate::has_rt_capabilities() {
                crate::zenity_file_dialog("save", Some(&format!("*.{}", extension)))
            } else {
                rfd::FileDialog::new()
                    .add_filter(filter_name, &[extension])
                    .set_file_name(&default_name.replace(".png", &format!(".{}", extension)))
                    .save_file()
            };
            let _ = tx.send(file);
        });
    }
}
