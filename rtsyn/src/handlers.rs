use crate::commands::*;
use crate::output::*;
use rtsyn_cli::{
    client, daemon,
    plugin_creator::{
        create_plugin, parse_variable_line, CreatorBehavior, FieldType, PluginCreateRequest,
        PluginKindType, PluginLanguage,
    },
    protocol::{DaemonRequest, DaemonResponse, DEFAULT_SOCKET_PATH},
};
use rtsyn_gui::{run_gui, GuiConfig};
use std::io::{self, Write};
use std::process::{Command, Stdio};

pub fn handle_command(command: Option<Commands>) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        None => {
            run_gui(GuiConfig::default())?;
        }
        Some(Commands::Daemon { command }) => handle_daemon_command(command)?,
    }
    Ok(())
}

fn handle_daemon_command(command: DaemonCommands) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        DaemonCommands::Run { detach } => {
            if detach {
                if let Err(err) = spawn_detached_daemon() {
                    print_error(&err);
                } else {
                    print_info("Daemon started");
                }
            } else if let Err(err) = daemon::run_daemon() {
                print_error(&err);
            }
        }
        DaemonCommands::Stop => {
            let request = DaemonRequest::DaemonStop;
            match client::send_request(&request) {
                Ok(response) => handle_daemon_response(response),
                Err(err) => print_error(&err),
            }
        }
        DaemonCommands::Reload => {
            let request = DaemonRequest::DaemonReload;
            match client::send_request(&request) {
                Ok(response) => handle_daemon_response(response),
                Err(err) => print_error(&err),
            }
        }
        DaemonCommands::Plugin { command } => handle_plugin_command(command)?,
        DaemonCommands::Workspace { command } => handle_workspace_command(command)?,
        DaemonCommands::Connection { command } => handle_connection_command(command)?,
        DaemonCommands::Runtime { command } => handle_runtime_command(command)?,
    }
    Ok(())
}

fn handle_plugin_command(command: PluginCommands) -> Result<(), Box<dyn std::error::Error>> {
    let mut available_json_query = false;
    let mut runtime_list_json_query = false;
    let request = match command {
        PluginCommands::New => {
            if let Err(err) = run_plugin_creator_wizard() {
                print_error(&err);
            }
            return Ok(());
        }
        PluginCommands::Add { name } => DaemonRequest::PluginAdd { name },
        PluginCommands::Install { path } => {
            let install_path = std::path::Path::new(&path);
            let absolute_path = if install_path.is_absolute() {
                install_path.to_path_buf()
            } else {
                match std::fs::canonicalize(install_path) {
                    Ok(resolved) => resolved,
                    Err(err) => {
                        print_error(&format!("Failed to resolve install path: {err}"));
                        return Ok(());
                    }
                }
            };
            DaemonRequest::PluginInstall {
                path: absolute_path.to_string_lossy().to_string(),
            }
        }
        PluginCommands::Reinstall { name } => DaemonRequest::PluginReinstall { name },
        PluginCommands::Rebuild { name } => DaemonRequest::PluginRebuild { name },
        PluginCommands::Remove { id } => DaemonRequest::PluginRemove { id },
        PluginCommands::Uninstall { name } => DaemonRequest::PluginUninstall { name },
        PluginCommands::Available { json_query } => {
            available_json_query = json_query;
            DaemonRequest::PluginList
        }
        PluginCommands::List { json_query } => {
            runtime_list_json_query = json_query;
            DaemonRequest::RuntimeList
        }
        PluginCommands::Show { id } => DaemonRequest::RuntimeShow { id },
        PluginCommands::Set { id, json } => DaemonRequest::RuntimeSetVariables { id, json },
        PluginCommands::View { id } => {
            if let Err(err) = spawn_daemon_viewer(id) {
                print_error(&err);
            }
            return Ok(());
        }
        PluginCommands::Start { id } => DaemonRequest::RuntimePluginStart { id },
        PluginCommands::Stop { id } => DaemonRequest::RuntimePluginStop { id },
        PluginCommands::Restart { id } => DaemonRequest::RuntimePluginRestart { id },
    };
    match client::send_request(&request) {
        Ok(response) => {
            handle_plugin_response(response, available_json_query, runtime_list_json_query)
        }
        Err(err) => print_error(&err),
    }
    Ok(())
}

