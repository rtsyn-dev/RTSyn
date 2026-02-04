use std::process::Command;

#[test]
fn daemon_mode_exits_cleanly() {
    let exe = env!("CARGO_BIN_EXE_rtsyn");
    let status = Command::new(exe)
        .args(["daemon", "--duration-seconds", "1"])
        .status()
        .expect("run rtsyn daemon");
    assert!(status.success());
}
