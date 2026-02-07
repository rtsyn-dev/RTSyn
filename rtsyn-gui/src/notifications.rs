use std::time::Instant;

#[derive(Debug, Clone)]
pub(crate) struct Notification {
    pub(crate) title: String,
    pub(crate) message: String,
    pub(crate) created_at: Instant,
}
