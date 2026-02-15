use serial_test::serial;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

const SOCKET_PATH: &str = "/tmp/rtsyn-daemon.sock";
const INSTALLED_DB: &str = "app_plugins/installed_plugins.json";

fn wait_for_daemon(exe: &str) -> bool {
    for _ in 0..20 {
        let output = Command::new(exe)
            .args(["daemon", "plugin", "available"])
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
        .args(["daemon", "plugin", "available"])
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
        .args(["daemon", "plugin", "available"])
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
fn plugin_commands_cover_runtime_actions() {
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
        .args(["daemon", "plugin", "available"])
        .output()
        .expect("list available plugins");
    let output = String::from_utf8_lossy(&output.stdout);
    assert!(output.contains("temp_runtime_plugin"));

    let output = Command::new(exe)
        .args(["daemon", "plugin", "add", "temp_runtime_plugin"])
        .output()
        .expect("add runtime plugin through daemon plugin command");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("[RTSyn][ERROR]"));

    let output = Command::new(exe)
        .args(["daemon", "plugin", "remove", "1"])
        .output()
        .expect("remove runtime plugin through daemon plugin command");
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
        .args(["daemon", "plugin", "list"])
        .output()
        .expect("list runtime plugins");
    let output = String::from_utf8_lossy(&output.stdout);
    assert!(output.contains("No runtime plugins"));

    let output = Command::new(exe)
        .args(["daemon", "plugin", "add", "live_plotter"])
        .output()
        .expect("add runtime plugin");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("[RTSyn][ERROR]"));

    let output = Command::new(exe)
        .args(["daemon", "plugin", "list"])
        .output()
        .expect("list runtime plugins after add");
    let output = String::from_utf8_lossy(&output.stdout);
    assert!(output.contains("(live_plotter)"));

    let _ = Command::new(exe)
        .args(["daemon", "plugin", "remove", "1"])
        .status();

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
fn runtime_settings_options_and_show() {
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
        .args(["daemon", "runtime", "settings", "options", "--json-query"])
        .output()
        .expect("runtime settings options");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"frequency_units\""));
    assert!(stdout.contains("\"period_units\""));

    let output = Command::new(exe)
        .args(["daemon", "runtime", "settings", "show", "--json-query"])
        .output()
        .expect("runtime settings show");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"frequency_value\""));
    assert!(stdout.contains("\"period_value\""));

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
fn runtime_settings_syncs_frequency_and_period() {
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
        .args([
            "daemon",
            "runtime",
            "settings",
            "set",
            "{\"frequency_value\":500,\"frequency_unit\":\"hz\"}",
        ])
        .status()
        .expect("set runtime settings");
    assert!(status.success());

    let output = Command::new(exe)
        .args(["daemon", "runtime", "settings", "show", "--json-query"])
        .output()
        .expect("show runtime settings");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"frequency_value\": 500.0"));
    assert!(stdout.contains("\"period_value\": 2.0"));

    let status = Command::new(exe)
        .args([
            "daemon",
            "runtime",
            "settings",
            "set",
            "{\"period_value\":4,\"period_unit\":\"ms\"}",
        ])
        .status()
        .expect("set runtime settings period");
    assert!(status.success());

    let output = Command::new(exe)
        .args(["daemon", "runtime", "settings", "show", "--json-query"])
        .output()
        .expect("show runtime settings period");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"period_value\": 4.0"));
    assert!(stdout.contains("\"frequency_value\": 250.0"));

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
fn runtime_settings_save_and_restore_commands_work() {
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

    let save_output = Command::new(exe)
        .args(["daemon", "runtime", "settings", "save"])
        .output()
        .expect("save runtime settings");
    assert!(!String::from_utf8_lossy(&save_output.stderr).contains("[RTSyn][ERROR]"));

    let restore_output = Command::new(exe)
        .args(["daemon", "runtime", "settings", "restore"])
        .output()
        .expect("restore runtime settings");
    assert!(!String::from_utf8_lossy(&restore_output.stderr).contains("[RTSyn][ERROR]"));

    let show_output = Command::new(exe)
        .args(["daemon", "runtime", "settings", "show", "--json-query"])
        .output()
        .expect("show runtime settings");
    let stdout = String::from_utf8_lossy(&show_output.stdout);
    assert!(stdout.contains("\"frequency_value\""));
    assert!(stdout.contains("\"period_value\""));

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
fn runtime_uml_diagram_command_returns_uml_text() {
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
        .args(["daemon", "runtime", "uml-diagram"])
        .output()
        .expect("runtime uml diagram");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("[RTSyn][ERROR]"));
    assert!(stdout.contains("@startuml"));
    assert!(stdout.contains("@enduml"));

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
fn runtime_plugin_set_updates_config() {
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
        .args(["daemon", "plugin", "add", "performance_monitor"])
        .output()
        .expect("add runtime plugin");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("[RTSyn][ERROR]"));

    let _ = Command::new(exe)
        .args(["daemon", "plugin", "start", "1"])
        .status();

    let status = Command::new(exe)
        .args(["daemon", "plugin", "set", "1", "{\"max_latency_us\":2000}"])
        .status()
        .expect("set runtime plugin variables");
    assert!(status.success());

    let mut found = false;
    for _ in 0..20 {
        let output = Command::new(exe)
            .args(["daemon", "plugin", "show", "1"])
            .output()
            .expect("show runtime plugin");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("[RTSyn][ERROR]") {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        if stdout.contains("max_latency_us: 2000") {
            found = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(found);

    let _ = Command::new(exe)
        .args(["daemon", "plugin", "remove", "1"])
        .status();

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
fn connection_add_validates_ports() {
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

    let _ = Command::new(exe)
        .args(["daemon", "workspace", "delete", "test_connections"])
        .status();
    let status = Command::new(exe)
        .args(["daemon", "workspace", "new", "test_connections"])
        .status()
        .expect("new workspace");
    assert!(status.success());
    let status = Command::new(exe)
        .args(["daemon", "workspace", "load", "test_connections"])
        .status()
        .expect("load workspace");
    assert!(status.success());

    let output = Command::new(exe)
        .args(["daemon", "plugin", "add", "performance_monitor"])
        .output()
        .expect("add performance_monitor");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("[RTSyn][ERROR]"));

    let output = Command::new(exe)
        .args(["daemon", "plugin", "add", "live_plotter"])
        .output()
        .expect("add live_plotter");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("[RTSyn][ERROR]"));

    let output = Command::new(exe)
        .args([
            "daemon",
            "connection",
            "add",
            "--from-plugin",
            "1",
            "--from-port",
            "bad_port",
            "--to-plugin",
            "2",
            "--to-port",
            "in_0",
        ])
        .output()
        .expect("invalid from port");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("[RTSyn][ERROR]"));

    let output = Command::new(exe)
        .args([
            "daemon",
            "connection",
            "add",
            "--from-plugin",
            "1",
            "--from-port",
            "period_us",
            "--to-plugin",
            "2",
            "--to-port",
            "bad_port",
        ])
        .output()
        .expect("invalid to port");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("[RTSyn][ERROR]"));

    let output = Command::new(exe)
        .args([
            "daemon",
            "connection",
            "add",
            "--from-plugin",
            "1",
            "--from-port",
            "period_us",
            "--to-plugin",
            "2",
            "--to-port",
            "in",
        ])
        .output()
        .expect("invalid extendable port");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("[RTSyn][ERROR]"));

    let output = Command::new(exe)
        .args([
            "daemon",
            "connection",
            "add",
            "--from-plugin",
            "1",
            "--from-port",
            "period_us",
            "--to-plugin",
            "2",
            "--to-port",
            "in_1",
        ])
        .output()
        .expect("invalid extendable port index");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("[RTSyn][ERROR]"));

    let output = Command::new(exe)
        .args([
            "daemon",
            "connection",
            "add",
            "--from-plugin",
            "1",
            "--from-port",
            "period_us",
            "--to-plugin",
            "2",
            "--to-port",
            "in_0",
        ])
        .output()
        .expect("valid connection");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("[RTSyn][ERROR]"));

    let output = Command::new(exe)
        .args(["daemon", "connection", "list"])
        .output()
        .expect("list connections");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("1:period_us -> 2:in_0"));

    let _ = Command::new(exe)
        .args(["daemon", "plugin", "remove", "2"])
        .status();
    let _ = Command::new(exe)
        .args(["daemon", "plugin", "remove", "1"])
        .status();

    let _ = Command::new(exe).args(["daemon", "stop"]).status();
    let _ = child.wait();

    if let Some(data) = installed_db_backup {
        let _ = std::fs::write(INSTALLED_DB, data);
    } else {
        let _ = std::fs::remove_file(INSTALLED_DB);
    }
}
