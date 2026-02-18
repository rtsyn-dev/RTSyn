use std::path::PathBuf;
use std::sync::mpsc::Receiver;

pub struct FileDialogManager {
    pub install_dialog_rx: Option<Receiver<Option<Vec<PathBuf>>>>,
    pub import_dialog_rx: Option<Receiver<Option<PathBuf>>>,
    pub load_dialog_rx: Option<Receiver<Option<PathBuf>>>,
    pub export_dialog_rx: Option<Receiver<(PathBuf, Option<PathBuf>)>>,
    pub csv_path_dialog_rx: Option<Receiver<Option<PathBuf>>>,
    pub plugin_creator_dialog_rx: Option<Receiver<Option<PathBuf>>>,
    pub plotter_screenshot_rx: Option<Receiver<Option<PathBuf>>>,
}

impl FileDialogManager {
    pub fn new() -> Self {
        Self {
            install_dialog_rx: None,
            import_dialog_rx: None,
            load_dialog_rx: None,
            export_dialog_rx: None,
            csv_path_dialog_rx: None,
            plugin_creator_dialog_rx: None,
            plotter_screenshot_rx: None,
        }
    }
}

impl Default for FileDialogManager {
    fn default() -> Self {
        Self::new()
    }
}
