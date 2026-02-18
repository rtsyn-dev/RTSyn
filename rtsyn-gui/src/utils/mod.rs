//! Utility functions for the RTSyn GUI.
//!
//! This module provides various helper functions organized by category:
//! - `numeric`: Number formatting, parsing, and validation
//! - `strings`: String manipulation and truncation

pub mod numeric;
pub mod strings;
pub mod system;

pub use system::{
    has_rt_capabilities, spawn_file_dialog_thread, zenity_file_dialog,
    zenity_file_dialog_with_name, zenity_folder_dialog_multi,
};

pub use numeric::{
    distance_to_segment, format_f64_6, format_f64_with_input, normalize_numeric_input,
    parse_f64_input, truncate_f64,
};
pub use strings::truncate_string;
