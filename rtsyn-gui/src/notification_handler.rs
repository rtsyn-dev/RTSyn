#![allow(dead_code)]
use crate::notifications::Notification;
use std::collections::HashMap;
use std::time::Instant;

pub struct NotificationHandler {
    notifications: Vec<Notification>,
    plugin_notifications: HashMap<u64, Vec<Notification>>,
    active_notification_plugin_id: Option<u64>,
}

impl NotificationHandler {
    pub fn new() -> Self {
        Self {
            notifications: Vec::new(),
            plugin_notifications: HashMap::new(),
            active_notification_plugin_id: None,
        }
    }

    pub fn set_active_plugin(&mut self, plugin_id: Option<u64>) {
        self.active_notification_plugin_id = plugin_id;
    }

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

    pub fn get_recent_notifications(&self) -> Vec<&Notification> {
        self.notifications.iter().rev().take(5).collect()
    }

    pub fn get_plugin_notifications(&self, plugin_id: u64) -> Option<&Vec<Notification>> {
        self.plugin_notifications.get(&plugin_id)
    }

    pub fn get_all_notifications(&self) -> &Vec<Notification> {
        &self.notifications
    }

    pub fn get_all_plugin_notifications(&self) -> &HashMap<u64, Vec<Notification>> {
        &self.plugin_notifications
    }

    pub fn cleanup_old_notifications(&mut self, max_age_secs: f32) {
        let now = Instant::now();
        self.notifications.retain(|n| now.duration_since(n.created_at).as_secs_f32() < max_age_secs);
        
        for notifications in self.plugin_notifications.values_mut() {
            notifications.retain(|n| now.duration_since(n.created_at).as_secs_f32() < max_age_secs);
        }
    }
}