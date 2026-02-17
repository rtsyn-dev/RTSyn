use crate::message_handler::{LogicMessage, LogicSettings, LogicState};
use crate::runtime::spawn_runtime;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;
use workspace::WorkspaceDefinition;

pub struct DaemonService {
    logic_tx: Sender<LogicMessage>,
    logic_state_rx: Receiver<LogicState>,
}

impl DaemonService {
    pub fn new() -> Result<Self, String> {
        let (logic_tx, logic_state_rx) = spawn_runtime()?;
        Ok(Self {
            logic_tx,
            logic_state_rx,
        })
    }

    pub fn load_workspace(&self, workspace: WorkspaceDefinition) {
        let _ = self.logic_tx.send(LogicMessage::UpdateWorkspace(workspace));
    }

    pub fn update_settings(&self, settings: LogicSettings) {
        let _ = self.logic_tx.send(LogicMessage::UpdateSettings(settings));
    }

    pub fn set_plugin_running(&self, plugin_id: u64, running: bool) {
        let _ = self
            .logic_tx
            .send(LogicMessage::SetPluginRunning(plugin_id, running));
    }

    pub fn restart_plugin(&self, plugin_id: u64) {
        let _ = self.logic_tx.send(LogicMessage::RestartPlugin(plugin_id));
    }

    pub fn poll_state(&self) -> Option<LogicState> {
        self.logic_state_rx.try_recv().ok()
    }

    pub fn run_for_duration(&self, duration: Duration) -> Result<(), String> {
        let start = std::time::Instant::now();
        while start.elapsed() < duration {
            if let Some(_state) = self.poll_state() {
                // Process state if needed
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    pub fn run_for_ticks(&self, ticks: u64) -> Result<(), String> {
        let mut last_tick = 0;
        while last_tick < ticks {
            if let Some(state) = self.poll_state() {
                last_tick = state.tick;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    }
}
