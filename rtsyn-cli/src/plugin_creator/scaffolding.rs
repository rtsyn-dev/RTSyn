use crate::plugin_creator::templates::{plugin_templates_dir, render_to_file};
use crate::plugin_creator::validation::PluginLanguage;
use std::fs;
use std::path::{Path, PathBuf};

pub fn create_plugin_structure(
    base_dir: &Path,
    folder_base: &str,
) -> Result<(PathBuf, PathBuf), String> {
    fs::create_dir_all(base_dir).map_err(|e| {
        format!(
            "Failed to create base directory {}: {e}",
            base_dir.display()
        )
    })?;

    let plugin_dir = unique_dir(base_dir, folder_base);
    let src_dir = plugin_dir.join("src");

    fs::create_dir_all(&src_dir)
        .map_err(|e| format!("Failed to create src directory {}: {e}", src_dir.display()))?;

    Ok((plugin_dir, src_dir))
}

pub fn create_plugin_files(
    plugin_dir: &Path,
    src_dir: &Path,
    language: PluginLanguage,
    kind: &str,
    replacements: &[(&str, String)],
) -> Result<(), String> {
    let tpl_dir = plugin_templates_dir()?;

    render_to_file(
        &tpl_dir.join("plugin.toml.tpl"),
        &plugin_dir.join("plugin.toml"),
        replacements,
    )?;
    render_to_file(
        &tpl_dir.join("Cargo.toml.tpl"),
        &plugin_dir.join("Cargo.toml"),
        replacements,
    )?;

    if language == PluginLanguage::Rust {
        render_to_file(
            &tpl_dir.join("rust_lib.rs.tpl"),
            &src_dir.join("lib.rs"),
            replacements,
        )?;
    } else {
        render_to_file(
            &tpl_dir.join("ffi_wrapper.rs.tpl"),
            &src_dir.join("lib.rs"),
            replacements,
        )?;
        render_to_file(
            &tpl_dir.join("c_core.h.tpl"),
            &src_dir.join(format!("{kind}.h")),
            replacements,
        )?;

        if language == PluginLanguage::C {
            render_to_file(
                &tpl_dir.join("c_core.c.tpl"),
                &src_dir.join(format!("{kind}.c")),
                replacements,
            )?;
            render_to_file(
                &tpl_dir.join("build_c.rs.tpl"),
                &plugin_dir.join("build.rs"),
                replacements,
            )?;
        } else {
            render_to_file(
                &tpl_dir.join("cpp_core.cpp.tpl"),
                &src_dir.join(format!("{kind}.cpp")),
                replacements,
            )?;
            render_to_file(
                &tpl_dir.join("build_cpp.rs.tpl"),
                &plugin_dir.join("build.rs"),
                replacements,
            )?;
        }
    }

    Ok(())
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
