use connection::{
    Connection, ConnectionConfig, ConnectionFactory, ConnectionKind, InProcessConnection,
};

#[test]
fn in_process_send_recv() {
    let conn = InProcessConnection::new();
    assert!(conn.try_recv().unwrap().is_none());
    conn.send(42).unwrap();
    assert_eq!(conn.try_recv().unwrap(), Some(42));
    assert!(conn.try_recv().unwrap().is_none());
}

#[test]
fn factory_creates_connection() {
    let config = ConnectionConfig {
        kind: ConnectionKind::Pipe,
    };
    let conn: Box<dyn Connection<i32>> = ConnectionFactory::create(&config);
    conn.send(7).unwrap();
    assert_eq!(conn.try_recv().unwrap(), Some(7));
}
