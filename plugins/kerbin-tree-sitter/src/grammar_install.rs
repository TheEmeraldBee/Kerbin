use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::grammar::{GrammarDefinition, normalize_lang_name};

#[derive(thiserror::Error, Debug)]
pub enum GrammarInstallError {
    #[error("Missing install definition for language")]
    MissingInstallDefinition,

    #[error("Build succeeded but shared lib not found")]
    NoSharedLibrary,

    #[error("Missing expected build directory")]
    MissingBuildDir,

    #[error("{command} failed with status: {status} and stderr: {stderr}")]
    CommandFailed {
        command: &'static str,
        status: ExitStatus,
        stderr: String,
    },

    #[error("tree-sitter couldn't be found on your computer, is it installed?")]
    MissingTreeSitter,

    #[error(transparent)]
    IOError(#[from] io::Error),
}

/// Cleans up the grammar directory after a successful build, keeping only the
/// compiled library and query files.
fn cleanup_grammar_directory(dir: &Path, normalized_name: &str) -> io::Result<()> {
    let essential_files: Vec<String> = vec![
        format!("{}.so", normalized_name),
        format!("{}.dll", normalized_name),
        format!("{}.dylib", normalized_name),
    ];

    let query_dir_name = "queries";
    let query_dir_path = dir.join(query_dir_name);

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");

        if path.is_dir() {
            if file_name != query_dir_name {
                fs::remove_dir_all(path)?;
            }
        } else if !essential_files.contains(&file_name.to_string()) {
            fs::remove_file(path)?;
        }
    }

    let src_queries = dir.join("src").join(query_dir_name);
    if src_queries.exists() && src_queries.is_dir() {
        if query_dir_path.exists() {
            fs::remove_dir_all(&query_dir_path).ok();
        }
        fs::rename(&src_queries, &query_dir_path)?;
        fs::remove_dir(dir.join("src")).ok();
    }

    Ok(())
}

/// Gets all possible variants of a name with -, _, and .
fn get_name_variants(name: &str) -> Vec<String> {
    let mut variants = vec![name.to_string()];

    if name.contains('-') {
        variants.push(name.replace('-', "_"));
        variants.push(name.replace('-', "."));
    }
    if name.contains('_') {
        variants.push(name.replace('_', "-"));
        variants.push(name.replace('_', "."));
    }
    if name.contains('.') {
        variants.push(name.replace('.', "-"));
        variants.push(name.replace('.', "_"));
    }

    variants.sort();
    variants.dedup();
    variants
}

/// Installs a language based on the install config
/// Installs to the config's runtime/grammars path with normalized directory name
pub fn install_language(
    base_path: PathBuf,
    mut def: GrammarDefinition,
) -> Result<(), GrammarInstallError> {
    let Some(install_def) = def.install.take() else {
        return Err(GrammarInstallError::MissingInstallDefinition);
    };

    let now_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    let repo_name = install_def
        .url
        .split('/')
        .next_back()
        .unwrap_or(&def.name)
        .replace(".git", "");

    // Create atomic build directory using nanosecond timestamp
    let build_root = base_path.join(".build").join(now_nanos.to_string());
    let repo_clone_dir = build_root.join(&repo_name);

    let build_dir = install_def
        .sub_dir
        .as_ref()
        .map(|sub| repo_clone_dir.join(sub))
        .unwrap_or_else(|| repo_clone_dir.clone());

    // Use original name for directory, but normalized name for the .so file
    let normalized_name = normalize_lang_name(&def.name);
    let final_grammar_dir = base_path.join(format!("tree-sitter-{}", def.name));
    let temp_final_dir = base_path.join(format!("tree-sitter-{}-{}", def.name, now_nanos));

    let result: Result<(), GrammarInstallError> = (|| {
        // Ensure the build root directory exists
        fs::create_dir_all(&build_root)?;

        let output = Command::new("git")
            .arg("clone")
            .arg("--depth")
            .arg("1")
            .arg(&install_def.url)
            .arg(&repo_clone_dir)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GrammarInstallError::CommandFailed {
                command: "git clone",
                status: output.status,
                stderr: stderr.to_string(),
            });
        }

        if !build_dir.exists() {
            return Err(GrammarInstallError::MissingBuildDir);
        }

        let build_output = Command::new("tree-sitter")
            .arg("build")
            .current_dir(&build_dir)
            .output()?;

        if !build_output.status.success() {
            let stderr = String::from_utf8_lossy(&build_output.stderr);
            return Err(GrammarInstallError::CommandFailed {
                command: "tree-sitter build",
                status: build_output.status,
                stderr: stderr.to_string(),
            });
        }

        // Get the build name (might have -, _, or .)
        let build_name = install_def.build_name.clone().unwrap_or(def.name.clone());

        // Try all variants of the build name
        let build_name_variants = get_name_variants(&build_name);

        let mut found_lib = None;

        for variant in &build_name_variants {
            for ext in &["so", "dll", "dylib"] {
                let filename = format!("{}.{}", variant, ext);
                let path = build_dir.join(&filename);
                if path.exists() {
                    found_lib = Some((filename, ext.to_string()));
                    break;
                }
            }
            if found_lib.is_some() {
                break;
            }
        }

        let (initial_compiled_lib_name, ext) =
            found_lib.ok_or_else(|| GrammarInstallError::NoSharedLibrary)?;

        // Always use normalized name for the final library
        let compiled_lib_name = format!("{}.{}", normalized_name, ext);

        fs::create_dir_all(&temp_final_dir)?;

        // Copy (don't move) the library so tree-sitter can still validate it
        fs::copy(
            build_dir.join(&initial_compiled_lib_name),
            temp_final_dir.join(&compiled_lib_name),
        )?;

        let final_query_dir = temp_final_dir.join("queries");
        let source_query_dirs = [build_dir.join("queries"), build_dir.join("src/queries")];

        source_query_dirs.iter().any(|source_dir| {
            if source_dir.exists() {
                if final_query_dir.exists() {
                    fs::remove_dir_all(&final_query_dir).ok();
                }
                fs::rename(source_dir, &final_query_dir).is_ok()
            } else {
                false
            }
        });

        cleanup_grammar_directory(&temp_final_dir, &normalized_name)?;

        // Atomic rename to final location
        if final_grammar_dir.exists() {
            fs::remove_dir_all(&final_grammar_dir)?;
        }
        fs::rename(&temp_final_dir, &final_grammar_dir)?;

        Ok(())
    })();

    // Clean up temp final dir if it still exists (on error)
    if temp_final_dir.exists() {
        fs::remove_dir_all(&temp_final_dir).ok();
    }

    // Always clean up the atomic build directory after installation (success or failure)
    if build_root.exists() {
        fs::remove_dir_all(&build_root).ok();
    }

    result
}