fn handle_workspace_command(command: WorkspaceCommands) -> Result<(), Box<dyn std::error::Error>> {
    let request = match command {
        WorkspaceCommands::List => DaemonRequest::WorkspaceList,
        WorkspaceCommands::Load { name } => DaemonRequest::WorkspaceLoad { name },
        WorkspaceCommands::New { name } => DaemonRequest::WorkspaceNew { name },
        WorkspaceCommands::Save { name } => DaemonRequest::WorkspaceSave { name },
        WorkspaceCommands::Edit { name } => DaemonRequest::WorkspaceEdit { name },
        WorkspaceCommands::Delete { name } => DaemonRequest::WorkspaceDelete { name },
    };
    match client::send_request(&request) {
        Ok(response) => handle_workspace_response(response),
        Err(err) => print_error(&err),
    }
    Ok(())
}

fn handle_connection_command(
    command: ConnectionCommands,
) -> Result<(), Box<dyn std::error::Error>> {
    let request = match command {
        ConnectionCommands::List => DaemonRequest::ConnectionList,
        ConnectionCommands::Show { plugin_id } => DaemonRequest::ConnectionShow { plugin_id },
        ConnectionCommands::Add {
            from_plugin,
            from_port,
            to_plugin,
            to_port,
            kind,
        } => DaemonRequest::ConnectionAdd {
            from_plugin,
            from_port,
            to_plugin,
            to_port,
            kind,
        },
        ConnectionCommands::Remove {
            from_plugin,
            from_port,
            to_plugin,
            to_port,
        } => DaemonRequest::ConnectionRemove {
            from_plugin,
            from_port,
            to_plugin,
            to_port,
        },
        ConnectionCommands::RemoveIndex { index } => DaemonRequest::ConnectionRemoveIndex { index },
    };
    match client::send_request(&request) {
        Ok(response) => handle_connection_response(response),
        Err(err) => print_error(&err),
    }
    Ok(())
}

fn handle_runtime_command(command: RuntimeCommands) -> Result<(), Box<dyn std::error::Error>> {
    let mut settings_json_query = false;
    let request = match command {
        RuntimeCommands::Settings { command } => match command {
            RuntimeSettingsCommands::Show { json_query } => {
                settings_json_query = json_query;
                DaemonRequest::RuntimeSettingsShow
            }
            RuntimeSettingsCommands::Set { json } => DaemonRequest::RuntimeSettingsSet { json },
            RuntimeSettingsCommands::Save => DaemonRequest::RuntimeSettingsSave,
            RuntimeSettingsCommands::Restore => DaemonRequest::RuntimeSettingsRestore,
            RuntimeSettingsCommands::Options { json_query } => {
                settings_json_query = json_query;
                DaemonRequest::RuntimeSettingsOptions
            }
        },
        RuntimeCommands::UmlDiagram => DaemonRequest::RuntimeUmlDiagram,
    };
    match client::send_request(&request) {
        Ok(response) => handle_runtime_response(response, settings_json_query),
        Err(err) => print_error(&err),
    }
    Ok(())
}

fn handle_daemon_response(response: DaemonResponse) {
    match response {
        DaemonResponse::Ok { message } => print_info(&message),
        DaemonResponse::Error { message } => print_error(&message),
        _ => {}
    }
}

fn handle_plugin_response(
    response: DaemonResponse,
    available_json_query: bool,
    runtime_list_json_query: bool,
) {
    match response {
        DaemonResponse::Ok { message } => print_info(&message),
        DaemonResponse::Error { message } => print_error(&message),
        DaemonResponse::PluginAdded { id } => print_info(&format!("Plugin added with id {id}")),
        DaemonResponse::PluginList { plugins } => {
            if available_json_query {
                let json =
                    serde_json::to_string_pretty(&plugins).unwrap_or_else(|_| "[]".to_string());
                println!("{json}");
                return;
            }
            print_plugin_list(&plugins);
        }
        DaemonResponse::RuntimeList { plugins } => {
            if runtime_list_json_query {
                let json =
                    serde_json::to_string_pretty(&plugins).unwrap_or_else(|_| "[]".to_string());
                println!("{json}");
                return;
            }
            print_runtime_list(&plugins);
        }
        DaemonResponse::RuntimeShow { id, kind, state } => {
            print_runtime_show(id, &kind, &state);
        }
        _ => {}
    }
}

