use crate::{spawn_file_dialog_thread, BuildAction, BuildResult, GuiApp};
use rtsyn_core::plugin::PluginManager;
use rtsyn_runtime::LogicMessage;
use std::sync::mpsc;

impl GuiApp {
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
                            colors,
                            title,
                            dark_theme,
                            x_axis,
                            y_axis,
                            high_quality,
                            export_svg,
                        )) = settings
                        {
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
                                        &colors,
                                        dark_theme,
                                        &x_axis,
                                        &y_axis,
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
                                        &colors,
                                        dark_theme,
                                        &x_axis,
                                        &y_axis,
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
                                        &colors,
                                        dark_theme,
                                        &x_axis,
                                        &y_axis,
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
        self.build_dialog.message = format!("Building {label}...");
        std::thread::spawn(move || {
            let success = PluginManager::build_plugin(&path);
            let _ = tx.send(BuildResult { success, action });
        });
    }

    pub(crate) fn request_plotter_screenshot(&mut self, plugin_id: u64) {
        if self.file_dialogs.plotter_screenshot_rx.is_some() {
            return;
        }

        let base_name = self
            .plotter_manager
            .plotter_preview_settings
            .get(&plugin_id)
            .and_then(|(_, _, _, _, _, title, _, _, _, _, _)| {
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
            .map(|(_, _, _, _, _, _, _, _, _, _, svg)| *svg)
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
