//! Plotter UI components and window management for RTSyn GUI.
//!
//! This module provides the user interface components for managing and displaying
//! real-time plotting windows in the RTSyn application. It handles:
//!
//! - Plotter window rendering and viewport management
//! - Interactive controls for plot customization (knobs, wheels, timebase)
//! - Series data visualization with scaling and offset controls
//! - Export functionality for plot images (PNG/SVG)
//! - Settings dialogs for plot appearance and behavior
//! - Connection management for plotter plugins
//! - Notification display for plotter-specific messages
//!
//! The module implements a comprehensive plotting interface that allows users to
//! visualize real-time data streams from various plugins, customize appearance,
//! and export plots for documentation or analysis purposes.

use super::*;

mod controls;
mod preview;
mod state;
mod windows;