fn handle_workspace_response(response: DaemonResponse) {
    match response {
        DaemonResponse::Ok { message } => print_info(&message),
        DaemonResponse::Error { message } => print_error(&message),
        DaemonResponse::WorkspaceList { workspaces } => {
            print_workspace_list(&workspaces);
        }
        _ => {}
    }
}

fn handle_connection_response(response: DaemonResponse) {
    match response {
        DaemonResponse::Ok { message } => print_info(&message),
        DaemonResponse::Error { message } => print_error(&message),
        DaemonResponse::ConnectionList { connections } => {
            print_connection_list(&connections);
        }
        _ => {}
    }
}

fn handle_runtime_response(response: DaemonResponse, settings_json_query: bool) {
    match response {
        DaemonResponse::Ok { message } => print_info(&message),
        DaemonResponse::Error { message } => print_error(&message),
        DaemonResponse::RuntimeSettings { settings } => {
            if settings_json_query {
                let json =
                    serde_json::to_string_pretty(&settings).unwrap_or_else(|_| "{}".to_string());
                println!("{json}");
                return;
            }
            print_runtime_settings(&settings);
        }
        DaemonResponse::RuntimeSettingsOptions { options } => {
            if settings_json_query {
                let json =
                    serde_json::to_string_pretty(&options).unwrap_or_else(|_| "{}".to_string());
                println!("{json}");
                return;
            }
            print_runtime_settings_options(&options);
        }
        DaemonResponse::RuntimeUmlDiagram { uml } => {
            println!("{uml}");
        }
        _ => {}
    }
}

