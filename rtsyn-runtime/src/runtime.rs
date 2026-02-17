use crate::message_handler::{LogicMessage, LogicState};
use crate::rt_thread::{ActiveRtBackend, RuntimeThread};
use crate::runtime_core::run_runtime_loop;
use std::sync::mpsc::{Receiver, Sender};

/// Spawns the runtime in a new thread with real-time scheduling
pub fn spawn_runtime() -> Result<(Sender<LogicMessage>, Receiver<LogicState>), String> {
    let (logic_tx, logic_rx) = std::sync::mpsc::channel::<LogicMessage>();
    let (logic_state_tx, logic_state_rx) = std::sync::mpsc::channel::<LogicState>();

    RuntimeThread::spawn(move || {
        let _ = run_runtime_loop(logic_rx, logic_state_tx);
    })?;

    Ok((logic_tx, logic_state_rx))
}

/// Runs the runtime in the current thread with real-time scheduling
pub fn run_runtime_current(
    logic_rx: Receiver<LogicMessage>,
    logic_state_tx: Sender<LogicState>,
) -> Result<(), String> {
    ActiveRtBackend::prepare()?;
    let _ = run_runtime_loop(logic_rx, logic_state_tx);
    Ok(())
}
