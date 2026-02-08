use super::*;
use crate::WindowFocus;
use rtsyn_runtime::LogicSettings;

impl GuiApp {
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
                self.workspace_dialog.description_input = self.workspace_manager.workspace.description.clone();
                self.workspace_dialog.edit_path = None;
            }
            WorkspaceDialogMode::Edit => {}
        }
        self.workspace_dialog.open = true;
        self.pending_window_focus = Some(WindowFocus::WorkspaceDialog);
    }

    pub(crate) fn open_manage_workspaces(&mut self) {
        self.windows.manage_workspace_open = true;
        self.windows.manage_workspace_selected_index = None;
        self.scan_workspaces();
        self.pending_window_focus = Some(WindowFocus::ManageWorkspaces);
    }

    pub(crate) fn open_load_workspaces(&mut self) {
        self.windows.load_workspace_open = true;
        self.windows.load_workspace_selected_index = None;
        self.scan_workspaces();
        self.pending_window_focus = Some(WindowFocus::LoadWorkspaces);
    }

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

    pub(crate) fn render_manage_workspaces_window(&mut self, ctx: &egui::Context) {
        if !self.windows.manage_workspace_open {
            return;
        }

        let mut open = self.windows.manage_workspace_open;
        let window_size = egui::vec2(360.0, 520.0);
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
                let left_w = (total_w * 0.52).max(180.0);
                let right_w = (total_w - left_w - 10.0).max(140.0);
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
                                style.visuals.widgets.inactive.bg_fill = egui::Color32::from_gray(50);
                                style.visuals.widgets.hovered.bg_fill = egui::Color32::from_gray(55);
                                style.visuals.widgets.active.bg_fill = egui::Color32::from_gray(60);
                                ui.set_style(style);
                                ui.add_sized(
                                    [160.0, 24.0],
                                    egui::TextEdit::singleline(&mut self.windows.manage_workspace_search)
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
                                            for (idx, entry) in self.workspace_manager.workspace_entries.iter().enumerate() {
                                                if !self.windows.manage_workspace_search.trim().is_empty()
                                                    && !entry
                                                        .name
                                                        .to_lowercase()
                                                        .contains(&self.windows.manage_workspace_search.to_lowercase())
                                                {
                                                    continue;
                                                }
                                                let label = entry.name.clone();
                                                let response = ui
                                                    .allocate_ui_with_layout(
                                                        egui::vec2(ui.available_width(), 22.0),
                                                        egui::Layout::left_to_right(egui::Align::Center),
                                                        |ui| {
                                                            ui.add(egui::SelectableLabel::new(
                                                                self.windows.manage_workspace_selected_index == Some(idx),
                                                                egui::RichText::new(label).size(14.0),
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
                                    if styled_button(ui, "Browse...").clicked() {
                                        self.open_import_dialog();
                                    }
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
                                    if let Some(idx) = self.windows.manage_workspace_selected_index {
                                        if let Some(entry) = self.workspace_manager.workspace_entries.get(idx) {
                                            ui.add_space(4.0);
                                            if !entry.description.is_empty() {
                                                ui.label(
                                                    egui::RichText::new(&entry.description)
                                                        .size(13.0)
                                                        .color(egui::Color32::from_gray(200)),
                                                );
                                                ui.add_space(8.0);
                                            }
                                            ui.horizontal(|ui| {
                                                ui.label(egui::RichText::new("Plugins:").strong());
                                                ui.label(entry.plugins.to_string());
                                            });
                                            if !entry.plugin_kinds.is_empty() {
                                                ui.add_space(4.0);
                                                ui.label(egui::RichText::new("Types:").strong());
                                                ui.label(
                                                    egui::RichText::new(entry.plugin_kinds.join(", "))
                                                        .size(12.0)
                                                    .color(egui::Color32::from_gray(180)),
                                                );
                                            }
                                            ui.add_space(12.0);
                                            ui.vertical_centered(|ui| {
                                                if styled_button(ui, "Load").clicked() {
                                                    action_load = Some(entry.path.clone());
                                                }
                                                if styled_button(ui, "Edit metadata").clicked() {
                                                    action_edit = Some(entry.path.clone());
                                                }
                                                if styled_button(ui, "Export").clicked() {
                                                    action_export = Some(entry.path.clone());
                                                }
                                                if styled_button(ui, "Delete").clicked() {
                                                    action_delete = Some(entry.path.clone());
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

    pub(crate) fn render_load_workspaces_window(&mut self, ctx: &egui::Context) {
        if !self.windows.load_workspace_open {
            return;
        }

        let mut open = self.windows.load_workspace_open;
        let window_size = egui::vec2(360.0, 520.0);
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
                let left_w = (total_w * 0.52).max(180.0);
                let right_w = (total_w - left_w - 10.0).max(140.0);
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
                                    style.visuals.widgets.inactive.bg_fill = egui::Color32::from_gray(50);
                                    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_gray(55);
                                    style.visuals.widgets.active.bg_fill = egui::Color32::from_gray(60);
                                    ui.set_style(style);
                                    ui.add_sized(
                                        [160.0, 24.0],
                                        egui::TextEdit::singleline(&mut self.windows.load_workspace_search)
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
                                            for (idx, entry) in self.workspace_manager.workspace_entries.iter().enumerate() {
                                                if !self.windows.load_workspace_search.trim().is_empty()
                                                    && !entry
                                                        .name
                                                        .to_lowercase()
                                                        .contains(&self.windows.load_workspace_search.to_lowercase())
                                                {
                                                    continue;
                                                }
                                                let label = entry.name.clone();
                                                let response = ui
                                                    .allocate_ui_with_layout(
                                                        egui::vec2(ui.available_width(), 22.0),
                                                        egui::Layout::left_to_right(egui::Align::Center),
                                                        |ui| {
                                                            ui.add(egui::SelectableLabel::new(
                                                                self.windows.load_workspace_selected_index == Some(idx),
                                                                egui::RichText::new(label).size(14.0),
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
                                if let Some(entry) = self.workspace_manager.workspace_entries.get(idx) {
                                    self.workspace_dialog.name_input = entry.name.clone();
                                    self.workspace_dialog.description_input = entry.description.clone();
                                }
                            }
                            ui.separator();
                            ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), footer_h),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    if styled_button(ui, "Browse...").clicked() {
                                        self.open_load_dialog();
                                    }
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
                                        if let Some(entry) = self.workspace_manager.workspace_entries.get(idx) {
                                            ui.add_space(4.0);
                                            if !entry.description.is_empty() {
                                                ui.label(egui::RichText::new(&entry.description).size(13.0).color(egui::Color32::from_gray(200)));
                                                ui.add_space(8.0);
                                            }
                                            ui.horizontal(|ui| {
                                                ui.label(egui::RichText::new("Plugins:").strong());
                                                ui.label(entry.plugins.to_string());
                                            });
                                            if !entry.plugin_kinds.is_empty() {
                                                ui.add_space(4.0);
                                                ui.label(egui::RichText::new("Types:").strong());
                                                ui.label(egui::RichText::new(entry.plugin_kinds.join(", ")).size(12.0).color(egui::Color32::from_gray(180)));
                                            }
                                            ui.add_space(12.0);
                                            ui.vertical_centered(|ui| {
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

    pub(crate) fn render_workspace_settings_window(&mut self, ctx: &egui::Context) {
        if !self.workspace_settings.open {
            return;
        }

        let mut open = self.workspace_settings.open;
        let window_size = egui::vec2(420.0, 240.0);
        let default_pos = Self::center_window(ctx, window_size);
        let mut draft = self
            .workspace_settings.draft
            .unwrap_or(WorkspaceSettingsDraft {
                frequency_value: self.frequency_value,
                frequency_unit: self.frequency_unit,
                period_value: self.period_value,
                period_unit: self.period_unit,
                tab: self.workspace_settings.tab,
                max_integration_steps: 10, // Default reasonable limit
            });
        let mut apply_clicked = false;
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

                    if draft.period_value < 1.0 {
                        draft.period_value = 1.0;
                        period_changed = true;
                    }
                    if draft.frequency_value < 1.0 {
                        draft.frequency_value = 1.0;
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
                            .clamp_range(1..=100)
                            .fixed_decimals(0),
                    );
                    ui.label("(per plugin per tick)");
                });
                ui.label("Lower values improve real-time performance but may reduce numerical accuracy.");

                ui.separator();
                if styled_button(ui, "Apply").clicked() {
                    apply_clicked = true;
                }
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

        if apply_clicked {
            self.frequency_value = draft.frequency_value;
            self.frequency_unit = draft.frequency_unit;
            self.period_value = draft.period_value;
            self.period_unit = draft.period_unit;
            self.workspace_settings.tab = draft.tab;
            
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
            .state_sync.logic_tx
                .send(LogicMessage::UpdateSettings(LogicSettings {
                    cores,
                    period_seconds,
                    time_scale,
                    time_label,
                    ui_hz: self.state_sync.logic_ui_hz,
                    max_integration_steps: draft.max_integration_steps,
                }));
            
            self.show_info("Runtime settings", "Sampling rate updated");
        }

        self.workspace_settings.open = open;
        if open {
            self.workspace_settings.draft = Some(draft);
        } else {
            self.workspace_settings.draft = None;
        }
    }

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

    pub(crate) fn render_info_dialog(&mut self, ctx: &egui::Context) {
        if self.notifications.is_empty() {
            return;
        }

        let now = Instant::now();
        let screen_rect = ctx.screen_rect();
        let max_width = 380.0;
        let mut y = screen_rect.min.y + 32.0;
        let x = screen_rect.max.x - 4.0;
        let total = 2.8;
        let mut idx = 0usize;
        for notification in &self.notifications {
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
        self.notifications
            .retain(|n| now.duration_since(n.created_at).as_secs_f32() < total);
        ctx.request_repaint_after(Duration::from_millis(16));
    }

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
