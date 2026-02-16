//! State management types and structures for the RTSyn GUI.
//!
//! This module organizes all state-related types used throughout the application:
//! - `types`: Core enums and type definitions
//! - `ui`: UI window state structures
//! - `sync`: State synchronization helpers

pub mod sync;
pub mod types;
pub mod ui;

pub use sync::StateSync;
pub use types::*;
pub use ui::*;
