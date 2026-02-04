use std::sync::mpsc::{self, Receiver, Sender};

#[derive(Debug, Clone, Copy)]
pub enum ConnectionKind {
    SharedMemory,
    Pipe,
    InProcess,
}

#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub kind: ConnectionKind,
}

#[derive(thiserror::Error, Debug)]
pub enum ConnectionError {
    #[error("send failed")]
    SendFailed,
    #[error("receive failed")]
    RecvFailed,
}

pub trait Connection<T>: Send {
    fn send(&self, value: T) -> Result<(), ConnectionError>;
    fn try_recv(&self) -> Result<Option<T>, ConnectionError>;
}

#[derive(Debug)]
pub struct InProcessConnection<T> {
    sender: Sender<T>,
    receiver: Receiver<T>,
}

impl<T> InProcessConnection<T> {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self { sender, receiver }
    }
}

impl<T: Send + 'static> Connection<T> for InProcessConnection<T> {
    fn send(&self, value: T) -> Result<(), ConnectionError> {
        self.sender
            .send(value)
            .map_err(|_| ConnectionError::SendFailed)
    }

    fn try_recv(&self) -> Result<Option<T>, ConnectionError> {
        match self.receiver.try_recv() {
            Ok(value) => Ok(Some(value)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(ConnectionError::RecvFailed),
        }
    }
}

pub struct ConnectionFactory;

impl ConnectionFactory {
    pub fn create<T: Send + 'static>(config: &ConnectionConfig) -> Box<dyn Connection<T>> {
        match config.kind {
            ConnectionKind::SharedMemory | ConnectionKind::Pipe | ConnectionKind::InProcess => {
                Box::new(InProcessConnection::new())
            }
        }
    }
}
