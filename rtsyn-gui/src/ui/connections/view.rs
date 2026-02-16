//! Connection management UI components for the RTSyn GUI application.
//!
//! This module provides the user interface for managing audio connections between plugins
//! in the RTSyn workspace. It includes functionality for:
//!
//! - Opening and managing connection editors for adding/removing connections
//! - Rendering connection management windows with plugin selection and port configuration
//! - Visual connection display with interactive connection lines between plugins
//! - Context menus for connection operations
//! - Support for both fixed and extendable input/output ports
//!
//! The connection system supports different connection types (audio, MIDI, etc.) and
//! provides visual feedback for connection states, including highlighting and tooltips.

use super::*;

impl GuiApp {
/// Renders the visual connection lines between plugins in the workspace.
    ///
    /// This function draws interactive connection lines that visually represent the audio/MIDI
    /// connections between plugins. The lines are drawn as colored segments with directional
    /// arrows and provide hover tooltips with connection details.
    ///
    /// # Parameters
    /// - `ctx`: The egui context for rendering
    /// - `panel_rect`: The rectangle defining the panel area where connections should be drawn
    ///
    /// # Visual Features
    /// - **Colored Lines**: Output segments in green, input segments in orange
    /// - **Directional Arrows**: Show the direction of signal flow
    /// - **Bidirectional Support**: Handles connections in both directions between plugin pairs
    /// - **Selection Highlighting**: Emphasizes connections involving selected plugins
    /// - **Hover Tooltips**: Display detailed connection information on mouse hover
    /// - **Right-click Context**: Opens context menus for connection operations
    ///
    /// # Side Effects
    /// - Updates hover state and tooltip display
    /// - May trigger context menu creation on right-click
    /// - Performs hit-testing for interactive connection lines
    ///
    /// # Implementation Details
    /// The function implements several sophisticated features:
    ///
    /// **Connection Grouping**: Groups multiple connections between the same plugin pair
    /// to avoid visual clutter and provide cleaner line routing.
    ///
    /// **Bidirectional Rendering**: When plugins have connections in both directions,
    /// the lines are offset to prevent overlap and maintain visual clarity.
    ///
    /// **Interactive Hit-Testing**: Uses geometric distance calculations to determine
    /// which connection line the mouse is hovering over, with a configurable tolerance.
    ///
    /// **Visual States**:
    /// - Normal: Full opacity with standard stroke width
    /// - Selected: Highlighted when related plugins are selected
    /// - Dimmed: Reduced opacity when other plugins are selected
    ///
    /// **Performance Optimization**: Only renders connections for plugins visible within
    /// the panel rectangle to improve performance with large workspaces.
    ///
    /// **Tooltip Information**: Shows connection index, port names, and connection types
    /// with appropriate color coding for inputs/outputs.
    pub(crate) fn render_connection_view(&mut self, ctx: &egui::Context, panel_rect: egui::Rect) {
        if !self.connections_view_enabled {
            self.connection_context_menu = None;
            return;
        }

        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("connection_lines"),
        ));

        let mut groups: HashMap<(u64, u64), (Vec<String>, Vec<String>, Vec<usize>)> =
            HashMap::new();
        for (idx, connection) in self
            .workspace_manager
            .workspace
            .connections
            .iter()
            .enumerate()
        {
            let entry = groups
                .entry((connection.from_plugin, connection.to_plugin))
                .or_insert_with(|| (Vec::new(), Vec::new(), Vec::new()));
            entry.0.push(connection.from_port.clone());
            entry.1.push(connection.to_port.clone());
            entry.2.push(idx);
        }

        let mut keys: Vec<(u64, u64)> = groups.keys().copied().collect();
        keys.sort();

        let unique_ports = |ports: &[String]| -> Vec<String> {
            let set: HashSet<String> = ports.iter().cloned().collect();
            let mut list: Vec<String> = set.into_iter().collect();
            list.sort();
            list
        };

        let out_color = egui::Color32::from_rgb(80, 200, 120);
        let in_color = egui::Color32::from_rgb(255, 170, 80);
        let selected_plugin = if self.confirm_dialog.open {
            None
        } else {
            self.selected_plugin_id
        };
        let with_alpha = |color: egui::Color32, alpha: u8| {
            egui::Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), alpha)
        };

        let pointer_pos = ctx.input(|i| i.pointer.hover_pos());
        let pointer_over_plugin = pointer_pos
            .map(|pos| self.plugin_rects.values().any(|rect| rect.contains(pos)))
            .unwrap_or(false);
        let pointer_over_window = pointer_pos
            .map(|pos| self.window_rects.iter().any(|rect| rect.contains(pos)))
            .unwrap_or(false);

        let mut best_hover: Option<(f32, egui::Pos2, Vec<String>, Vec<String>, u64, u64, usize)> =
            None;
        for (from_id, to_id) in keys {
            let Some((from_ports, to_ports, conn_indices)) = groups.get(&(from_id, to_id)) else {
                continue;
            };
            let reverse_ports = groups.get(&(to_id, from_id));
            if reverse_ports.is_some() && from_id > to_id {
                continue;
            }
            let unique_outputs = unique_ports(from_ports);
            let unique_inputs = unique_ports(to_ports);
            let conn_index = conn_indices.iter().min().copied().unwrap_or(0);
            let conn_display_index = conn_index + 1;
            let Some(from_rect) = self.plugin_rects.get(&from_id) else {
                continue;
            };
            let Some(to_rect) = self.plugin_rects.get(&to_id) else {
                continue;
            };
            if !panel_rect.intersects(*from_rect) && !panel_rect.intersects(*to_rect) {
                continue;
            }

            let start = from_rect.center();
            let end = to_rect.center();
            let dir = (end - start).normalized();
            let perp = egui::vec2(-dir.y, dir.x);
            let offset = if reverse_ports.is_some() {
                perp * 6.0
            } else {
                egui::Vec2::ZERO
            };

            let is_selected = selected_plugin
                .map(|selected| selected == from_id || selected == to_id)
                .unwrap_or(false);
            let (out_line, in_line, stroke) = if let Some(_) = selected_plugin {
                if is_selected {
                    (out_color, in_color, 4.0)
                } else {
                    (with_alpha(out_color, 80), with_alpha(in_color, 80), 1.5)
                }
            } else {
                (out_color, in_color, 2.0)
            };

            let draw_line = |start: egui::Pos2,
                             end: egui::Pos2,
                             painter: &egui::Painter,
                             out_line: egui::Color32,
                             in_line: egui::Color32,
                             stroke: f32| {
                let mid = egui::pos2((start.x + end.x) * 0.5, (start.y + end.y) * 0.5);
                painter.line_segment([start, mid], (stroke, out_line));
                painter.line_segment([mid, end], (stroke, in_line));

                let dir = (end - start).normalized();
                let arrow_len = 8.0;
                let arrow_width = 5.0;
                let tip = mid + dir * arrow_len;
                let left = mid + egui::vec2(-dir.y, dir.x) * arrow_width;
                let right = mid + egui::vec2(dir.y, -dir.x) * arrow_width;
                painter.add(egui::Shape::convex_polygon(
                    vec![tip, left, right],
                    out_line,
                    egui::Stroke::NONE,
                ));
                mid
            };

            let mid_primary = draw_line(
                start + offset,
                end + offset,
                &painter,
                out_line,
                in_line,
                stroke,
            );
            let (mid_reverse, reverse_outputs, reverse_inputs, reverse_index) =
                if let Some((rev_out, rev_in, rev_indices)) = reverse_ports {
                    let mid = draw_line(
                        end - offset,
                        start - offset,
                        &painter,
                        out_line,
                        in_line,
                        stroke,
                    );
                    let rev_index = rev_indices.iter().min().copied().unwrap_or(0);
                    (
                        Some(mid),
                        unique_ports(rev_out),
                        unique_ports(rev_in),
                        rev_index,
                    )
                } else {
                    (None, Vec::new(), Vec::new(), 0)
                };

            if let Some(pointer) = pointer_pos {
                if pointer_over_plugin || pointer_over_window {
                    continue;
                }
                if self.confirm_dialog.open {
                    continue;
                }
                let hover_pad = 10.0;
                let dist_primary = distance_to_segment(pointer, start + offset, end + offset);
                if dist_primary <= hover_pad {
                    let replace = best_hover
                        .as_ref()
                        .map(|(dist, _, _, _, _, _, _)| dist_primary < *dist)
                        .unwrap_or(true);
                    if replace {
                        best_hover = Some((
                            dist_primary,
                            mid_primary,
                            unique_outputs.clone(),
                            unique_inputs.clone(),
                            from_id,
                            to_id,
                            conn_display_index,
                        ));
                    }
                }
                if let Some(mid) = mid_reverse {
                    let dist_reverse = distance_to_segment(pointer, end - offset, start - offset);
                    if dist_reverse <= hover_pad {
                        let replace = best_hover
                            .as_ref()
                            .map(|(dist, _, _, _, _, _, _)| dist_reverse < *dist)
                            .unwrap_or(true);
                        if replace {
                            best_hover = Some((
                                dist_reverse,
                                mid,
                                reverse_outputs.clone(),
                                reverse_inputs.clone(),
                                to_id,
                                from_id,
                                reverse_index + 1,
                            ));
                        }
                    }
                }
            }
        }
        if self.confirm_dialog.open {
            best_hover = None;
        }
        if let (Some(pointer), Some((_dist, _mid, outputs, inputs, from_id, to_id, conn_index))) =
            (pointer_pos, best_hover)
        {
            // Only show tooltip if pointer is not over any UI element
            if pointer_over_plugin
                || pointer_over_window
                || self.confirm_dialog.open
                || ctx.is_pointer_over_area()
            {
                // Still allow right-click menu
                if ctx.input(|i| i.pointer.secondary_clicked()) && !self.confirm_dialog.open {
                    let matched: Vec<ConnectionDefinition> = self
                        .workspace_manager
                        .workspace
                        .connections
                        .iter()
                        .filter(|conn| conn.from_plugin == from_id && conn.to_plugin == to_id)
                        .cloned()
                        .collect();
                    if !matched.is_empty() {
                        self.connection_context_menu = Some((matched, pointer, ctx.frame_nr()));
                    }
                }
                return;
            }
            if ctx.input(|i| i.pointer.secondary_clicked()) && !self.confirm_dialog.open {
                let matched: Vec<ConnectionDefinition> = self
                    .workspace_manager
                    .workspace
                    .connections
                    .iter()
                    .filter(|conn| conn.from_plugin == from_id && conn.to_plugin == to_id)
                    .cloned()
                    .collect();
                if !matched.is_empty() {
                    self.connection_context_menu = Some((matched, pointer, ctx.frame_nr()));
                }
            }
            let outputs_len = outputs.len();
            let inputs_len = inputs.len();
            let outputs = if outputs.is_empty() {
                "none".to_string()
            } else {
                outputs.join(", ")
            };
            let inputs = if inputs.is_empty() {
                "none".to_string()
            } else {
                inputs.join(", ")
            };
            let mut tooltip_pos = pointer + egui::vec2(12.0, 12.0);
            if tooltip_pos.y < panel_rect.min.y + 6.0 {
                tooltip_pos.y = panel_rect.min.y + 6.0;
            }
            egui::Area::new(egui::Id::new(("conn_hover", from_id, to_id)))
                .order(egui::Order::Middle)
                .fixed_pos(tooltip_pos)
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_min_width(0.0);
                        ui.set_max_width(180.0);
                        ui.horizontal(|ui| {
                            let (id_rect, _) = ui
                                .allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                            ui.painter().circle_filled(
                                id_rect.center(),
                                8.0,
                                egui::Color32::from_gray(60),
                            );
                            ui.painter().text(
                                id_rect.center(),
                                egui::Align2::CENTER_CENTER,
                                conn_index.to_string(),
                                egui::FontId::proportional(11.0),
                                ui.visuals().text_color(),
                            );
                            ui.label(RichText::new("Connection").strong());
                        });
                        ui.separator();
                        let input_label = if inputs_len == 1 { "Input:" } else { "Inputs:" };
                        let output_label = if outputs_len == 1 {
                            "Output:"
                        } else {
                            "Outputs:"
                        };
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(output_label).color(out_color));
                            ui.label(outputs);
                        });
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(input_label).color(in_color));
                            ui.label(inputs);
                        });
                    });
                });
        }
    }

    /// Renders the context menu for connection operations.
    ///
    /// This function displays a context menu that appears when right-clicking on connection
    /// lines in the connection view. It provides quick access to connection management
    /// operations without opening the full connection editor.
    ///
    /// # Parameters
    /// - `ctx`: The egui context for rendering UI elements
    ///
    /// # Menu Features
    /// - **Remove Connection**: Allows quick removal of the clicked connection(s)
    /// - **Smart Cleanup**: Handles both direct connection removal and extendable input cleanup
    /// - **Batch Operations**: Can remove multiple connections between the same plugin pair
    ///
    /// # Side Effects
    /// - May remove connections from the workspace
    /// - May remove extendable input ports when appropriate
    /// - Updates workspace dirty state when changes are made
    /// - Calls `enforce_connection_dependent` to maintain consistency
    /// - Closes the menu after operations or when clicking elsewhere
    ///
    /// # Implementation Details
    /// The context menu handles two types of connection removal:
    ///
    /// **Direct Removal**: For standard connections, removes the connection definition
    /// directly from the workspace connections list.
    ///
    /// **Extendable Input Removal**: For plugins with extendable inputs, identifies
    /// the input port index and removes the entire input port, which automatically
    /// removes all connections to that port.
    ///
    /// **Menu Lifecycle**:
    /// - Opens when `connection_context_menu` is set with connection data and position
    /// - Remains open until user clicks an action or clicks outside the menu
    /// - Automatically closes when confirm dialogs are open to prevent conflicts
    /// - Tracks the frame when opened to prevent immediate closure from the same click
    ///
    /// **Batch Processing**: When multiple connections exist between the same plugin pair,
    /// the menu operation affects all connections in the group, with special handling
    /// to sort extendable input indices in reverse order to prevent index shifting issues.
    pub(crate) fn render_connection_context_menu(&mut self, ctx: &egui::Context) {
        if !self.connections_view_enabled {
            self.connection_context_menu = None;
            return;
        }
        let Some((connections, pos, opened_frame)) = self.connection_context_menu.clone() else {
            return;
        };

        let mut close_menu = false;
        let menu_response = egui::Area::new(egui::Id::new("connection_context_menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    let row_height = ui.text_style_height(&egui::TextStyle::Button) + 6.0;
                    let menu_width = 160.0;
                    let remove_clicked = ui
                        .allocate_ui_with_layout(
                            egui::vec2(menu_width, row_height),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.add(egui::SelectableLabel::new(false, "Remove connection"))
                                    .clicked()
                            },
                        )
                        .inner;
                    if remove_clicked {
                        let mut remove_direct: Vec<ConnectionDefinition> = Vec::new();
                        let mut remove_inputs: HashMap<u64, Vec<usize>> = HashMap::new();
                        for conn in &connections {
                            let extendable = self
                                .workspace_manager
                                .workspace
                                .plugins
                                .iter()
                                .find(|p| p.id == conn.to_plugin)
                                .map(|p| self.is_extendable_inputs(&p.kind))
                                .unwrap_or(false);
                            if extendable {
                                if let Some(idx) = Self::extendable_input_index(&conn.to_port) {
                                    remove_inputs.entry(conn.to_plugin).or_default().push(idx);
                                    continue;
                                }
                            }
                            remove_direct.push(conn.clone());
                        }

                        for (plugin_id, mut inputs) in remove_inputs {
                            inputs.sort_unstable_by(|a, b| b.cmp(a));
                            inputs.dedup();
                            for idx in inputs {
                                self.remove_extendable_input_at(plugin_id, idx);
                            }
                        }

                        if !remove_direct.is_empty() {
                            let matches =
                                |left: &ConnectionDefinition, right: &ConnectionDefinition| {
                                    left.from_plugin == right.from_plugin
                                        && left.to_plugin == right.to_plugin
                                        && left.from_port == right.from_port
                                        && left.to_port == right.to_port
                                        && left.kind == right.kind
                                };
                            self.workspace_manager.workspace.connections.retain(|conn| {
                                !remove_direct.iter().any(|remove| matches(conn, remove))
                            });
                            self.workspace_manager.workspace_dirty = true;
                            self.enforce_connection_dependent();
                        }
                        close_menu = true;
                    }
                });
            });

        let pointer_pos = ctx.input(|i| i.pointer.interact_pos());
        let hovered = pointer_pos
            .map(|pos| menu_response.response.rect.contains(pos))
            .unwrap_or(false);
        let close_click = ctx.input(|i| {
            i.pointer.primary_clicked() || i.pointer.primary_down() || i.pointer.secondary_clicked()
        });
        if close_click && !hovered && ctx.frame_nr() != opened_frame {
            close_menu = true;
        }

        if close_menu || self.confirm_dialog.open {
            self.connection_context_menu = None;
        }
    }
}
