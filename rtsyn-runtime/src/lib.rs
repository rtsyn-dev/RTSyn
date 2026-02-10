use rtsyn_plugin::{Plugin, PluginContext, PluginError};
use workspace::WorkspaceDefinition;

mod rt_thread;

#[derive(Debug, Clone, Copy)]
pub struct PluginSchedule {
    pub priority: u8,
    pub estimated_cost: u32,
}

pub struct PluginHandle {
    pub schedule: PluginSchedule,
    pub plugin: Box<dyn Plugin>,
}

#[derive(thiserror::Error, Debug)]
pub enum RuntimeError {
    #[error("plugin error")]
    Plugin(#[from] PluginError),
}

pub struct Runtime {
    config: WorkspaceDefinition,
    plugins: Vec<PluginHandle>,
    ctx: PluginContext,
}

pub mod daemon;
pub mod runtime;

pub use runtime::{run_runtime_current, spawn_runtime, LogicMessage, LogicSettings, LogicState};

#[cfg(test)]
mod tests {
    use super::*;
    use workspace::WorkspaceSettings;

    #[test]
    fn runtime_new_creates_with_config() {
        let workspace = WorkspaceDefinition {
            name: "test".to_string(),
            description: "test workspace".to_string(),
            target_hz: 500,
            plugins: Vec::new(),
            connections: Vec::new(),
            settings: WorkspaceSettings::default(),
        };

        let runtime = Runtime::new(workspace);
        assert_eq!(runtime.config().name, "test");
        assert_eq!(runtime.config().target_hz, 500);
    }

    #[test]
    fn runtime_tick_succeeds_with_no_plugins() {
        let workspace = WorkspaceDefinition {
            name: "empty".to_string(),
            description: String::new(),
            target_hz: 1000,
            plugins: Vec::new(),
            connections: Vec::new(),
            settings: WorkspaceSettings::default(),
        };

        let mut runtime = Runtime::new(workspace);
        assert!(runtime.tick().is_ok());
    }
}

impl Runtime {
    pub fn new(config: WorkspaceDefinition) -> Self {
        Self {
            config,
            plugins: Vec::new(),
            ctx: PluginContext::default(),
        }
    }

    pub fn add_plugin(&mut self, plugin: Box<dyn Plugin>, schedule: PluginSchedule) {
        self.plugins.push(PluginHandle { schedule, plugin });
    }

    pub fn config(&self) -> &WorkspaceDefinition {
        &self.config
    }

    pub fn tick(&mut self) -> Result<(), RuntimeError> {
        self.plugins
            .sort_by_key(|handle| std::cmp::Reverse(handle.schedule.priority));
        for handle in &mut self.plugins {
            handle.plugin.process(&mut self.ctx)?;
        }
        self.ctx.tick = self.ctx.tick.wrapping_add(1);
        Ok(())
    }
}
