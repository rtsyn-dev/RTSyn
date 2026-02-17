//! Manager modules for the RTSyn GUI.
//!
//! This module contains various manager components that handle specific
//! aspects of the application state and behavior:
//! - `file_dialogs`: File dialog state management
//! - `notification_handler`: Notification system
//! - `plugin_behavior`: Plugin behavior caching
//! - `plotter`: Plotter state management
//! - `workspace`: Workspace operations

pub mod file_dialogs;
pub mod notifications;
pub mod plotter;
pub mod plugin_behavior;

pub use file_dialogs::FileDialogManager;
pub use notifications::NotificationHandler;
pub use plotter::PlotterManager;
pub use plugin_behavior::PluginBehaviorManager;
