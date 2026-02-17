use std::path::PathBuf;
use std::process::Command;

pub fn has_rt_capabilities() -> bool {
    #[cfg(unix)]
    unsafe {
        let policy = libc::sched_getscheduler(0);
        policy == libc::SCHED_FIFO || policy == libc::SCHED_RR
    }
    #[cfg(not(unix))]
    false
}

pub fn zenity_file_dialog(mode: &str, filter: Option<&str>) -> Option<PathBuf> {
    zenity_file_dialog_with_name(mode, filter, None)
}

pub fn zenity_file_dialog_with_name(
    mode: &str,
    filter: Option<&str>,
    filename: Option<&str>,
) -> Option<PathBuf> {
    let mut cmd = Command::new("zenity");
    cmd.arg("--file-selection");

    match mode {
        "save" => {
            cmd.arg("--save");
        }
        "folder" => {
            cmd.arg("--directory");
        }
        _ => {} // open file is default
    }

    if let Some(f) = filter {
        cmd.arg("--file-filter").arg(f);
    }

    if let Some(name) = filename {
        cmd.arg("--filename").arg(name);
    }

    cmd.output().ok().and_then(|output| {
        if output.status.success() {
            let path_string = String::from_utf8_lossy(&output.stdout);
            let path_str = path_string.trim();
            if !path_str.is_empty() {
                Some(PathBuf::from(path_str))
            } else {
                None
            }
        } else {
            None
        }
    })
}

pub fn spawn_file_dialog_thread<F, T>(f: F) -> std::thread::JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    std::thread::spawn(f)
}
