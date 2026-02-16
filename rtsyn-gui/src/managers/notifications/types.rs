use std::time::Instant;

#[derive(Debug, Clone)]
pub struct Notification {
    pub title: String,
    pub message: String,
    pub created_at: Instant,
}
