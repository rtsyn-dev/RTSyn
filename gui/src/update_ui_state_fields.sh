#!/bin/bash

# Plotter preview fields
sed -i 's/self\.plotter_preview_open/self.plotter_preview.open/g' lib.rs ui/*.rs
sed -i 's/self\.plotter_preview_target/self.plotter_preview.target/g' lib.rs ui/*.rs
sed -i 's/self\.plotter_preview_show_axes/self.plotter_preview.show_axes/g' lib.rs ui/*.rs
sed -i 's/self\.plotter_preview_show_legend/self.plotter_preview.show_legend/g' lib.rs ui/*.rs
sed -i 's/self\.plotter_preview_show_grid/self.plotter_preview.show_grid/g' lib.rs ui/*.rs
sed -i 's/self\.plotter_preview_series_names/self.plotter_preview.series_names/g' lib.rs ui/*.rs
sed -i 's/self\.plotter_preview_colors/self.plotter_preview.colors/g' lib.rs ui/*.rs
sed -i 's/self\.plotter_preview_title/self.plotter_preview.title/g' lib.rs ui/*.rs
sed -i 's/self\.plotter_preview_dark_theme/self.plotter_preview.dark_theme/g' lib.rs ui/*.rs
sed -i 's/self\.plotter_preview_x_axis_name/self.plotter_preview.x_axis_name/g' lib.rs ui/*.rs
sed -i 's/self\.plotter_preview_y_axis_name/self.plotter_preview.y_axis_name/g' lib.rs ui/*.rs
sed -i 's/self\.plotter_preview_high_quality/self.plotter_preview.high_quality/g' lib.rs ui/*.rs
sed -i 's/self\.plotter_preview_export_svg/self.plotter_preview.export_svg/g' lib.rs ui/*.rs
sed -i 's/self\.plotter_preview_width/self.plotter_preview.width/g' lib.rs ui/*.rs
sed -i 's/self\.plotter_preview_height/self.plotter_preview.height/g' lib.rs ui/*.rs

# Connection editor fields
sed -i 's/self\.connection_edit_open/self.connection_editor.open/g' lib.rs ui/*.rs
sed -i 's/self\.connection_edit_mode/self.connection_editor.mode/g' lib.rs ui/*.rs
sed -i 's/self\.connection_edit_tab/self.connection_editor.tab/g' lib.rs ui/*.rs
sed -i 's/self\.connection_edit_plugin_id/self.connection_editor.plugin_id/g' lib.rs ui/*.rs
sed -i 's/self\.connection_edit_selected_idx/self.connection_editor.selected_idx/g' lib.rs ui/*.rs
sed -i 's/self\.connection_edit_from_port_idx/self.connection_editor.from_port_idx/g' lib.rs ui/*.rs
sed -i 's/self\.connection_edit_to_port_idx/self.connection_editor.to_port_idx/g' lib.rs ui/*.rs
sed -i 's/self\.connection_edit_last_selected/self.connection_editor.last_selected/g' lib.rs ui/*.rs
sed -i 's/self\.connection_edit_last_tab/self.connection_editor.last_tab/g' lib.rs ui/*.rs
sed -i 's/self\.connection_from_idx/self.connection_editor.from_idx/g' lib.rs ui/*.rs
sed -i 's/self\.connection_to_idx/self.connection_editor.to_idx/g' lib.rs ui/*.rs
sed -i 's/self\.connection_from_port/self.connection_editor.from_port/g' lib.rs ui/*.rs
sed -i 's/self\.connection_to_port/self.connection_editor.to_port/g' lib.rs ui/*.rs
sed -i 's/self\.connection_kind/self.connection_editor.kind/g' lib.rs ui/*.rs
sed -i 's/self\.connection_kind_options/self.connection_editor.kind_options/g' lib.rs ui/*.rs

# Workspace dialog fields
sed -i 's/self\.workspace_dialog_open/self.workspace_dialog.open/g' lib.rs ui/*.rs
sed -i 's/self\.workspace_dialog_mode/self.workspace_dialog.mode/g' lib.rs ui/*.rs
sed -i 's/self\.workspace_name_input/self.workspace_dialog.name_input/g' lib.rs ui/*.rs
sed -i 's/self\.workspace_description_input/self.workspace_dialog.description_input/g' lib.rs ui/*.rs
sed -i 's/self\.workspace_edit_path/self.workspace_dialog.edit_path/g' lib.rs ui/*.rs

# Build dialog fields
sed -i 's/self\.build_dialog_open/self.build_dialog.open/g' lib.rs ui/*.rs
sed -i 's/self\.build_dialog_in_progress/self.build_dialog.in_progress/g' lib.rs ui/*.rs
sed -i 's/self\.build_dialog_message/self.build_dialog.message/g' lib.rs ui/*.rs
sed -i 's/self\.build_dialog_title/self.build_dialog.title/g' lib.rs ui/*.rs
sed -i 's/self\.build_dialog_rx/self.build_dialog.rx/g' lib.rs ui/*.rs

# Confirm dialog fields
sed -i 's/self\.confirm_dialog_open/self.confirm_dialog.open/g' lib.rs ui/*.rs
sed -i 's/self\.confirm_dialog_title/self.confirm_dialog.title/g' lib.rs ui/*.rs
sed -i 's/self\.confirm_dialog_message/self.confirm_dialog.message/g' lib.rs ui/*.rs
sed -i 's/self\.confirm_dialog_action_label/self.confirm_dialog.action_label/g' lib.rs ui/*.rs
sed -i 's/self\.confirm_action/self.confirm_dialog.action/g' lib.rs ui/*.rs

# Workspace settings fields
sed -i 's/self\.workspace_settings_open/self.workspace_settings.open/g' lib.rs ui/*.rs
sed -i 's/self\.workspace_settings_draft/self.workspace_settings.draft/g' lib.rs ui/*.rs
sed -i 's/self\.workspace_settings_tab/self.workspace_settings.tab/g' lib.rs ui/*.rs

# Window state fields
sed -i 's/self\.manage_workspace_open/self.windows.manage_workspace_open/g' lib.rs ui/*.rs
sed -i 's/self\.load_workspace_open/self.windows.load_workspace_open/g' lib.rs ui/*.rs
sed -i 's/self\.manage_workspace_selected_index/self.windows.manage_workspace_selected_index/g' lib.rs ui/*.rs
sed -i 's/self\.load_workspace_selected_index/self.windows.load_workspace_selected_index/g' lib.rs ui/*.rs
sed -i 's/self\.manage_plugins_open/self.windows.manage_plugins_open/g' lib.rs ui/*.rs
sed -i 's/self\.manage_plugins_tab/self.windows.manage_plugins_tab/g' lib.rs ui/*.rs
sed -i 's/self\.install_search/self.windows.install_search/g' lib.rs ui/*.rs
sed -i 's/self\.manage_selected_index/self.windows.manage_selected_index/g' lib.rs ui/*.rs
sed -i 's/self\.plugins_open/self.windows.plugins_open/g' lib.rs ui/*.rs
sed -i 's/self\.plugin_tab/self.windows.plugin_tab/g' lib.rs ui/*.rs
sed -i 's/self\.plugin_search/self.windows.plugin_search/g' lib.rs ui/*.rs
sed -i 's/self\.plugin_selected_index/self.windows.plugin_selected_index/g' lib.rs ui/*.rs
sed -i 's/self\.organize_search/self.windows.organize_search/g' lib.rs ui/*.rs
sed -i 's/self\.organize_selected_index/self.windows.organize_selected_index/g' lib.rs ui/*.rs
sed -i 's/self\.manage_connections_open/self.windows.manage_connections_open/g' lib.rs ui/*.rs
sed -i 's/self\.plugin_config_open/self.windows.plugin_config_open/g' lib.rs ui/*.rs
sed -i 's/self\.plugin_config_id/self.windows.plugin_config_id/g' lib.rs ui/*.rs

echo "Field updates complete"
