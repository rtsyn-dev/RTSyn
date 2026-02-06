#!/bin/bash
# Script to update UI modules

cd "$(dirname "$0")"

for file in src/ui/*.rs; do
    echo "Updating $file..."
    cp "$file" "$file.backup"
    
    # Replace workspace accesses
    sed -i 's/self\.workspace\./self.workspace_manager.workspace./g' "$file"
    sed -i 's/self\.workspace_path/self.workspace_manager.workspace_path/g' "$file"
    sed -i 's/self\.workspace_dirty/self.workspace_manager.workspace_dirty/g' "$file"
    sed -i 's/self\.workspace_entries/self.workspace_manager.workspace_entries/g' "$file"
    
    # Replace plugin manager accesses
    sed -i 's/self\.installed_plugins/self.plugin_manager.installed_plugins/g' "$file"
    sed -i 's/self\.plugin_behaviors/self.plugin_manager.plugin_behaviors/g' "$file"
    sed -i 's/self\.detected_plugins/self.plugin_manager.detected_plugins/g' "$file"
    
    # Replace state sync accesses
    sed -i 's/self\.logic_tx/self.state_sync.logic_tx/g' "$file"
    sed -i 's/self\.computed_outputs/self.state_sync.computed_outputs/g' "$file"
    sed -i 's/self\.input_values/self.state_sync.input_values/g' "$file"
    sed -i 's/self\.internal_variable_values/self.state_sync.internal_variable_values/g' "$file"
    sed -i 's/self\.viewer_values/self.state_sync.viewer_values/g' "$file"
    
    # Replace plotter manager accesses
    sed -i 's/self\.plotters/self.plotter_manager.plotters/g' "$file"
    
    # Replace file dialog accesses
    sed -i 's/self\.install_dialog_rx/self.file_dialogs.install_dialog_rx/g' "$file"
    sed -i 's/self\.import_dialog_rx/self.file_dialogs.import_dialog_rx/g' "$file"
    sed -i 's/self\.load_dialog_rx/self.file_dialogs.load_dialog_rx/g' "$file"
    sed -i 's/self\.export_dialog_rx/self.file_dialogs.export_dialog_rx/g' "$file"
    sed -i 's/self\.csv_path_dialog_rx/self.file_dialogs.csv_path_dialog_rx/g' "$file"
    sed -i 's/self\.plotter_screenshot_rx/self.file_dialogs.plotter_screenshot_rx/g' "$file"
done

echo "UI modules updated. Backups saved as *.backup"
