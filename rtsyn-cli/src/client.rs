use crate::protocol::{DaemonRequest, DaemonResponse, DEFAULT_SOCKET_PATH};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

pub fn send_request(request: &DaemonRequest) -> Result<DaemonResponse, String> {
    send_request_to(DEFAULT_SOCKET_PATH, request)
}

pub fn send_request_to(path: &str, request: &DaemonRequest) -> Result<DaemonResponse, String> {
    let mut stream = UnixStream::connect(path)
        .map_err(|_| format!("Failed to connect to daemon at {path}. Is it running?"))?;
    let payload = serde_json::to_string(request).map_err(|e| e.to_string())?;
    stream
        .write_all(format!("{payload}\n").as_bytes())
        .map_err(|e| e.to_string())?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    if line.trim().is_empty() {
        return Err("Daemon returned empty response".to_string());
    }
    serde_json::from_str::<DaemonResponse>(line.trim()).map_err(|e| e.to_string())
}
