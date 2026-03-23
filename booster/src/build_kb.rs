use std::path::{Path, PathBuf};

pub struct Plugin {
    pub name: String,
    pub rust_name: String,
    pub source: PluginSource,
}

pub enum PluginSource {
    Core,
    Git(String),
    Path(PathBuf),
}

pub struct ParseError {
    pub line: usize,
    pub message: String,
}

pub fn parse(content: &str) -> Result<Vec<Plugin>, Vec<ParseError>> {
    let mut plugins = Vec::new();
    let mut errors = Vec::new();

    for (i, raw_line) in content.lines().enumerate() {
        let line_num = i + 1;
        let trimmed = raw_line.split('#').next().unwrap_or("").trim();

        if trimmed.is_empty() {
            continue;
        }

        let tokens: Vec<&str> = trimmed
            .split_whitespace()
            .collect();

        if tokens[0] != "plugin" {
            errors.push(ParseError {
                line: line_num,
                message: format!("unexpected token '{}'; expected 'plugin'", tokens[0]),
            });
            continue;
        }

        if tokens.len() < 3 {
            errors.push(ParseError {
                line: line_num,
                message: "plugin declaration requires a type and name (e.g. 'plugin core kerbin-lsp')".to_string(),
            });
            continue;
        }

        let plugin_type = tokens[1];
        match plugin_type {
            "core" => {
                let name = tokens[2].to_string();
                let rust_name = name.replace('-', "_");
                plugins.push(Plugin { name, rust_name, source: PluginSource::Core });
            }
            "git" => {
                if tokens.len() < 4 {
                    errors.push(ParseError {
                        line: line_num,
                        message: "git plugin requires: plugin git <name> <url>".to_string(),
                    });
                    continue;
                }
                let name = tokens[2].to_string();
                let url = tokens[3].to_string();
                let rust_name = name.replace('-', "_");
                plugins.push(Plugin { name, rust_name, source: PluginSource::Git(url) });
            }
            "path" => {
                let path = PathBuf::from(tokens[2]);
                let cargo_toml_path = path.join("Cargo.toml");
                let cargo_content = match std::fs::read_to_string(&cargo_toml_path) {
                    Ok(c) => c,
                    Err(e) => {
                        errors.push(ParseError {
                            line: line_num,
                            message: format!(
                                "cannot read '{}': {}",
                                cargo_toml_path.display(),
                                e
                            ),
                        });
                        continue;
                    }
                };
                match extract_cargo_name(&cargo_content) {
                    Some(name) => {
                        let rust_name = name.replace('-', "_");
                        plugins.push(Plugin { name, rust_name, source: PluginSource::Path(path) });
                    }
                    None => {
                        errors.push(ParseError {
                            line: line_num,
                            message: format!(
                                "could not find package name in '{}'",
                                cargo_toml_path.display()
                            ),
                        });
                    }
                }
            }
            other => {
                errors.push(ParseError {
                    line: line_num,
                    message: format!(
                        "unknown plugin type '{}'; expected 'core', 'git', or 'path'",
                        other
                    ),
                });
            }
        }
    }

    if errors.is_empty() {
        Ok(plugins)
    } else {
        Err(errors)
    }
}

fn extract_cargo_name(content: &str) -> Option<String> {
    let mut in_package = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[package]" {
            in_package = true;
            continue;
        }
        if in_package && trimmed.starts_with('[') {
            break;
        }
        if in_package
            && let Some(rest) = trimmed.strip_prefix("name") {
                let rest = rest.trim_start();
                if let Some(rest) = rest.strip_prefix('=') {
                    let value = rest.trim().trim_matches('"');
                    if !value.is_empty() {
                        return Some(value.to_string());
                    }
                }
            }
    }
    None
}

/// Compute a relative path from `from_dir` to `to`, using canonicalization
/// to resolve any `..` or `.` components in either path.
fn relative_path(from_dir: &Path, to: &Path) -> PathBuf {
    let from = from_dir.canonicalize().unwrap_or_else(|_| from_dir.to_path_buf());
    let to = to.canonicalize().unwrap_or_else(|_| to.to_path_buf());

    let from_components: Vec<_> = from.components().collect();
    let to_components: Vec<_> = to.components().collect();

    let common = from_components
        .iter()
        .zip(to_components.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let mut result = PathBuf::new();
    for _ in 0..(from_components.len() - common) {
        result.push("..");
    }
    for component in &to_components[common..] {
        result.push(component);
    }
    result
}

pub fn generate_cargo_toml(plugins: &[Plugin], build_dir: &Path, output_dir: &Path) -> String {
    let mut toml = String::new();
    toml.push_str("[package]\n");
    toml.push_str("name = \"config\"\n");
    toml.push_str("version = \"0.0.0\"\n");
    toml.push_str("edition = \"2024\"\n");
    toml.push_str("\n[dependencies]\n");
    toml.push_str(&format!(
        "kerbin-core = {{ path = \"{}\" }}\n",
        relative_path(output_dir, &build_dir.join("kerbin-core")).display()
    ));
    for plugin in plugins {
        match &plugin.source {
            PluginSource::Core => {
                toml.push_str(&format!(
                    "{} = {{ path = \"{}\" }}\n",
                    plugin.name,
                    relative_path(
                        output_dir,
                        &build_dir.join("plugins").join(&plugin.name)
                    )
                    .display()
                ));
            }
            PluginSource::Git(url) => {
                toml.push_str(&format!("{} = {{ git = \"{}\" }}\n", plugin.name, url));
            }
            PluginSource::Path(path) => {
                let rel = if path.is_absolute() {
                    relative_path(output_dir, path)
                } else {
                    path.clone()
                };
                toml.push_str(&format!(
                    "{} = {{ path = \"{}\" }}\n",
                    plugin.name,
                    rel.display()
                ));
            }
        }
    }
    // Declare as a workspace root so it can be checked independently of the build workspace
    toml.push_str("\n[workspace]\n");
    toml
}

pub fn generate_lib_rs(plugins: &[Plugin]) -> String {
    let mut lib = String::new();
    lib.push_str("// AUTO-GENERATED by booster from build.kb — do not edit manually\n");
    lib.push_str("use kerbin_core::State;\n");
    lib.push('\n');
    lib.push_str("pub async fn init(state: &mut State) {\n");
    for plugin in plugins {
        lib.push_str(&format!("    {}::init(state).await;\n", plugin.rust_name));
    }
    lib.push_str("}\n");
    lib
}
