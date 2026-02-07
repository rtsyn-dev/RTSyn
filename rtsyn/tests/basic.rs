use std::process::Command;
use std::time::Duration;

const SOCKET_PATH: &str = "/tmp/rtsyn-daemon.sock";

#[test]
fn daemon_mode_exits_cleanly() {
    let exe = env!("CARGO_BIN_EXE_rtsyn");
    if std::path::Path::new(SOCKET_PATH).exists() {
        if std::os::unix::net::UnixStream::connect(SOCKET_PATH).is_ok() {
            return;
        }
        let _ = std::fs::remove_file(SOCKET_PATH);
    }

    let mut child = Command::new(exe)
        .args(["daemon", "run"])
        .spawn()
        .expect("run rtsyn daemon");

    std::thread::sleep(Duration::from_millis(150));

    let status = Command::new(exe)
        .args(["daemon", "plugin", "list"])
        .status()
        .expect("query daemon");
    assert!(status.success());

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn daemon_stop_terminates_process() {
    let exe = env!("CARGO_BIN_EXE_rtsyn");
    if std::path::Path::new(SOCKET_PATH).exists() {
        if std::os::unix::net::UnixStream::connect(SOCKET_PATH).is_ok() {
            let _ = Command::new(exe).args(["daemon", "stop"]).status();
        }
        let _ = std::fs::remove_file(SOCKET_PATH);
    }

    let mut child = Command::new(exe)
        .args(["daemon", "run"])
        .spawn()
        .expect("run rtsyn daemon");

    std::thread::sleep(Duration::from_millis(150));

    let status = Command::new(exe)
        .args(["daemon", "stop"])
        .status()
        .expect("stop daemon");
    assert!(status.success());

    let mut waited = 0;
    loop {
        if let Ok(Some(_)) = child.try_wait() {
            break;
        }
        if waited > 20 {
            let _ = child.kill();
            panic!("daemon did not stop in time");
        }
        std::thread::sleep(Duration::from_millis(50));
        waited += 1;
    }
}
