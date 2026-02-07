use eframe::egui;
use eframe::egui::RichText;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use crate::state::*;
use crate::utils::{distance_to_segment, format_f64_6, truncate_f64};
use crate::{WorkspaceSettingsDraft, GuiApp};
use rtsyn_runtime::runtime::LogicMessage;
use std::time::{Duration, Instant};
use workspace::{prune_extendable_inputs_plugin_connections, ConnectionDefinition};

mod connections;
mod plotters;
mod plugins;
mod workspaces;
