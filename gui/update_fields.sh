#!/bin/bash
# Script to update field accesses to use managers

cd "$(dirname "$0")"

# Backup
cp src/lib.rs src/lib.rs.backup

# Replace workspace accesses
sed -i 's/self\.workspace\./self.workspace_manager.workspace./g' src/lib.rs
sed -i 's/self\.workspace_path/self.workspace_manager.workspace_path/g' src/lib.rs
sed -i 's/self\.workspace_dirty/self.workspace_manager.workspace_dirty/g' src/lib.rs
sed -i 's/self\.workspace_entries/self.workspace_manager.workspace_entries/g' src/lib.rs

# Replace plugin manager accesses
sed -i 's/self\.installed_plugins/self.plugin_manager.installed_plugins/g' src/lib.rs
sed -i 's/self\.plugin_behaviors/self.plugin_manager.plugin_behaviors/g' src/lib.rs
sed -i 's/self\.detected_plugins/self.plugin_manager.detected_plugins/g' src/lib.rs
sed -i 's/self\.next_plugin_id/self.plugin_manager.next_plugin_id/g' src/lib.rs
sed -i 's/self\.available_plugin_ids/self.plugin_manager.available_plugin_ids/g' src/lib.rs

# Replace state sync accesses
sed -i 's/self\.logic_tx/self.state_sync.logic_tx/g' src/lib.rs
sed -i 's/self\.logic_state_rx/self.state_sync.logic_state_rx/g' src/lib.rs
sed -i 's/self\.computed_outputs/self.state_sync.computed_outputs/g' src/lib.rs
sed -i 's/self\.input_values/self.state_sync.input_values/g' src/lib.rs
sed -i 's/self\.internal_variable_values/self.state_sync.internal_variable_values/g' src/lib.rs
sed -i 's/self\.viewer_values/self.state_sync.viewer_values/g' src/lib.rs
sed -i 's/self\.last_output_update/self.state_sync.last_output_update/g' src/lib.rs
sed -i 's/self\.logic_period_seconds/self.state_sync.logic_period_seconds/g' src/lib.rs
sed -i 's/self\.logic_time_scale/self.state_sync.logic_time_scale/g' src/lib.rs
sed -i 's/self\.logic_time_label/self.state_sync.logic_time_label/g' src/lib.rs
sed -i 's/self\.logic_ui_hz/self.state_sync.logic_ui_hz/g' src/lib.rs

# Replace plotter manager accesses
sed -i 's/self\.plotters/self.plotter_manager.plotters/g' src/lib.rs
sed -i 's/self\.plotter_preview_settings/self.plotter_manager.plotter_preview_settings/g' src/lib.rs

# Replace file dialog accesses
sed -i 's/self\.install_dialog_rx/self.file_dialogs.install_dialog_rx/g' src/lib.rs
sed -i 's/self\.import_dialog_rx/self.file_dialogs.import_dialog_rx/g' src/lib.rs
sed -i 's/self\.load_dialog_rx/self.file_dialogs.load_dialog_rx/g' src/lib.rs
sed -i 's/self\.export_dialog_rx/self.file_dialogs.export_dialog_rx/g' src/lib.rs
sed -i 's/self\.csv_path_dialog_rx/self.file_dialogs.csv_path_dialog_rx/g' src/lib.rs
sed -i 's/self\.plotter_screenshot_rx/self.file_dialogs.plotter_screenshot_rx/g' src/lib.rs

echo "Replacements complete. Check src/lib.rs"
echo "Backup saved as src/lib.rs.backup"
