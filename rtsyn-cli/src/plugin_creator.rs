use serde_json::{Number, Value};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginLanguage {
    Rust,
    C,
    Cpp,
}

impl PluginLanguage {
    pub fn parse(input: &str) -> Result<Self, String> {
        match input.trim().to_ascii_lowercase().as_str() {
            "rust" => Ok(Self::Rust),
            "c" => Ok(Self::C),
            "cpp" | "c++" => Ok(Self::Cpp),
            other => Err(format!(
                "Invalid language '{other}'. Valid values: rust, c, cpp"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::C => "c",
            Self::Cpp => "cpp",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginKindType {
    Standard,
    Device,
    Computational,
}

impl PluginKindType {
    pub fn parse(input: &str) -> Result<Self, String> {
        match input.trim().to_ascii_lowercase().as_str() {
            "standard" => Ok(Self::Standard),
            "device" => Ok(Self::Device),
            "computational" => Ok(Self::Computational),
            other => Err(format!(
                "Invalid plugin type '{other}'. Valid values: standard, device, computational"
            )),
        }
    }

    pub fn variant(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Device => "Device",
            Self::Computational => "Computational",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Device => "device",
            Self::Computational => "computational",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    Float,
    Bool,
    Int,
    File,
}

impl FieldType {
    pub fn parse(input: &str) -> Result<Self, String> {
        match input.trim().to_ascii_lowercase().as_str() {
            "float" | "f64" | "f32" => Ok(Self::Float),
            "bool" | "boolean" => Ok(Self::Bool),
            "int" | "i64" | "i32" | "u64" | "u32" => Ok(Self::Int),
            "file" | "path" | "string" => Ok(Self::File),
            other => Err(format!(
                "Invalid field type '{other}'. Valid values: float, bool, int, file"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Float => "float",
            Self::Bool => "bool",
            Self::Int => "int",
            Self::File => "file",
        }
    }

    pub fn default_text(self) -> &'static str {
        match self {
            Self::Float => "0.0",
            Self::Bool => "false",
            Self::Int => "0",
            Self::File => "",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PluginVariable {
    pub name: String,
    pub field_type: FieldType,
    pub default_text: String,
}

impl PluginVariable {
    pub fn new(name: &str, field_type: FieldType, default_text: Option<&str>) -> Result<Self, String> {
        let clean_name = to_snake_case(name);
        if clean_name.is_empty() {
            return Err("Variable name cannot be empty".to_string());
        }
        let normalized_default = normalize_default(field_type, default_text.unwrap_or(field_type.default_text()))?;
        Ok(Self {
            name: clean_name,
            field_type,
            default_text: normalized_default,
        })
    }

    pub fn default_json_value(&self) -> Value {
        match self.field_type {
            FieldType::Float => self
                .default_text
                .parse::<f64>()
                .ok()
                .and_then(Number::from_f64)
                .map(Value::Number)
                .unwrap_or_else(|| Value::from(0.0_f64)),
            FieldType::Bool => Value::Bool(self.default_text.eq_ignore_ascii_case("true")),
            FieldType::Int => Value::from(self.default_text.parse::<i64>().unwrap_or(0_i64)),
            FieldType::File => Value::String(self.default_text.clone()),
        }
    }

    pub fn rust_value_expr(&self) -> String {
        match self.field_type {
            FieldType::Float => {
                let v = self.default_text.parse::<f64>().unwrap_or(0.0);
                format!("Value::from({v}_f64)")
            }
            FieldType::Bool => {
                if self.default_text.eq_ignore_ascii_case("true") {
                    "Value::from(true)".to_string()
                } else {
                    "Value::from(false)".to_string()
                }
            }
            FieldType::Int => {
                let v = self.default_text.parse::<i64>().unwrap_or(0_i64);
                format!("Value::from({v}_i64)")
            }
            FieldType::File => format!("Value::from({:?})", self.default_text),
        }
    }

    pub fn as_numeric_f64(&self) -> Option<f64> {
        match self.field_type {
            FieldType::Float => self.default_text.parse::<f64>().ok(),
            FieldType::Int => self.default_text.parse::<i64>().ok().map(|v| v as f64),
            FieldType::Bool | FieldType::File => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreatorBehavior {
    pub autostart: bool,
    pub supports_start_stop: bool,
    pub supports_restart: bool,
    pub supports_apply: bool,
    pub external_window: bool,
    pub starts_expanded: bool,
    pub required_input_ports: Vec<String>,
    pub required_output_ports: Vec<String>,
}

impl Default for CreatorBehavior {
    fn default() -> Self {
        Self {
            autostart: false,
            supports_start_stop: true,
            supports_restart: true,
            supports_apply: false,
            external_window: false,
            starts_expanded: true,
            required_input_ports: Vec::new(),
            required_output_ports: Vec::new(),
        }
    }
}

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

pub fn parse_variable_line(line: &str) -> Result<PluginVariable, String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err("Variable spec line cannot be empty".to_string());
    }
    let (left, default_part) = match trimmed.split_once('=') {
        Some((lhs, rhs)) => (lhs.trim(), Some(rhs.trim())),
        None => (trimmed, None),
    };
    let (name, ty) = match left.split_once(':') {
        Some((n, t)) => (n.trim(), t.trim()),
        None => (left, "float"),
    };
    let field_type = FieldType::parse(ty)?;
    PluginVariable::new(name, field_type, default_part)
}

pub fn create_plugin(req: &PluginCreateRequest) -> Result<PathBuf, String> {
    let plugin_name = req.name.trim();
    if plugin_name.is_empty() {
        return Err("Plugin name cannot be empty".to_string());
    }

    let kind = to_snake_case(plugin_name);
    let slug = to_kebab_case(plugin_name);
    let folder_base = match req.language {
        PluginLanguage::C => format!("{slug}_c"),
        PluginLanguage::Cpp => format!("{slug}_cpp"),
        PluginLanguage::Rust => format!("{slug}_rust"),
    };
    let package_name = folder_base.clone();
    let rust_struct = to_pascal_case(&kind);
    let core_prefix = kind.clone();
    let core_state_tag = format!("{kind}_state");
    let core_state_type = format!("{kind}_state_t");
    let ffi_core_struct = format!("{rust_struct}Core");

    fs::create_dir_all(&req.base_dir)
        .map_err(|e| format!("Failed to create base directory {}: {e}", req.base_dir.display()))?;
    let plugin_dir = unique_dir(&req.base_dir, &folder_base);
    let src_dir = plugin_dir.join("src");
    fs::create_dir_all(&src_dir)
        .map_err(|e| format!("Failed to create src directory {}: {e}", src_dir.display()))?;

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

    let mut state_fields = String::new();
    let mut default_fields = String::new();
    let mut c_state_fields = String::new();
    let mut c_default_fields = String::new();

    for name in inputs.iter().chain(outputs.iter()).chain(internals.iter()) {
        state_fields.push_str(&format!("    {name}: f64,\n"));
        default_fields.push_str(&format!("            {name}: 0.0,\n"));
        c_state_fields.push_str(&format!("    double {name};\n"));
        c_default_fields.push_str(&format!("    s->{name} = 0.0;\n"));
    }

    for var in &numeric_vars {
        state_fields.push_str(&format!("    {}: f64,\n", var.name));
        default_fields.push_str(&format!(
            "            {}: {},\n",
            var.name,
            var.as_numeric_f64().unwrap_or(0.0)
        ));
        c_state_fields.push_str(&format!("    double {};\n", var.name));
        c_default_fields.push_str(&format!(
            "    s->{} = {};\n",
            var.name,
            var.as_numeric_f64().unwrap_or(0.0)
        ));
    }

    let config_match_arms = numeric_vars
        .iter()
        .map(|v| format!("            \"{}\" => self.{} = v,\n", v.name, v.name))
        .collect::<String>();
    let input_match_arms = inputs
        .iter()
        .map(|v| format!("            \"{v}\" => self.{v} = value,\n"))
        .collect::<String>();
    let output_match_arms = outputs
        .iter()
        .map(|v| format!("            \"{v}\" => self.{v},\n"))
        .collect::<String>();
    let internal_match_arms = internals
        .iter()
        .map(|v| format!("            \"{v}\" => Some(self.{v}),\n"))
        .collect::<String>();
    let default_vars_vec = req
        .variables
        .iter()
        .map(|v| format!("            (\"{}\", {}),\n", v.name, v.rust_value_expr()))
        .collect::<String>();
    let default_vars_json_pairs = req
        .variables
        .iter()
        .map(|v| {
            format!(
                "                [{:?}, {}],\n",
                v.name,
                serde_json::to_string(&v.default_json_value()).unwrap_or_else(|_| "null".to_string())
            )
        })
        .collect::<String>();
    let c_config_arms = numeric_vars
        .iter()
        .map(|v| {
            format!(
                "    if (key_eq(key, len, \"{}\")) {{ s->{} = value; return; }}\n",
                v.name, v.name
            )
        })
        .collect::<String>();
    let c_input_arms = inputs
        .iter()
        .map(|v| format!("    if (key_eq(name, len, \"{v}\")) {{ s->{v} = value; return; }}\n"))
        .collect::<String>();
    let c_output_arms = outputs
        .iter()
        .map(|v| format!("    if (key_eq(name, len, \"{v}\")) return s->{v};\n"))
        .collect::<String>();

    let process_body = match (inputs.first(), outputs.first()) {
        (Some(i), Some(o)) => format!("        let _ = period_seconds;\n        self.{o} = self.{i};"),
        _ => "        let _ = period_seconds;\n        // TODO: implement plugin dynamics".to_string(),
    };
    let c_process_body = match (inputs.first(), outputs.first()) {
        (Some(i), Some(o)) => format!("    s->{o} = s->{i};"),
        _ => "    (void)s;".to_string(),
    };

    let req_input_json_raw = serde_json::to_string(&req.behavior.required_input_ports)
        .unwrap_or_else(|_| "[]".to_string());
    let req_output_json_raw = serde_json::to_string(&req.behavior.required_output_ports)
        .unwrap_or_else(|_| "[]".to_string());
    let req_input_json = format!("{req_input_json_raw:?}");
    let req_output_json = format!("{req_output_json_raw:?}");

    let mut replacements: Vec<(&str, String)> = vec![
        ("PLUGIN_NAME", plugin_name.to_string()),
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
            if req.behavior.autostart { "true" } else { "false" }.to_string(),
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
            if req.behavior.supports_restart { "true" } else { "false" }.to_string(),
        ),
        (
            "SUPPORTS_APPLY",
            if req.behavior.supports_apply { "true" } else { "false" }.to_string(),
        ),
        (
            "EXTERNAL_WINDOW",
            if req.behavior.external_window { "true" } else { "false" }.to_string(),
        ),
        (
            "STARTS_EXPANDED",
            if req.behavior.starts_expanded { "true" } else { "false" }.to_string(),
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

    let tpl_dir = plugin_templates_dir()?;
    render_to_file(
        &tpl_dir.join("plugin.toml.tpl"),
        &plugin_dir.join("plugin.toml"),
        &replacements,
    )?;
    render_to_file(
        &tpl_dir.join("Cargo.toml.tpl"),
        &plugin_dir.join("Cargo.toml"),
        &replacements,
    )?;

    if req.language == PluginLanguage::Rust {
        render_to_file(
            &tpl_dir.join("rust_lib.rs.tpl"),
            &src_dir.join("lib.rs"),
            &replacements,
        )?;
    } else {
        render_to_file(
            &tpl_dir.join("ffi_wrapper.rs.tpl"),
            &src_dir.join("lib.rs"),
            &replacements,
        )?;
        render_to_file(
            &tpl_dir.join("c_core.h.tpl"),
            &src_dir.join(format!("{kind}.h")),
            &replacements,
        )?;
        if req.language == PluginLanguage::C {
            replacements.push(("CORE_SOURCE_FILE", format!("{kind}.c")));
            replacements.push(("CORE_BUILD_LIB", format!("{kind}_core_c")));
            render_to_file(
                &tpl_dir.join("c_core.c.tpl"),
                &src_dir.join(format!("{kind}.c")),
                &replacements,
            )?;
            render_to_file(
                &tpl_dir.join("build_c.rs.tpl"),
                &plugin_dir.join("build.rs"),
                &replacements,
            )?;
        } else {
            replacements.push(("CORE_SOURCE_FILE", format!("{kind}.cpp")));
            replacements.push(("CORE_BUILD_LIB", format!("{kind}_core_cpp")));
            render_to_file(
                &tpl_dir.join("cpp_core.cpp.tpl"),
                &src_dir.join(format!("{kind}.cpp")),
                &replacements,
            )?;
            render_to_file(
                &tpl_dir.join("build_cpp.rs.tpl"),
                &plugin_dir.join("build.rs"),
                &replacements,
            )?;
        }
    }

    Ok(plugin_dir)
}

pub fn normalize_default(field_type: FieldType, value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    let candidate = if trimmed.is_empty() {
        field_type.default_text()
    } else {
        trimmed
    };
    match field_type {
        FieldType::Float => {
            let v = candidate
                .parse::<f64>()
                .map_err(|_| format!("Invalid float default '{candidate}'"))?;
            Ok(v.to_string())
        }
        FieldType::Bool => match candidate.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "y" => Ok("true".to_string()),
            "false" | "0" | "no" | "n" => Ok("false".to_string()),
            _ => Err(format!("Invalid bool default '{candidate}' (use true/false)")),
        },
        FieldType::Int => {
            let v = candidate
                .parse::<i64>()
                .map_err(|_| format!("Invalid int default '{candidate}'"))?;
            Ok(v.to_string())
        }
        FieldType::File => Ok(candidate.to_string()),
    }
}

fn plugin_templates_dir() -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../rtsyn-plugin/templates"),
        manifest_dir.join("../rtsyn-plugin/templates"),
        PathBuf::from("rtsyn-plugin/templates"),
        PathBuf::from("../rtsyn-plugin/templates"),
    ];
    candidates
        .into_iter()
        .find(|p| p.is_dir())
        .ok_or_else(|| "Could not locate rtsyn-plugin templates directory".to_string())
}

fn render_to_file(
    template_path: &Path,
    output_path: &Path,
    replacements: &[(&str, String)],
) -> Result<(), String> {
    let mut template = fs::read_to_string(template_path)
        .map_err(|e| format!("Failed to read {}: {e}", template_path.display()))?;
    for (key, value) in replacements {
        let needle = format!("__{key}__");
        template = template.replace(&needle, value);
    }
    fs::write(output_path, template)
        .map_err(|e| format!("Failed to write {}: {e}", output_path.display()))
}

fn to_snake_case(input: &str) -> String {
    let mut out = String::new();
    let mut prev_sep = false;
    for ch in input.chars().flat_map(|c| c.to_lowercase()) {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
            prev_sep = false;
        } else if !prev_sep {
            out.push('_');
            prev_sep = true;
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "generated_plugin".to_string()
    } else {
        trimmed
    }
}

fn to_kebab_case(input: &str) -> String {
    to_snake_case(input).replace('_', "-")
}

fn to_pascal_case(input: &str) -> String {
    to_snake_case(input)
        .split('_')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut chars = p.chars();
            let first = chars.next().unwrap_or('x').to_ascii_uppercase();
            format!("{first}{}", chars.as_str())
        })
        .collect::<String>()
}

fn unique_dir(parent: &Path, base: &str) -> PathBuf {
    let first = parent.join(base);
    if !first.exists() {
        return first;
    }
    for idx in 1.. {
        let candidate = parent.join(format!("{base}_{idx}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    first
}

fn quote_array(items: &[String]) -> String {
    items
        .iter()
        .map(|s| format!("{s:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn sanitize_names(items: &[String]) -> Vec<String> {
    items
        .iter()
        .map(|s| to_snake_case(s))
        .filter(|s| !s.is_empty())
        .collect()
}
