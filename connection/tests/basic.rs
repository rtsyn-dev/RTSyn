use connection::{Connection, ConnectionConfig, ConnectionFactory, ConnectionKind};

fn create_connection(kind: ConnectionKind, queue_capacity: usize) -> Box<dyn Connection<i32>> {
    let config = ConnectionConfig {
        kind,
        queue_capacity,
    };
    ConnectionFactory::create(&config)
}

#[test]
fn config_default_is_inprocess_with_reasonable_capacity() {
    let config = ConnectionConfig::default();
    assert!(matches!(config.kind, ConnectionKind::InProcess));
    assert_eq!(config.queue_capacity, 1024);
}

#[test]
fn in_process_send_recv() {
    let conn = create_connection(ConnectionKind::InProcess, 1024);
    assert!(conn.try_recv().unwrap().is_none());
    conn.send(42).unwrap();
    assert_eq!(conn.try_recv().unwrap(), Some(42));
    assert!(conn.try_recv().unwrap().is_none());
}

#[test]
fn factory_creates_connection() {
    let conn = create_connection(ConnectionKind::Pipe, 1024);
    conn.send(7).unwrap();
    assert_eq!(conn.try_recv().unwrap(), Some(7));
}

#[test]
fn factory_creates_inprocess_connection() {
    let conn = create_connection(ConnectionKind::InProcess, 1024);
    conn.send(9).unwrap();
    assert_eq!(conn.try_recv().unwrap(), Some(9));
}

#[test]
fn shared_memory_send_recv() {
    let conn = create_connection(ConnectionKind::SharedMemory, 4);
    conn.send(1).unwrap();
    conn.send(2).unwrap();
    assert_eq!(conn.try_recv().unwrap(), Some(1));
    assert_eq!(conn.try_recv().unwrap(), Some(2));
    assert!(conn.try_recv().unwrap().is_none());
}

#[test]
fn shared_memory_is_fifo() {
    let conn = create_connection(ConnectionKind::SharedMemory, 4);
    conn.send(1).unwrap();
    conn.send(2).unwrap();
    conn.send(3).unwrap();
    assert_eq!(conn.try_recv().unwrap(), Some(1));
    assert_eq!(conn.try_recv().unwrap(), Some(2));
    assert_eq!(conn.try_recv().unwrap(), Some(3));
    assert!(conn.try_recv().unwrap().is_none());
}

#[test]
fn shared_memory_respects_capacity() {
    let conn = create_connection(ConnectionKind::SharedMemory, 1);
    conn.send(10).unwrap();
    assert!(conn.send(11).is_err());
    assert_eq!(conn.try_recv().unwrap(), Some(10));
}

#[test]
fn shared_memory_clamps_zero_capacity_to_one() {
    let conn = create_connection(ConnectionKind::SharedMemory, 0);
    conn.send(10).unwrap();
    assert!(conn.send(11).is_err());
    assert_eq!(conn.try_recv().unwrap(), Some(10));
}

#[test]
fn pipe_respects_capacity() {
    let conn = create_connection(ConnectionKind::Pipe, 1);
    conn.send(10).unwrap();
    assert!(conn.send(11).is_err());
    assert_eq!(conn.try_recv().unwrap(), Some(10));
}

#[test]
fn pipe_is_fifo() {
    let conn = create_connection(ConnectionKind::Pipe, 4);
    conn.send(1).unwrap();
    conn.send(2).unwrap();
    conn.send(3).unwrap();
    assert_eq!(conn.try_recv().unwrap(), Some(1));
    assert_eq!(conn.try_recv().unwrap(), Some(2));
    assert_eq!(conn.try_recv().unwrap(), Some(3));
    assert!(conn.try_recv().unwrap().is_none());
}

#[test]
fn pipe_clamps_zero_capacity_to_one() {
    let conn = create_connection(ConnectionKind::Pipe, 0);
    conn.send(10).unwrap();
    assert!(conn.send(11).is_err());
    assert_eq!(conn.try_recv().unwrap(), Some(10));
}

#[test]
fn factory_creates_shared_memory_connection() {
    let conn = create_connection(ConnectionKind::SharedMemory, 1);
    conn.send(5).unwrap();
    assert!(conn.send(6).is_err());
    assert_eq!(conn.try_recv().unwrap(), Some(5));
}
