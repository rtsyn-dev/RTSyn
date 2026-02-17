use crate::plugin_creator::validation::{PluginKindType, PluginVariable};
use std::fs;
use std::path::{Path, PathBuf};

pub fn plugin_templates_dir() -> Result<PathBuf, String> {
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

pub fn render_to_file(
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

pub fn generate_process_body(
    plugin_type: PluginKindType,
    inputs: &[String],
    outputs: &[String],
    internals: &[String],
) -> String {
    if plugin_type == PluginKindType::Computational {
        if internals.is_empty() {
            "        let _ = period_seconds;\n        // TODO: computational plugin skeleton (add internal state vars and equations)."
                .to_string()
        } else {
            let state_init = internals
                .iter()
                .map(|name| format!("self.{name}"))
                .collect::<Vec<_>>()
                .join(", ");
            let state_writeback = internals
                .iter()
                .enumerate()
                .map(|(idx, name)| format!("        self.{name} = state[{idx}];\n"))
                .collect::<String>();
            let output_sync = match (outputs.first(), internals.first()) {
                (Some(out), Some(internal)) => format!("        self.{out} = self.{internal};\n"),
                _ => String::new(),
            };
            format!(
                "        let dt = if period_seconds.is_finite() && period_seconds > 0.0 {{ period_seconds }} else {{ 1e-3 }};\n        let mut state = [{state_init}];\n        rk4_step(&mut state, dt, |_st, der| {{\n            for d in der.iter_mut() {{\n                *d = 0.0;\n            }}\n            // TODO: set derivatives from your model equations.\n        }});\n{state_writeback}{output_sync}"
            )
        }
    } else {
        match (inputs.first(), outputs.first()) {
            (Some(i), Some(o)) => {
                format!("        let _ = period_seconds;\n        self.{o} = self.{i};")
            }
            _ => "        let _ = period_seconds;\n        // TODO: implement plugin dynamics"
                .to_string(),
        }
    }
}

pub fn generate_c_process_body(
    plugin_type: PluginKindType,
    inputs: &[String],
    outputs: &[String],
    internals: &[String],
) -> String {
    if plugin_type == PluginKindType::Computational {
        if internals.is_empty() {
            "    (void)s;\n    (void)period_seconds;\n    // TODO: add internal state vars and model equations.".to_string()
        } else {
            let state_init = internals
                .iter()
                .map(|name| format!("s->{name}"))
                .collect::<Vec<_>>()
                .join(", ");
            let state_writeback = internals
                .iter()
                .enumerate()
                .map(|(idx, name)| format!("    s->{name} = state[{idx}];\n"))
                .collect::<String>();
            let output_sync = match (outputs.first(), internals.first()) {
                (Some(out), Some(internal)) => format!("    s->{out} = s->{internal};\n"),
                _ => String::new(),
            };
            format!(
                "    double dt = (period_seconds > 0.0 && period_seconds < 1e9) ? period_seconds : 1e-3;\n    (void)dt;\n    double state[] = {{{state_init}}};\n    // TODO: define deriv_fn(state, deriv, n, user_data) and call:\n    // rtsyn_plugin_rk4_step_n(state, {n}, dt, deriv_fn, s);\n{state_writeback}{output_sync}",
                n = internals.len()
            )
        }
    } else {
        match (inputs.first(), outputs.first()) {
            (Some(i), Some(o)) => format!("    s->{o} = s->{i};"),
            _ => "    (void)s;".to_string(),
        }
    }
}

pub fn generate_state_fields(
    inputs: &[String],
    outputs: &[String],
    internals: &[String],
    numeric_vars: &[&PluginVariable],
) -> (String, String) {
    let mut state_fields = String::new();
    let mut default_fields = String::new();

    for name in inputs.iter().chain(outputs.iter()).chain(internals.iter()) {
        state_fields.push_str(&format!("    {name}: f64,\n"));
        default_fields.push_str(&format!("            {name}: 0.0,\n"));
    }

    for var in numeric_vars {
        state_fields.push_str(&format!("    {}: f64,\n", var.name));
        default_fields.push_str(&format!(
            "            {}: {},\n",
            var.name,
            var.as_numeric_f64().unwrap_or(0.0)
        ));
    }

    (state_fields, default_fields)
}

pub fn generate_c_state_fields(
    inputs: &[String],
    outputs: &[String],
    internals: &[String],
    numeric_vars: &[&PluginVariable],
) -> (String, String) {
    let mut c_state_fields = String::new();
    let mut c_default_fields = String::new();

    for name in inputs.iter().chain(outputs.iter()).chain(internals.iter()) {
        c_state_fields.push_str(&format!("    double {name};\n"));
        c_default_fields.push_str(&format!("    s->{name} = 0.0;\n"));
    }

    for var in numeric_vars {
        c_state_fields.push_str(&format!("    double {};\n", var.name));
        c_default_fields.push_str(&format!(
            "    s->{} = {};\n",
            var.name,
            var.as_numeric_f64().unwrap_or(0.0)
        ));
    }

    (c_state_fields, c_default_fields)
}

pub fn generate_match_arms(
    numeric_vars: &[&PluginVariable],
    inputs: &[String],
    outputs: &[String],
    internals: &[String],
) -> (String, String, String, String) {
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

    (
        config_match_arms,
        input_match_arms,
        output_match_arms,
        internal_match_arms,
    )
}

pub fn generate_c_match_arms(
    numeric_vars: &[&PluginVariable],
    inputs: &[String],
    outputs: &[String],
) -> (String, String, String) {
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

    (c_config_arms, c_input_arms, c_output_arms)
}

pub fn generate_default_vars(variables: &[PluginVariable]) -> (String, String) {
    let default_vars_vec = variables
        .iter()
        .map(|v| format!("            (\"{}\", {}),\n", v.name, v.rust_value_expr()))
        .collect::<String>();
    let default_vars_json_pairs = variables
        .iter()
        .map(|v| {
            format!(
                "                [{:?}, {}],\n",
                v.name,
                serde_json::to_string(&v.default_json_value())
                    .unwrap_or_else(|_| "null".to_string())
            )
        })
        .collect::<String>();

    (default_vars_vec, default_vars_json_pairs)
}
