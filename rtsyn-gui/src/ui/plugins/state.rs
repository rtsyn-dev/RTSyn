use super::cards::PluginRenderContext;
use super::*;
use crate::HighlightMode;

impl GuiApp {
    pub(crate) fn render_state_view(&mut self, ctx: &egui::Context, panel_rect: egui::Rect) {
        self.render_state_view_filtered(ctx, panel_rect, None);
    }

    pub(crate) fn render_state_view_filtered(
        &mut self,
        ctx: &egui::Context,
        panel_rect: egui::Rect,
        only_connected: Option<bool>,
    ) {
        let render_ctx = self.create_plugin_render_context(panel_rect);
        let mut plugin_to_select: Option<u64> = None;

        // Collect plugin IDs and kinds first to avoid borrowing issues
        let plugin_data: Vec<_> = self
            .workspace_manager
            .workspace
            .plugins
            .iter()
            .filter(|p| {
                self.should_render_plugin(p, only_connected, &render_ctx.highlighted_plugins)
            })
            .map(|p| (p.id, p.kind.clone()))
            .collect();

        for (index, (plugin_id, plugin_kind)) in plugin_data.iter().enumerate() {
            let pos = self.calculate_plugin_position(index, &render_ctx, *plugin_id);
            let stroke = self.get_plugin_highlight_color(*plugin_id, &render_ctx);

            let response = egui::Area::new(egui::Id::new(("state_circle", plugin_id)))
                .order(egui::Order::Middle)
                .current_pos(pos + egui::vec2(-render_ctx.circle_radius, -render_ctx.circle_radius))
                .movable(true)
                .constrain_to(panel_rect)
                .show(ctx, |ui| {
                    let (rect, resp) = ui.allocate_exact_size(
                        egui::vec2(
                            render_ctx.circle_radius * 2.0,
                            render_ctx.circle_radius * 2.0,
                        ),
                        egui::Sense::click(),
                    );

                    self.render_plugin_circle_simple(
                        ui,
                        rect,
                        *plugin_id,
                        plugin_kind,
                        &render_ctx,
                        stroke,
                    );
                    self.plugin_rects.insert(*plugin_id, rect);
                    resp
                });

            // Keep state positions as node centers so drag state is stable frame-to-frame.
            let node_center = response.response.rect.min
                + egui::vec2(render_ctx.circle_radius, render_ctx.circle_radius);
            self.state_plugin_positions.insert(*plugin_id, node_center);

            if only_connected == Some(true) {
                ctx.move_to_top(response.response.layer_id);
            }

            if let Some(selected_id) = self.handle_plugin_interaction(
                ctx,
                &response,
                *plugin_id,
                &render_ctx.highlighted_plugins,
            ) {
                plugin_to_select = Some(selected_id);
            }
        }

        if let Some(id) = plugin_to_select {
            self.double_click_plugin(id);
        }
    }

    fn create_plugin_render_context(&mut self, panel_rect: egui::Rect) -> PluginRenderContext {
        let highlighted_plugins = self.get_highlighted_plugins();
        let has_connection_highlight = !matches!(self.highlight_mode, HighlightMode::None);
        let name_by_kind = self.get_name_by_kind();

        let (tab_primary, tab_secondary) = match self.connection_editor.tab {
            ConnectionEditTab::Inputs => (
                egui::Color32::from_rgb(255, 170, 80),
                egui::Color32::from_rgb(80, 200, 120),
            ),
            ConnectionEditTab::Outputs => (
                egui::Color32::from_rgb(80, 200, 120),
                egui::Color32::from_rgb(255, 170, 80),
            ),
        };

        PluginRenderContext {
            highlighted_plugins,
            has_connection_highlight,
            current_id: self.connection_editor.plugin_id,
            selected_id: self.connection_highlight_plugin_id,
            tab_primary,
            tab_secondary,
            name_by_kind,
            panel_rect,
            circle_radius: 30.0,
            spacing: 100.0,
        }
    }

