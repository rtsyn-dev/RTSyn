use serde_json::{Number, Value};
use std::collections::HashSet;

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
    pub fn new(
        name: &str,
        field_type: FieldType,
        default_text: Option<&str>,
    ) -> Result<Self, String> {
        let clean_name = to_snake_case(name);
        if clean_name.is_empty() {
            return Err("Variable name cannot be empty".to_string());
        }
        let normalized_default = normalize_default(
            field_type,
            default_text.unwrap_or(field_type.default_text()),
        )?;
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

fn format_float_value(v: f64) -> String {
    if v.fract().abs() < f64::EPSILON {
        format!("{v:.1}")
    } else {
        v.to_string()
    }
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
            let normalized = candidate.replace(',', ".");
            let v = normalized
                .parse::<f64>()
                .map_err(|_| format!("Invalid float default '{normalized}'"))?;
            Ok(format_float_value(v))
        }
        FieldType::Bool => match candidate.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "y" => Ok("true".to_string()),
            "false" | "0" | "no" | "n" => Ok("false".to_string()),
            _ => Err(format!(
                "Invalid bool default '{candidate}' (use true/false)"
            )),
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

pub fn strip_language_suffix(input: &str) -> String {
    let mut name = input.trim().to_string();
    loop {
        let lower = name.to_ascii_lowercase();
        let mut changed = false;
        for suffix in [
            " (rust)", " (c++)", " (cpp)", " (c)", "-rust", "-cpp", "-c", "_rust", "_cpp", "_c",
            " rust", " c++", " cpp", " c",
        ] {
            if lower.ends_with(suffix) {
                let new_len = name.len().saturating_sub(suffix.len());
                name = name[..new_len]
                    .trim_end_matches([' ', '-', '_', '(', ')'])
                    .trim()
                    .to_string();
                changed = true;
                break;
            }
        }
        if !changed {
            break;
        }
    }
    name
}

pub fn to_kebab_case(input: &str) -> String {
    to_snake_case(input).replace('_', "-")
}

pub fn to_snake_case(input: &str) -> String {
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

pub fn to_pascal_case(input: &str) -> String {
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

pub fn sanitize_names(items: &[String]) -> Result<Vec<String>, String> {
    sanitize_unique_names(items, "field")
}

pub fn sanitize_unique_names(items: &[String], category: &str) -> Result<Vec<String>, String> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for item in items {
        let sanitized = to_snake_case(item);
        if sanitized.is_empty() {
            continue;
        }
        if !seen.insert(sanitized.clone()) {
            return Err(format!("Duplicate {category} name '{sanitized}'"));
        }
        result.push(sanitized);
    }
    Ok(result)
}

pub fn ensure_unique_field_names(names: &[String], category: &str) -> Result<(), String> {
    let mut seen = HashSet::new();
    for name in names {
        if !seen.insert(name.clone()) {
            return Err(format!("Duplicate {category} name '{name}'"));
        }
    }
    Ok(())
}

pub struct FieldNameAllocator {
    used: HashSet<String>,
}

impl FieldNameAllocator {
    pub fn new() -> Self {
        Self {
            used: HashSet::new(),
        }
    }

    pub fn allocate(&mut self, base: &str, suffix: &str) -> String {
        if self.used.insert(base.to_string()) {
            return base.to_string();
        }
        let mut idx = 1;
        loop {
            let candidate = if idx == 1 {
                format!("{base}{suffix}")
            } else {
                format!("{base}{suffix}{idx}")
            };
            if self.used.insert(candidate.clone()) {
                return candidate;
            }
            idx += 1;
        }
    }

    pub fn allocate_series(&mut self, bases: Vec<String>, suffix: &str) -> Vec<String> {
        bases
            .into_iter()
            .map(|base| self.allocate(&base, suffix))
            .collect()
    }
}

pub fn uniquify_plugin_variable_names(vars: &mut [PluginVariable], duplicate_suffix: &str) {
    let mut seen = HashSet::new();
    for var in vars.iter_mut() {
        let base = var.name.clone();
        if seen.insert(base.clone()) {
            continue;
        }
        let mut candidate = format!("{base}{duplicate_suffix}");
        let mut idx = 2;
        while !seen.insert(candidate.clone()) {
            candidate = format!("{base}{duplicate_suffix}{idx}");
            idx += 1;
        }
        var.name = candidate;
    }
}

pub fn quote_array(items: &[String]) -> String {
    items
        .iter()
        .map(|s| format!("{s:?}"))
        .collect::<Vec<_>>()
        .join(", ")
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
