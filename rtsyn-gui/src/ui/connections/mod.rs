//! Connection management UI components for the RTSyn GUI application.
//!
//! This module provides the user interface for managing audio connections between plugins
//! in the RTSyn workspace. It includes functionality for:
//!
//! - Opening and managing connection editors for adding/removing connections
//! - Rendering connection management windows with plugin selection and port configuration
//! - Visual connection display with interactive connection lines between plugins
//! - Context menus for connection operations
//! - Support for both fixed and extendable input/output ports
//!
//! The connection system supports different connection types (audio, MIDI, etc.) and
//! provides visual feedback for connection states, including highlighting and tooltips.

use super::*;

mod editor;
mod management;
mod view;
