use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
    time::{SystemTime, SystemTimeError, UNIX_EPOCH},
};

use crate::grammar::{GrammarDefinition, get_platform_extensions, normalize_lang_name};

#[derive(thiserror::Error, Debug)]
pub enum GrammarInstallError {
    #[error("Missing install definition for language")]
    MissingInstallDefinition,

    #[error("Missing expected build directory")]
    MissingBuildDir,

    #[error("{command} failed with status: {status} and stderr: {stderr}")]
    CommandFailed {
        command: &'static str,
        status: ExitStatus,
        stderr: String,
    },

    #[error("Grammar source missing: src/parser.c not found in {path}")]
    MissingSourceFiles { path: String },

    #[error("C compiler failed:\n{stderr}")]
    CompileFailed { stderr: String },

    #[error(transparent)]
    IOError(#[from] io::Error),

    #[error(transparent)]
    SystemTimeError(#[from] SystemTimeError),
}

fn cleanup_grammar_directory(dir: &Path, normalized_name: &str) -> io::Result<()> {
    let essential_files: Vec<String> = vec![
        format!("{}.so", normalized_name),
        format!("{}.dll", normalized_name),
        format!("{}.dylib", normalized_name),
        "package.json".to_string(),
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

fn run_compiler(mut cmd: Command) -> Result<(), GrammarInstallError> {
    let out = cmd.output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(GrammarInstallError::CompileFailed {
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

fn c_compiler() -> String {
    std::env::var("CC").unwrap_or_else(|_| {
        if cfg!(target_env = "msvc") {
            "cl".to_string()
        } else {
            "cc".to_string()
        }
    })
}

fn cxx_compiler() -> String {
    std::env::var("CXX").unwrap_or_else(|_| {
        if cfg!(target_env = "msvc") {
            "cl".to_string()
        } else {
            "c++".to_string()
        }
    })
}

fn add_shared_flags(cmd: &mut Command, output_path: &Path) {
    if cfg!(target_env = "msvc") {
        cmd.arg("/nologo")
            .arg("/LD")
            .arg("/utf-8")
            .arg(format!("/out:{}", output_path.display()));
    } else {
        #[cfg(target_os = "macos")]
        cmd.arg("-dynamiclib");
        #[cfg(not(target_os = "macos"))]
        {
            cmd.arg("-shared");
            cmd.arg("-Wl,-z,relro,-z,now");
        }
        cmd.arg("-fPIC")
            .arg("-fno-exceptions")
            .arg("-o")
            .arg(output_path);
    }
}

fn compile_c(
    src_dir: &Path,
    parser_c: &Path,
    scanner_c: Option<&Path>,
    output_path: &Path,
) -> Result<(), GrammarInstallError> {
    let mut cmd = Command::new(c_compiler());

    add_shared_flags(&mut cmd, output_path);
    cmd.arg(format!("-I{}", src_dir.display()));
    cmd.arg("-std=c11");
    cmd.arg(parser_c);
    if let Some(sc) = scanner_c {
        cmd.arg(sc);
    }

    run_compiler(cmd)
}

fn compile_mixed(
    src_dir: &Path,
    parser_c: &Path,
    scanner_cc: &Path,
    output_path: &Path,
) -> Result<(), GrammarInstallError> {
    // Step 1: compile scanner.cc → scanner.o
    let obj = output_path.with_extension("o");
    let mut cxx = Command::new(cxx_compiler());
    if cfg!(target_env = "msvc") {
        cxx.arg("/nologo").arg("/utf-8");
    } else {
        cxx.arg("-fPIC").arg("-fno-exceptions").arg("-std=c++14");
    }
    cxx.arg(format!("-I{}", src_dir.display()))
        .arg("-c")
        .arg(scanner_cc)
        .arg("-o")
        .arg(&obj);
    run_compiler(cxx)?;

    // Step 2: compile parser.c + link scanner.o → shared library
    let mut cc_cmd = Command::new(c_compiler());
    add_shared_flags(&mut cc_cmd, output_path);
    cc_cmd
        .arg(format!("-I{}", src_dir.display()))
        .arg("-std=c11")
        .arg(parser_c)
        .arg(&obj);
    run_compiler(cc_cmd)?;

    fs::remove_file(&obj).ok();
    Ok(())
}

fn compile_grammar(src_dir: &Path, output_path: &Path) -> Result<(), GrammarInstallError> {
    let parser_c = src_dir.join("parser.c");
    if !parser_c.exists() {
        return Err(GrammarInstallError::MissingSourceFiles {
            path: src_dir.display().to_string(),
        });
    }

    let scanner_c = src_dir.join("scanner.c");
    let scanner_cc = src_dir.join("scanner.cc");

    if scanner_cc.exists() {
        compile_mixed(src_dir, &parser_c, &scanner_cc, output_path)
    } else {
        compile_c(
            src_dir,
            &parser_c,
            scanner_c.exists().then_some(scanner_c.as_path()),
            output_path,
        )
    }
}

/// Installs a language based on the install config
pub fn install_language(
    base_path: PathBuf,
    mut def: GrammarDefinition,
) -> Result<(), GrammarInstallError> {
    let Some(install_def) = def.install.take() else {
        return Err(GrammarInstallError::MissingInstallDefinition);
    };

    let now_nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();

    let repo_name = install_def
        .url
        .split('/')
        .next_back()
        .unwrap_or(&def.name)
        .replace(".git", "");

    let build_root = base_path.join(".build").join(now_nanos.to_string());
    let repo_clone_dir = build_root.join(&repo_name);

    let build_dir = install_def
        .sub_dir
        .as_ref()
        .map(|sub| repo_clone_dir.join(sub))
        .unwrap_or_else(|| repo_clone_dir.clone());

    let normalized_name = normalize_lang_name(&def.name);
    let final_grammar_dir = base_path.join(format!("tree-sitter-{}", def.name));
    let temp_final_dir = base_path.join(format!("tree-sitter-{}-{}", def.name, now_nanos));

    let result: Result<(), GrammarInstallError> = (|| {
        fs::create_dir_all(&build_root)?;

        let output = Command::new("git")
            .arg("clone")
            .arg("--depth")
            .arg("1")
            .arg(&install_def.url)
            .arg(&repo_clone_dir)
            .output()?;

        if !output.status.success() {
            return Err(GrammarInstallError::CommandFailed {
                command: "git clone",
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        if !build_dir.exists() {
            return Err(GrammarInstallError::MissingBuildDir);
        }

        // Compile directly from src/parser.c (+ optional scanner) — no tree-sitter CLI needed
        let ext = get_platform_extensions()[0];
        let output_lib_name = format!("{}.{}", normalized_name, ext);
        let output_lib_path = build_dir.join(&output_lib_name);

        let src_dir = build_dir.join("src");
        compile_grammar(&src_dir, &output_lib_path)?;

        fs::create_dir_all(&temp_final_dir)?;
        fs::copy(&output_lib_path, temp_final_dir.join(&output_lib_name))?;

        let pkg_json = repo_clone_dir.join("package.json");
        if pkg_json.exists() {
            fs::copy(&pkg_json, temp_final_dir.join("package.json")).ok();
        }

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

        if final_grammar_dir.exists() {
            fs::remove_dir_all(&final_grammar_dir)?;
        }
        fs::rename(&temp_final_dir, &final_grammar_dir)?;

        Ok(())
    })();

    if temp_final_dir.exists() {
        fs::remove_dir_all(&temp_final_dir).ok();
    }

    if build_root.exists() {
        fs::remove_dir_all(&build_root).ok();
    }

    result
}
