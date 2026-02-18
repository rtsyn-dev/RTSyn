use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy)]
pub enum ConnectionKind {
    SharedMemory,
    Pipe,
    InProcess,
}

#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub kind: ConnectionKind,
    pub queue_capacity: usize,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            kind: ConnectionKind::InProcess,
            queue_capacity: 1024,
        }
    }
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
struct InProcessConnection<T> {
    sender: Sender<T>,
    receiver: Receiver<T>,
}

impl<T> InProcessConnection<T> {
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self { sender, receiver }
    }
}

#[derive(Debug)]
struct PipeConnection<T> {
    sender: SyncSender<T>,
    receiver: Receiver<T>,
}

impl<T> PipeConnection<T> {
    fn new(capacity: usize) -> Self {
        let (sender, receiver) = mpsc::sync_channel(capacity.max(1));
        Self { sender, receiver }
    }
}

impl<T: Send + 'static> Connection<T> for PipeConnection<T> {
    fn send(&self, value: T) -> Result<(), ConnectionError> {
        self.sender
            .try_send(value)
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

#[derive(Debug)]
struct SharedMemoryConnection<T> {
    queue: Arc<Mutex<VecDeque<T>>>,
    capacity: usize,
}

impl<T> SharedMemoryConnection<T> {
    fn new(capacity: usize) -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::with_capacity(capacity.max(1)))),
            capacity: capacity.max(1),
        }
    }
}

impl<T: Send + 'static> Connection<T> for SharedMemoryConnection<T> {
    fn send(&self, value: T) -> Result<(), ConnectionError> {
        let mut queue = self.queue.lock().map_err(|_| ConnectionError::SendFailed)?;
        if queue.len() >= self.capacity {
            return Err(ConnectionError::SendFailed);
        }
        queue.push_back(value);
        Ok(())
    }

    fn try_recv(&self) -> Result<Option<T>, ConnectionError> {
        let mut queue = self.queue.lock().map_err(|_| ConnectionError::RecvFailed)?;
        Ok(queue.pop_front())
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
            ConnectionKind::SharedMemory => Box::new(SharedMemoryConnection::new(
                config.queue_capacity,
            )),
            ConnectionKind::Pipe => Box::new(PipeConnection::new(config.queue_capacity)),
            ConnectionKind::InProcess => Box::new(InProcessConnection::new()),
        }
    }
}
