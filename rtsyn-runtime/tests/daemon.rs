use rtsyn_runtime::daemon::DaemonService;
use std::time::Duration;

#[test]
fn daemon_service_creates_successfully() {
    let daemon = DaemonService::new();
    assert!(daemon.is_ok());
}

#[test]
fn daemon_service_runs_for_duration() {
    let daemon = DaemonService::new().expect("Failed to create daemon");
    let result = daemon.run_for_duration(Duration::from_millis(10));
    assert!(result.is_ok());
}