fn spawn_detached_daemon() -> Result<(), String> {
    let socket_path = std::path::Path::new(DEFAULT_SOCKET_PATH);
    if socket_path.exists() {
        if std::os::unix::net::UnixStream::connect(socket_path).is_ok() {
            return Err("Daemon already running".to_string());
        }
        let _ = std::fs::remove_file(socket_path);
    }

    let exe = std::env::current_exe().map_err(|e| format!("Failed to get executable path: {e}"))?;
    let mut cmd = Command::new(exe);
    cmd.args(["daemon", "run"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to start daemon: {e}"))
}

fn spawn_daemon_viewer(plugin_id: u64) -> Result<(), String> {
    if std::os::unix::net::UnixStream::connect(DEFAULT_SOCKET_PATH).is_err() {
        return Err("Daemon is not running".to_string());
    }
    match client::send_request(&DaemonRequest::RuntimeShow { id: plugin_id }) {
        Ok(DaemonResponse::Error { message }) => {
            if message.contains("Plugin not found") {
                return Err("Plugin not found in runtime".to_string());
            }
        }
        Ok(_) => {}
        Err(err) => return Err(err),
    }
    let exe = std::env::current_exe().map_err(|e| format!("Failed to get executable path: {e}"))?;
    Command::new(exe)
        .env("RTSYN_DAEMON_VIEW_PLUGIN_ID", plugin_id.to_string())
        .env("RTSYN_DAEMON_SOCKET", DEFAULT_SOCKET_PATH)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to open viewer: {e}"))
}

fn run_plugin_creator_wizard() -> Result<(), String> {
    let base_dir = prompt_line("Base directory (default ./): ", Some("."))?;
    let plugin_name = prompt_line("Plugin name: ", None)?;
    if plugin_name.trim().is_empty() {
        return Err("Plugin name cannot be empty".to_string());
    }
    let description = prompt_line("Description (optional): ", Some(""))?;

    let language = loop {
        let value = prompt_line("Language [rust/c/cpp] (default rust): ", Some("rust"))?;
        match PluginLanguage::parse(&value) {
            Ok(v) => break v,
            Err(err) => print_info(&err),
        }
    };
    let plugin_type = loop {
        let value = prompt_line(
            "Plugin type [standard/device/computational] (default standard): ",
            Some("standard"),
        )?;
        match PluginKindType::parse(&value) {
            Ok(v) => break v,
            Err(err) => print_info(&err),
        }
    };

    let autostart = prompt_bool("Autostart? [y/N]: ", false)?;
    let supports_start_stop = prompt_bool("Supports start/stop? [Y/n]: ", true)?;
    let supports_restart = prompt_bool("Supports restart? [Y/n]: ", true)?;
    let supports_apply = prompt_bool("Supports apply/modify button? [y/N]: ", false)?;
    let external_window = prompt_bool("Open in external window? [y/N]: ", false)?;
    let starts_expanded = prompt_bool("Starts expanded? [Y/n]: ", true)?;

    let n_inputs = prompt_usize("Number of inputs: ")?;
    let n_outputs = prompt_usize("Number of outputs: ")?;
    let n_internal = prompt_usize("Number of internal variables: ")?;
    let n_vars = prompt_usize("Number of configurable variables: ")?;

    let mut inputs = Vec::with_capacity(n_inputs);
    for i in 0..n_inputs {
        let v = prompt_line(
            &format!("Input #{} name: ", i + 1),
            Some(&format!("in_{i}")),
        )?;
        inputs.push(v);
    }
    let mut outputs = Vec::with_capacity(n_outputs);
    for i in 0..n_outputs {
        let v = prompt_line(
            &format!("Output #{} name: ", i + 1),
            Some(&format!("out_{i}")),
        )?;
        outputs.push(v);
    }
    let mut internals = Vec::with_capacity(n_internal);
    for i in 0..n_internal {
        let v = prompt_line(
            &format!("Internal #{} name: ", i + 1),
            Some(&format!("x_{i}")),
        )?;
        internals.push(v);
    }

    let mut variables = Vec::with_capacity(n_vars);
    for i in 0..n_vars {
        let name = prompt_line(
            &format!("Variable #{} name: ", i + 1),
            Some(&format!("param_{i}")),
        )?;
        let field_type = loop {
            let ty = prompt_line(
                "  Type [float/bool/int/file] (default float): ",
                Some("float"),
            )?;
            match FieldType::parse(&ty) {
                Ok(v) => break v,
                Err(err) => print_info(&err),
            }
        };
        let default = prompt_line(
            &format!(
                "  Default ({}): ",
                match field_type {
                    FieldType::Float => "default 0.0",
                    FieldType::Bool => "default false",
                    FieldType::Int => "default 0",
                    FieldType::File => "default empty string",
                }
            ),
            Some(field_type.default_text()),
        )?;
        let spec_line = format!("{name}:{}={default}", field_type.as_str());
        let var = parse_variable_line(&spec_line)?;
        variables.push(var);
    }

    let required_input_ports_csv = prompt_line(
        "Required connected input ports to start (csv, optional): ",
        Some(""),
    )?;
    let required_output_ports_csv = prompt_line(
        "Required connected output ports to start (csv, optional): ",
        Some(""),
    )?;

    let req = PluginCreateRequest {
        base_dir: std::path::PathBuf::from(base_dir.trim()),
        name: plugin_name,
        description,
        language,
        plugin_type,
        behavior: CreatorBehavior {
            autostart,
            supports_start_stop,
            supports_restart,
            supports_apply,
            external_window,
            starts_expanded,
            required_input_ports: split_csv(&required_input_ports_csv),
            required_output_ports: split_csv(&required_output_ports_csv),
        },
        inputs,
        outputs,
        internal_variables: internals,
        variables,
    };

    let plugin_dir = create_plugin(&req)?;
    print_info(&format!("Plugin created at {}", plugin_dir.display()));
    Ok(())
}

fn prompt_line(prompt: &str, default: Option<&str>) -> Result<String, String> {
    print!("{prompt}");
    io::stdout().flush().map_err(|e| e.to_string())?;
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|e| format!("Failed to read input: {e}"))?;
    let value = line.trim();
    if value.is_empty() {
        Ok(default.unwrap_or("").to_string())
    } else {
        Ok(value.to_string())
    }
}

fn prompt_usize(prompt: &str) -> Result<usize, String> {
    loop {
        let value = prompt_line(prompt, None)?;
        if let Ok(n) = value.parse::<usize>() {
            return Ok(n);
        }
        print_info("Please enter a valid integer.");
    }
}

fn prompt_bool(prompt: &str, default: bool) -> Result<bool, String> {
    loop {
        let default_text = if default { "y" } else { "n" };
        let value = prompt_line(prompt, Some(default_text))?;
        match value.trim().to_ascii_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => print_info("Please answer y or n."),
        }
    }
}

fn split_csv(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}
