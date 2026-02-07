use std::process::Command;
use std::time::Duration;
use serial_test::serial;
use std::path::Path;

const SOCKET_PATH: &str = "/tmp/rtsyn-daemon.sock";
const INSTALLED_DB: &str = "app_plugins/installed_plugins.json";

fn wait_for_daemon(exe: &str) -> bool {
    for _ in 0..20 {
        let output = Command::new(exe)
            .args(["daemon", "plugin", "list"])
            .output()
            .expect("query daemon");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("[RTSyn][ERROR]") {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        if stdout.contains("CSV Recorder") {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
#[serial]
fn daemon_mode_exits_cleanly() {
    let exe = env!("CARGO_BIN_EXE_rtsyn");
    let mut installed_db_backup: Option<Vec<u8>> = None;
    if Path::new(INSTALLED_DB).exists() {
        installed_db_backup = std::fs::read(INSTALLED_DB).ok();
        let _ = std::fs::remove_file(INSTALLED_DB);
    }
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

    assert!(wait_for_daemon(exe));

    let _ = child.kill();
    let _ = child.wait();

    if let Some(data) = installed_db_backup {
        let _ = std::fs::write(INSTALLED_DB, data);
    } else {
        let _ = std::fs::remove_file(INSTALLED_DB);
    }
}

#[test]
#[serial]
fn daemon_stop_terminates_process() {
    let exe = env!("CARGO_BIN_EXE_rtsyn");
    let mut installed_db_backup: Option<Vec<u8>> = None;
    if Path::new(INSTALLED_DB).exists() {
        installed_db_backup = std::fs::read(INSTALLED_DB).ok();
        let _ = std::fs::remove_file(INSTALLED_DB);
    }
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

    assert!(wait_for_daemon(exe));

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

    if let Some(data) = installed_db_backup {
        let _ = std::fs::write(INSTALLED_DB, data);
    } else {
        let _ = std::fs::remove_file(INSTALLED_DB);
    }
}

#[test]
#[serial]
fn daemon_reload_resets_state() {
    let exe = env!("CARGO_BIN_EXE_rtsyn");
    let mut installed_db_backup: Option<Vec<u8>> = None;
    if Path::new(INSTALLED_DB).exists() {
        installed_db_backup = std::fs::read(INSTALLED_DB).ok();
        let _ = std::fs::remove_file(INSTALLED_DB);
    }
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

    assert!(wait_for_daemon(exe));

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let temp_plugin_path = temp_dir.path().join("temp-reload-plugin");
    std::fs::create_dir_all(&temp_plugin_path).expect("create plugin dir");
    std::fs::write(
        temp_plugin_path.join("plugin.toml"),
        "name = \"Temp Reload Plugin\"\nkind = \"temp_reload_plugin\"\nversion = \"0.1.0\"\n",
    )
    .expect("write plugin.toml");

    let output = Command::new(exe)
        .args([
            "daemon",
            "plugin",
            "install",
            temp_plugin_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("install plugin");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("[RTSyn][ERROR]"));

    let output = Command::new(exe)
        .args(["daemon", "plugin", "list"])
        .output()
        .expect("list plugins");
    let output = String::from_utf8_lossy(&output.stdout);
    assert!(output.contains("temp_reload_plugin"));

    let status = Command::new(exe)
        .args(["daemon", "reload"])
        .status()
        .expect("reload daemon");
    assert!(status.success());

    let output = Command::new(exe)
        .args(["daemon", "plugin", "list"])
        .output()
        .expect("list plugins after reload");
    let output = String::from_utf8_lossy(&output.stdout);
    assert!(output.contains("temp_reload_plugin"));

    let output = Command::new(exe)
        .args(["daemon", "workspace", "save"])
        .output()
        .expect("save after reload");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("[RTSyn][ERROR]"));

    let _ = Command::new(exe).args(["daemon", "stop"]).status();
    let _ = child.wait();

    if let Some(data) = installed_db_backup {
        let _ = std::fs::write(INSTALLED_DB, data);
    } else {
        let _ = std::fs::remove_file(INSTALLED_DB);
    }
}

#[test]
#[serial]
fn runtime_commands_match_plugin_commands() {
    let exe = env!("CARGO_BIN_EXE_rtsyn");
    let mut installed_db_backup: Option<Vec<u8>> = None;
    if Path::new(INSTALLED_DB).exists() {
        installed_db_backup = std::fs::read(INSTALLED_DB).ok();
        let _ = std::fs::remove_file(INSTALLED_DB);
    }
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
    assert!(wait_for_daemon(exe));

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let temp_plugin_path = temp_dir.path().join("temp-runtime-plugin");
    std::fs::create_dir_all(&temp_plugin_path).expect("create plugin dir");
    std::fs::write(
        temp_plugin_path.join("plugin.toml"),
        "name = \"Temp Runtime Plugin\"\nkind = \"temp_runtime_plugin\"\nversion = \"0.1.0\"\n",
    )
    .expect("write plugin.toml");

    let output = Command::new(exe)
        .args([
            "daemon",
            "plugin",
            "install",
            temp_plugin_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("install plugin");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("[RTSyn][ERROR]"));

    let output = Command::new(exe)
        .args(["daemon", "runtime", "available"])
        .output()
        .expect("list runtime available plugins");
    let output = String::from_utf8_lossy(&output.stdout);
    assert!(output.contains("temp_runtime_plugin"));

    let output = Command::new(exe)
        .args(["daemon", "runtime", "add", "temp_runtime_plugin"])
        .output()
        .expect("add runtime plugin");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("[RTSyn][ERROR]"));

    let output = Command::new(exe)
        .args(["daemon", "runtime", "remove", "1"])
        .output()
        .expect("remove runtime plugin");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("[RTSyn][ERROR]"));

    let _ = Command::new(exe).args(["daemon", "stop"]).status();
    let _ = child.wait();

    if let Some(data) = installed_db_backup {
        let _ = std::fs::write(INSTALLED_DB, data);
    } else {
        let _ = std::fs::remove_file(INSTALLED_DB);
    }
}

#[test]
#[serial]
fn runtime_list_reflects_workspace() {
    let exe = env!("CARGO_BIN_EXE_rtsyn");
    let mut installed_db_backup: Option<Vec<u8>> = None;
    if Path::new(INSTALLED_DB).exists() {
        installed_db_backup = std::fs::read(INSTALLED_DB).ok();
        let _ = std::fs::remove_file(INSTALLED_DB);
    }
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
    assert!(wait_for_daemon(exe));

    let output = Command::new(exe)
        .args(["daemon", "runtime", "list"])
        .output()
        .expect("list runtime plugins");
    let output = String::from_utf8_lossy(&output.stdout);
    assert!(output.contains("No runtime plugins"));

    let output = Command::new(exe)
        .args(["daemon", "runtime", "add", "live_plotter"])
        .output()
        .expect("add runtime plugin");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("[RTSyn][ERROR]"));

    let output = Command::new(exe)
        .args(["daemon", "runtime", "list"])
        .output()
        .expect("list runtime plugins after add");
    let output = String::from_utf8_lossy(&output.stdout);
    assert!(output.contains("(live_plotter)"));

    let _ = Command::new(exe)
        .args(["daemon", "runtime", "remove", "1"])
        .status();

    let _ = Command::new(exe).args(["daemon", "stop"]).status();
    let _ = child.wait();

    if let Some(data) = installed_db_backup {
        let _ = std::fs::write(INSTALLED_DB, data);
    } else {
        let _ = std::fs::remove_file(INSTALLED_DB);
    }
}
