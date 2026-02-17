use serde_json;
use std::path::PathBuf;

pub mod scaffolding;
pub mod templates;
pub mod validation;

pub use validation::{
    normalize_default, parse_variable_line, quote_array, sanitize_names, strip_language_suffix,
    to_kebab_case, to_pascal_case, CreatorBehavior, FieldType, PluginKindType, PluginLanguage,
    PluginVariable,
};

pub use templates::{
    generate_c_match_arms, generate_c_process_body, generate_c_state_fields, generate_default_vars,
    generate_match_arms, generate_process_body, generate_state_fields, plugin_templates_dir,
    render_to_file,
};

pub use scaffolding::{create_plugin_files, create_plugin_structure};

#[derive(Debug, Clone)]
pub struct PluginCreateRequest {
    pub base_dir: PathBuf,
    pub name: String,
    pub description: String,
    pub language: PluginLanguage,
    pub plugin_type: PluginKindType,
    pub behavior: CreatorBehavior,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub internal_variables: Vec<String>,
    pub variables: Vec<PluginVariable>,
}

pub fn create_plugin(req: &PluginCreateRequest) -> Result<PathBuf, String> {
    let requested_name = req.name.trim();
    if requested_name.is_empty() {
        return Err("Plugin name cannot be empty".to_string());
    }
    let plugin_name = strip_language_suffix(requested_name);
    let plugin_name = if plugin_name.is_empty() {
        requested_name.to_string()
    } else {
        plugin_name
    };

    let kind = validation::to_snake_case(&plugin_name);
    let slug = to_kebab_case(&plugin_name);
    let folder_base = slug.clone();
    let package_name = folder_base.clone();
    let rust_struct = to_pascal_case(&kind);
    let core_prefix = kind.clone();
    let core_state_tag = format!("{kind}_state");
    let core_state_type = format!("{kind}_state_t");
    let ffi_core_struct = format!("{rust_struct}Core");

    let (plugin_dir, src_dir) = create_plugin_structure(&req.base_dir, &folder_base)?;

    let inputs = sanitize_names(&req.inputs);
    let outputs = sanitize_names(&req.outputs);
    let internals = sanitize_names(&req.internal_variables);
    let numeric_vars: Vec<&PluginVariable> = req
        .variables
        .iter()
        .filter(|v| matches!(v.field_type, FieldType::Float | FieldType::Int))
        .collect();

    let input_array = quote_array(&inputs);
    let output_array = quote_array(&outputs);
    let internal_array = quote_array(&internals);

    let (state_fields, default_fields) =
        generate_state_fields(&inputs, &outputs, &internals, &numeric_vars);
    let (c_state_fields, c_default_fields) =
        generate_c_state_fields(&inputs, &outputs, &internals, &numeric_vars);
    let (config_match_arms, input_match_arms, output_match_arms, internal_match_arms) =
        generate_match_arms(&numeric_vars, &inputs, &outputs, &internals);
    let (c_config_arms, c_input_arms, c_output_arms) =
        generate_c_match_arms(&numeric_vars, &inputs, &outputs);
    let (default_vars_vec, default_vars_json_pairs) = generate_default_vars(&req.variables);

    let process_body = generate_process_body(req.plugin_type, &inputs, &outputs, &internals);
    let c_process_body = generate_c_process_body(req.plugin_type, &inputs, &outputs, &internals);

    let req_input_json_raw = serde_json::to_string(&req.behavior.required_input_ports)
        .unwrap_or_else(|_| "[]".to_string());
    let req_output_json_raw = serde_json::to_string(&req.behavior.required_output_ports)
        .unwrap_or_else(|_| "[]".to_string());
    let req_input_json = format!("{req_input_json_raw:?}");
    let req_output_json = format!("{req_output_json_raw:?}");

    let mut replacements: Vec<(&str, String)> = vec![
        ("PLUGIN_NAME", plugin_name.clone()),
        ("PLUGIN_KIND", kind.clone()),
        ("DESCRIPTION", req.description.clone()),
        ("PACKAGE_NAME", package_name.clone()),
        ("LIBRARY_NAME", format!("lib{package_name}.so")),
        ("INPUT_ARRAY", input_array),
        ("OUTPUT_ARRAY", output_array),
        ("INTERNAL_ARRAY", internal_array),
        ("PLUGIN_TYPE_VARIANT", req.plugin_type.variant().to_string()),
        ("PLUGIN_TYPE_STR", req.plugin_type.as_str().to_string()),
        (
            "LOADS_STARTED",
            if req.behavior.autostart {
                "true"
            } else {
                "false"
            }
            .to_string(),
        ),
        (
            "SUPPORTS_START_STOP",
            if req.behavior.supports_start_stop {
                "true"
            } else {
                "false"
            }
            .to_string(),
        ),
        (
            "SUPPORTS_RESTART",
            if req.behavior.supports_restart {
                "true"
            } else {
                "false"
            }
            .to_string(),
        ),
        (
            "SUPPORTS_APPLY",
            if req.behavior.supports_apply {
                "true"
            } else {
                "false"
            }
            .to_string(),
        ),
        (
            "EXTERNAL_WINDOW",
            if req.behavior.external_window {
                "true"
            } else {
                "false"
            }
            .to_string(),
        ),
        (
            "STARTS_EXPANDED",
            if req.behavior.starts_expanded {
                "true"
            } else {
                "false"
            }
            .to_string(),
        ),
        ("REQ_INPUT_JSON", req_input_json),
        ("REQ_OUTPUT_JSON", req_output_json),
        ("REQ_INPUT_JSON_RAW", req_input_json_raw),
        ("REQ_OUTPUT_JSON_RAW", req_output_json_raw),
        ("STATE_FIELDS", state_fields),
        ("DEFAULT_FIELDS", default_fields),
        ("CONFIG_MATCH_ARMS", config_match_arms),
        ("INPUT_MATCH_ARMS", input_match_arms),
        ("OUTPUT_MATCH_ARMS", output_match_arms),
        ("INTERNAL_MATCH_ARMS", internal_match_arms),
        ("DEFAULT_VARS_VEC", default_vars_vec),
        ("DEFAULT_VARS_JSON_PAIRS", default_vars_json_pairs),
        ("PROCESS_BODY", process_body),
        ("C_STATE_FIELDS", c_state_fields),
        ("C_DEFAULT_FIELDS", c_default_fields),
        ("C_CONFIG_ARMS", c_config_arms),
        ("C_INPUT_ARMS", c_input_arms),
        ("C_OUTPUT_ARMS", c_output_arms),
        ("C_PROCESS_BODY", c_process_body),
        ("RUST_STRUCT", rust_struct),
        ("CORE_PREFIX", core_prefix.clone()),
        ("CORE_STATE_TAG", core_state_tag),
        ("CORE_STATE_TYPE", core_state_type),
        ("FFI_CORE_STRUCT", ffi_core_struct),
        ("CORE_HEADER_FILE", format!("{kind}.h")),
        (
            "BUILD_DEPS",
            if req.language == PluginLanguage::Rust {
                String::new()
            } else {
                "[build-dependencies]\ncc = \"1\"".to_string()
            },
        ),
    ];

    if req.language != PluginLanguage::Rust {
        if req.language == PluginLanguage::C {
            replacements.push(("CORE_SOURCE_FILE", format!("{kind}.c")));
            replacements.push(("CORE_BUILD_LIB", format!("{kind}_core")));
        } else {
            replacements.push(("CORE_SOURCE_FILE", format!("{kind}.cpp")));
            replacements.push(("CORE_BUILD_LIB", format!("{kind}_core")));
        }
    }

    create_plugin_files(&plugin_dir, &src_dir, req.language, &kind, &replacements)?;

    Ok(plugin_dir)
}

#[cfg(test)]
mod tests {
    use super::validation::strip_language_suffix;

    #[test]
    fn strip_language_suffix_removes_common_trailing_markers() {
        assert_eq!(
            strip_language_suffix("Example Plugin (C)"),
            "Example Plugin"
        );
        assert_eq!(
            strip_language_suffix("Example Plugin (C++)"),
            "Example Plugin"
        );
        assert_eq!(
            strip_language_suffix("Example Plugin (Rust)"),
            "Example Plugin"
        );
        assert_eq!(
            strip_language_suffix("example-plugin-cpp"),
            "example-plugin"
        );
        assert_eq!(
            strip_language_suffix("example_plugin_rust"),
            "example_plugin"
        );
        assert_eq!(strip_language_suffix("example plugin c"), "example plugin");
    }

    #[test]
    fn strip_language_suffix_keeps_regular_names() {
        assert_eq!(
            strip_language_suffix("Hindmarsh Rose v2"),
            "Hindmarsh Rose v2"
        );
        assert_eq!(
            strip_language_suffix("Electrical Synapse"),
            "Electrical Synapse"
        );
    }
}
