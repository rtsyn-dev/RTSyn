use rtsyn_cli::protocol::{
    ConnectionSummary, PluginSummary, RuntimePluginState, RuntimePluginSummary,
    RuntimeSettingsOptions, WorkspaceSummary,
};
use workspace::WorkspaceSettings;

pub fn print_info(message: &str) {
    println!("[RTSyn][INFO] {message}");
}

pub fn print_error(message: &str) {
    eprintln!("[RTSyn][ERROR]: {message}");
}

pub fn print_plugin_list(plugins: &[PluginSummary]) {
    if plugins.is_empty() {
        print_info("No plugins installed");
    } else {
        print_info("List of available plugins:");
        for plugin in plugins {
            let version = plugin.version.as_deref().unwrap_or("unknown");
            let removable = if plugin.removable {
                "removable"
            } else {
                "bundled"
            };
            if let Some(path) = &plugin.path {
                println!("{} ({}) [{}] {}", plugin.name, plugin.kind, removable, path);
            } else {
                println!(
                    "{} ({}) [{}] v{}",
                    plugin.name, plugin.kind, removable, version
                );
            }
        }
    }
}

pub fn print_runtime_list(plugins: &[RuntimePluginSummary]) {
    if plugins.is_empty() {
        print_info("No runtime plugins");
    } else {
        print_info("Runtime plugins:");
        for plugin in plugins {
            println!("{} ({})", plugin.id, plugin.kind);
        }
    }
}

pub fn print_runtime_show(id: u64, kind: &str, state: &RuntimePluginState) {
    println!("[RTSyn][INFO] {id} - {kind}");
    println!("Variables:");
    if state.variables.is_empty() {
        println!("\t(none)");
    } else {
        for (name, value) in &state.variables {
            println!("\t{name}: {value}");
        }
    }
    println!("Outputs:");
    if state.outputs.is_empty() {
        println!("\t(none)");
    } else {
        for (name, value) in &state.outputs {
            println!("\t{name}: {value}");
        }
    }
    println!("Inputs:");
    if state.inputs.is_empty() {
        println!("\t(none)");
    } else {
        for (name, value) in &state.inputs {
            println!("\t{name}: {value}");
        }
    }
    println!("Internal variables:");
    if state.internal_variables.is_empty() {
        println!("\t(none)");
    } else {
        for (name, value) in &state.internal_variables {
            println!("\t{name}: {value}");
        }
    }
}

pub fn print_workspace_list(workspaces: &[WorkspaceSummary]) {
    if workspaces.is_empty() {
        print_info("No workspaces found");
    } else {
        print_info("List of workspaces:");
        for ws in workspaces {
            let plugins = if ws.plugins == 1 { "plugin" } else { "plugins" };
            println!(
                "{} - {} {} ({})",
                ws.name, ws.plugins, plugins, ws.description
            );
        }
    }
}

pub fn print_connection_list(connections: &[ConnectionSummary]) {
    if connections.is_empty() {
        print_info("No connections found");
    } else {
        print_info("List of connections:");
        for conn in connections {
            println!(
                "[{}] {}:{} -> {}:{} ({})",
                conn.index,
                conn.from_plugin,
                conn.from_port,
                conn.to_plugin,
                conn.to_port,
                conn.kind
            );
        }
    }
}

pub fn print_runtime_settings(settings: &WorkspaceSettings) {
    print_info("Runtime settings:");
    println!("frequency_value: {}", settings.frequency_value);
    println!("frequency_unit: {}", settings.frequency_unit);
    println!("period_value: {}", settings.period_value);
    println!("period_unit: {}", settings.period_unit);
    println!("selected_cores: {:?}", settings.selected_cores);
}

pub fn print_runtime_settings_options(options: &RuntimeSettingsOptions) {
    print_info("Runtime settings options:");
    println!("frequency_units: {}", options.frequency_units.join(", "));
    println!("period_units: {}", options.period_units.join(", "));
    println!("min_frequency_value: {}", options.min_frequency_value);
    println!("min_period_value: {}", options.min_period_value);
    println!(
        "max_integration_steps: {}..={}",
        options.max_integration_steps_min, options.max_integration_steps_max
    );
}