    fn should_render_plugin(
        &self,
        plugin: &workspace::PluginDefinition,
        only_connected: Option<bool>,
        highlighted_plugins: &std::collections::HashSet<u64>,
    ) -> bool {
        let external = self
            .behavior_manager
            .cached_behaviors
            .get(&plugin.kind)
            .map(|b| b.external_window)
            .unwrap_or(false);

        if external {
            return false;
        }

        if let Some(only_conn) = only_connected {
            let is_connected = highlighted_plugins.contains(&plugin.id);
            only_conn == is_connected
        } else {
            true
        }
    }

    fn get_plugin_highlight_color(
        &self,
        plugin_id: u64,
        render_ctx: &PluginRenderContext,
    ) -> egui::Stroke {
        if render_ctx.current_id == Some(plugin_id) {
            egui::Stroke::new(3.0, render_ctx.tab_primary)
        } else if render_ctx.selected_id == Some(plugin_id) {
            egui::Stroke::new(3.0, render_ctx.tab_secondary)
        } else if render_ctx.has_connection_highlight
            && render_ctx.highlighted_plugins.contains(&plugin_id)
        {
            egui::Stroke::new(3.0, egui::Color32::from_rgb(100, 150, 255))
        } else {
            egui::Stroke::new(1.0, egui::Color32::from_gray(100))
        }
    }

    fn handle_plugin_interaction(
        &mut self,
        ctx: &egui::Context,
        response: &egui::InnerResponse<egui::Response>,
        plugin_id: u64,
        highlighted_plugins: &std::collections::HashSet<u64>,
    ) -> Option<u64> {
        if response.inner.secondary_clicked() {
            self.plugin_context_menu = Some((
                plugin_id,
                response.inner.interact_pointer_pos().unwrap_or_default(),
                ctx.frame_nr(),
            ));
        }

        if ctx.input(|i| {
            i.pointer
                .button_double_clicked(egui::PointerButton::Primary)
        }) {
            if response.inner.hovered() && !self.confirm_dialog.open {
                return if matches!(self.highlight_mode, HighlightMode::AllConnections(id) if id == plugin_id)
                {
                    self.highlight_mode = HighlightMode::None;
                    None
                } else {
                    Some(plugin_id)
                };
            }
        }

        if response.inner.clicked()
            && !self.confirm_dialog.open
            && !ctx.input(|i| {
                i.pointer
                    .button_double_clicked(egui::PointerButton::Primary)
            })
        {
            let is_highlighted = highlighted_plugins.contains(&plugin_id);
            if is_highlighted {
                if !matches!(self.highlight_mode, HighlightMode::AllConnections(id) if id == plugin_id)
                {
                    return Some(plugin_id);
                }
            } else {
                self.highlight_mode = HighlightMode::None;
            }
        }

        None
    }

    fn calculate_plugin_position(
        &self,
        index: usize,
        render_ctx: &PluginRenderContext,
        plugin_id: u64,
    ) -> egui::Pos2 {
        let cols = ((render_ctx.panel_rect.width() / render_ctx.spacing).floor() as usize).max(1);
        let col = index % cols;
        let row = index / cols;
        let default_pos = render_ctx.panel_rect.min
            + egui::vec2(
                50.0 + col as f32 * render_ctx.spacing,
                50.0 + row as f32 * render_ctx.spacing,
            );

        self.state_plugin_positions
            .get(&plugin_id)
            .copied()
            .unwrap_or(default_pos)
    }

    fn render_plugin_circle_simple(
        &self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        plugin_id: u64,
        plugin_kind: &str,
        render_ctx: &PluginRenderContext,
        stroke: egui::Stroke,
    ) {
        ui.painter().circle_filled(
            rect.center(),
            render_ctx.circle_radius,
            egui::Color32::from_gray(60),
        );
        ui.painter()
            .circle_stroke(rect.center(), render_ctx.circle_radius, stroke);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            plugin_id.to_string(),
            egui::FontId::proportional(14.0),
            egui::Color32::WHITE,
        );

        let name = render_ctx
            .name_by_kind
            .get(plugin_kind)
            .cloned()
            .unwrap_or_else(|| Self::display_kind(plugin_kind));
        ui.painter().text(
            rect.center() + egui::vec2(0.0, render_ctx.circle_radius + 10.0),
            egui::Align2::CENTER_TOP,
            name,
            egui::FontId::proportional(10.0),
            egui::Color32::from_gray(200),
        );
    }
}
