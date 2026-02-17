#![allow(dead_code)]
use super::types::Notification;
use std::collections::HashMap;
use std::time::Instant;

/// Manages application and plugin-specific notifications.
///
/// This handler provides centralized notification management for both global
/// application notifications and plugin-specific notifications. It supports
/// automatic cleanup of old notifications and context-aware notification routing.
pub struct NotificationHandler {
    /// Global application notifications
    notifications: Vec<Notification>,
    /// Plugin-specific notifications indexed by plugin ID
    plugin_notifications: HashMap<u64, Vec<Notification>>,
    /// Currently active plugin ID for context-aware notification routing
    active_notification_plugin_id: Option<u64>,
}

impl NotificationHandler {
    /// Creates a new NotificationHandler with empty notification collections.
    ///
    /// # Returns
    /// A new instance with initialized but empty notification storage.
    pub fn new() -> Self {
        Self {
            notifications: Vec::new(),
            plugin_notifications: HashMap::new(),
            active_notification_plugin_id: None,
        }
    }

    /// Sets the active plugin context for notification routing.
    ///
    /// This function establishes which plugin should receive notifications when
    /// using context-aware notification methods. When an active plugin is set,
    /// calls to `show_info` will automatically route to plugin-specific notifications.
    ///
    /// # Parameters
    /// - `plugin_id`: Optional plugin ID to set as active, or None to clear context
    ///
    /// # Behavior
    /// - Setting to Some(id): Routes `show_info` calls to plugin-specific notifications
    /// - Setting to None: Routes `show_info` calls to global application notifications
    pub fn set_active_plugin(&mut self, plugin_id: Option<u64>) {
        self.active_notification_plugin_id = plugin_id;
    }

    /// Shows an informational notification with context-aware routing.
    ///
    /// This function creates and displays a notification, automatically routing it
    /// to either global or plugin-specific storage based on the current active plugin context.
    ///
    /// # Parameters
    /// - `title`: The notification title/header
    /// - `message`: The detailed notification message
    ///
    /// # Behavior
    /// - If an active plugin is set: routes to plugin-specific notifications
    /// - If no active plugin: routes to global application notifications
    /// - Automatically timestamps the notification with current instant
    pub fn show_info(&mut self, title: &str, message: &str) {
        if let Some(plugin_id) = self.active_notification_plugin_id {
            self.show_plugin_info(plugin_id, title, message);
        } else {
            let notification = Notification {
                title: title.to_string(),
                message: message.to_string(),
                created_at: Instant::now(),
            };
            self.notifications.push(notification);
        }
    }

    /// Shows an informational notification for a specific plugin.
    ///
    /// This function creates and displays a notification that is explicitly
    /// associated with a particular plugin, regardless of the current active plugin context.
    ///
    /// # Parameters
    /// - `plugin_id`: The ID of the plugin to associate the notification with
    /// - `title`: The notification title/header
    /// - `message`: The detailed notification message
    ///
    /// # Behavior
    /// - Always routes to the specified plugin's notification collection
    /// - Creates the plugin's notification vector if it doesn't exist
    /// - Automatically timestamps the notification with current instant
    pub fn show_plugin_info(&mut self, plugin_id: u64, title: &str, message: &str) {
        let notification = Notification {
            title: title.to_string(),
            message: message.to_string(),
            created_at: Instant::now(),
        };
        self.plugin_notifications
            .entry(plugin_id)
            .or_default()
            .push(notification);
    }

    /// Retrieves the most recent global notifications for display.
    ///
    /// This function returns a limited set of the most recent global application
    /// notifications in reverse chronological order (newest first).
    ///
    /// # Returns
    /// A vector of references to the 5 most recent global notifications.
    ///
    /// # Behavior
    /// - Returns notifications in reverse chronological order (newest first)
    /// - Limits results to maximum of 5 notifications
    /// - Only includes global notifications, not plugin-specific ones
    pub fn get_recent_notifications(&self) -> Vec<&Notification> {
        self.notifications.iter().rev().take(5).collect()
    }

    /// Retrieves all notifications for a specific plugin.
    ///
    /// This function returns all notifications associated with a particular plugin,
    /// or None if the plugin has no notifications.
    ///
    /// # Parameters
    /// - `plugin_id`: The ID of the plugin whose notifications to retrieve
    ///
    /// # Returns
    /// - `Some(&Vec<Notification>)`: Reference to the plugin's notification vector
    /// - `None`: If the plugin has no notifications or doesn't exist
    pub fn get_plugin_notifications(&self, plugin_id: u64) -> Option<&Vec<Notification>> {
        self.plugin_notifications.get(&plugin_id)
    }

    /// Retrieves all global application notifications.
    ///
    /// This function provides access to the complete collection of global
    /// application notifications for comprehensive display or processing.
    ///
    /// # Returns
    /// A reference to the vector containing all global notifications.
    ///
    /// # Note
    /// This does not include plugin-specific notifications. Use
    /// `get_all_plugin_notifications` for plugin-specific notifications.
    pub fn get_all_notifications(&self) -> &Vec<Notification> {
        &self.notifications
    }

    /// Retrieves all plugin-specific notification collections.
    ///
    /// This function provides access to the complete mapping of plugin IDs to
    /// their respective notification collections for comprehensive management.
    ///
    /// # Returns
    /// A reference to the HashMap containing all plugin notification collections,
    /// indexed by plugin ID.
    ///
    /// # Note
    /// This does not include global application notifications. Use
    /// `get_all_notifications` for global notifications.
    pub fn get_all_plugin_notifications(&self) -> &HashMap<u64, Vec<Notification>> {
        &self.plugin_notifications
    }

    /// Removes notifications older than the specified age threshold.
    ///
    /// This function performs automatic cleanup of both global and plugin-specific
    /// notifications that exceed the specified age limit, helping to prevent
    /// unbounded memory growth from accumulated notifications.
    ///
    /// # Parameters
    /// - `max_age_secs`: Maximum age in seconds for notifications to be retained
    ///
    /// # Behavior
    /// - Removes notifications from both global and plugin-specific collections
    /// - Uses the notification's `created_at` timestamp for age calculation
    /// - Preserves the structure of plugin notification collections (empty vectors remain)
    /// - Operates on all plugin collections simultaneously
    pub fn cleanup_old_notifications(&mut self, max_age_secs: f32) {
        let now = Instant::now();
        self.notifications
            .retain(|n| now.duration_since(n.created_at).as_secs_f32() < max_age_secs);

        for notifications in self.plugin_notifications.values_mut() {
            notifications.retain(|n| now.duration_since(n.created_at).as_secs_f32() < max_age_secs);
        }
    }
}
