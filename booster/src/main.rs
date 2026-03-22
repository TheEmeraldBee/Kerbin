mod build_kb;

use clap::*;
use dialoguer::*;
use indicatif::*;
use kerbin_core::{ClientIpc, session_name_path, session_pid_path, sessions_dir};
use kerbin_input::tokenize;
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

#[derive(Parser)]
#[clap(version, about)]
/// Kerbin's custom designed booster to launch you into editing
pub struct Args {
    /// Path to the config directory (overrides the saved path)
    #[clap(short, long, global = true)]
    pub config: Option<PathBuf>,

    #[clap(subcommand)]
    pub command: SubCommand,
}

#[derive(Subcommand, Clone)]
pub enum SubCommand {
    /// Tells you info about your current `kerbin` version
    Info,

    /// Manage your kerbin installation
    Install,

    /// Rebuild kerbin with your current config (or -c to use a different one)
    Rebuild,

    /// Generate the config crate from build.kb without a full install (for development/testing)
    Generate {
        /// Directory containing kerbin-core and plugins (defaults to current directory)
        #[clap(short, long)]
        build_dir: Option<PathBuf>,
    },

    /// Send a command to a running kerbin session
    Exec {
        /// The session ID to target
        #[clap(short, long)]
        session: String,
        /// The command to execute
        #[clap(num_args = 1.., required = true)]
        command: Vec<String>,
    },

    /// Manage running kerbin sessions
    Session {
        #[clap(subcommand)]
        command: SessionCommand,
    },

    /// Validate config files without doing a full rebuild.
    Check,
}

#[derive(Subcommand, Clone)]
pub enum SessionCommand {
    /// List all running sessions
    List,

    /// Gracefully close a session (sends :q!)
    Close {
        /// The session ID to close
        session: String,
    },

    /// Open a file in a running session
    Open {
        /// The session ID to target
        #[clap(short, long)]
        session: String,
        /// The file to open
        file: String,
    },

    /// Force-kill a session (SIGKILL)
    Kill {
        /// The session ID or name to kill
        session: String,
    },

    /// Assign a human-readable name to a session
    Rename {
        /// The session ID to rename
        session: String,
        /// The new name
        name: String,
    },
}

#[derive(Serialize, Deserialize, Clone)]
struct KerbinInfo {
    version: String,
    config_path: String,
    install_date: String,
    last_build_date: String,
}

impl KerbinInfo {
    fn save(&self, kerbin_dir: &Path) {
        let mut info_path = kerbin_dir.to_path_buf();
        info_path.push("kerbin-info.json");
        let json = serde_json::to_string_pretty(self).expect("Failed to serialize info");
        std::fs::write(info_path, json).expect("Failed to write kerbin-info.json");
    }

    fn load(kerbin_dir: &Path) -> Option<Self> {
        let mut info_path = kerbin_dir.to_path_buf();
        info_path.push("kerbin-info.json");
        if !info_path.exists() {
            return None;
        }
        let json = std::fs::read_to_string(info_path).ok()?;
        serde_json::from_str(&json).ok()
    }
}

fn get_timestamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn canon(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

fn get_default_config_path() -> PathBuf {
    // Check XDG_CONFIG_HOME environment variable first
    if let Ok(home) = std::env::var("XDG_CONFIG_HOME") {
        let mut path = PathBuf::from(home);
        path.push("kerbin");
        return path;
    }

    // Check if ~/.config/kerbin exists (Common on macOS/Linux dev setups)
    let home_config = dirs::home_dir().map(|h| h.join(".config").join("kerbin"));
    if let Some(path) = home_config.as_ref().filter(|p| p.exists()) {
        return path.clone();
    }

    // Fallback to the OS-native directory (via the `dirs` crate)
    let mut res = dirs::config_dir().expect("Failed to find user config directory");
    res.push("kerbin");
    res
}

fn handle_config_copy(kerbin_dir: &Path) -> Option<PathBuf> {
    let mut default_config_src = kerbin_dir.to_path_buf();
    default_config_src.push("./build/config"); // Changed from kerbin-config to config

    let default_config_dest = get_default_config_path();

    // Check if config already exists
    let config_exists = default_config_dest.exists();

    let prompt_msg = if config_exists {
        format!(
            "A config already exists at {}. Do you want to overwrite it with the default config?",
            canon(&default_config_dest).display()
        )
    } else {
        format!(
            "Would you like to copy the default config to {}?",
            canon(&default_config_dest).display()
        )
    };

    if config_exists {
        println!(
            "⚠️  Warning: Config already exists at {}",
            canon(&default_config_dest).display()
        );
    }

    let should_copy = Confirm::with_theme(&theme::ColorfulTheme::default())
        .with_prompt(prompt_msg)
        .default(false)
        .interact()
        .unwrap();

    if should_copy {
        if config_exists {
            let backup = Confirm::with_theme(&theme::ColorfulTheme::default())
                .with_prompt("Create a backup of the existing config first?")
                .default(true)
                .interact()
                .unwrap();

            if backup {
                let mut backup_path = default_config_dest.clone();
                backup_path.set_extension(format!("backup.{}", chrono::Local::now().timestamp()));
                std::fs::rename(&default_config_dest, &backup_path)
                    .expect("Failed to create backup");
                println!("✓ Backed up existing config to: {}", canon(&backup_path).display());
            } else {
                std::fs::remove_dir_all(&default_config_dest)
                    .expect("Failed to remove existing config");
            }
        }

        // Copy config directory
        let copy_options = fs_extra::dir::CopyOptions::new().overwrite(true);
        fs_extra::dir::copy(
            &default_config_src,
            default_config_dest.parent().unwrap(),
            &copy_options,
        )
        .expect("Failed to copy config");

        // Rename if needed (fs_extra copies with source name)
        let mut copied_path = default_config_dest.parent().unwrap().to_path_buf();
        copied_path.push("config"); // Changed from kerbin-config to config
        if copied_path != default_config_dest {
            std::fs::rename(&copied_path, &default_config_dest)
                .expect("Failed to rename config directory");
        }

        println!("✓ Config copied to: {}", canon(&default_config_dest).display());

        // Update the config's Cargo.toml to fix relative paths
        let mut config_cargo_toml = default_config_dest.clone();
        config_cargo_toml.push("Cargo.toml");

        if config_cargo_toml.exists() {
            println!("Updating config Cargo.toml paths...");
            let cargo_content = std::fs::read_to_string(&config_cargo_toml)
                .expect("Failed to read config Cargo.toml");

            let mut build_path = kerbin_dir.to_path_buf();
            build_path.push("build");
            let build_path_str = build_path.to_str().unwrap();

            // Use regex to replace any path = "../something" with absolute paths
            let re =
                regex::Regex::new(r#"path\s*=\s*"\.\./([^"]*)""#).expect("Failed to create regex");

            let updated_content = re
                .replace_all(&cargo_content, |caps: &regex::Captures| {
                    let relative_path = &caps[1];
                    format!(r#"path = "{}/{}" "#, build_path_str, relative_path)
                })
                .to_string();

            std::fs::write(&config_cargo_toml, updated_content)
                .expect("Failed to write updated config Cargo.toml");
            println!("✓ Updated config dependency paths");
        }

        return Some(default_config_dest);
    } else {
        let use_custom = Confirm::with_theme(&theme::ColorfulTheme::default())
            .with_prompt("Would you like to specify a custom config path?")
            .default(false)
            .interact()
            .unwrap();

        if use_custom {
            let custom_path: String = Input::with_theme(&theme::ColorfulTheme::default())
                .with_prompt("Enter custom config path")
                .interact_text()
                .unwrap();
            return Some(PathBuf::from(custom_path));
        }
    }

    // Default to the build directory path
    Some(default_config_src)
}

/// `build_dir`  — where kerbin-core and plugins live (used to write dependency paths)
/// `output_dir` — where to write the generated crate (Cargo.toml + src/lib.rs)
fn generate_config_crate(config_dir: &Path, build_dir: &Path, output_dir: &Path) -> PathBuf {
    let build_kb_path = config_dir.join("build.kb");

    let content = match std::fs::read_to_string(&build_kb_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("✗ build.kb not found at '{}'", canon(&build_kb_path).display());
            eprintln!("  This file is required to declare your plugins.");
            eprintln!("  Error: {}", e);
            std::process::exit(1);
        }
    };

    let plugins = match build_kb::parse(&content) {
        Ok(p) => p,
        Err(errors) => {
            eprintln!("✗ Errors in build.kb:");
            for err in &errors {
                eprintln!("  line {}: {}", err.line, err.message);
            }
            std::process::exit(1);
        }
    };

    let src_dir = output_dir.join("src");
    std::fs::create_dir_all(&src_dir).expect("Failed to create output src dir");

    let cargo_toml = build_kb::generate_cargo_toml(&plugins, build_dir, output_dir);
    let lib_rs = build_kb::generate_lib_rs(&plugins);

    std::fs::write(output_dir.join("Cargo.toml"), cargo_toml)
        .expect("Failed to write generated Cargo.toml");
    std::fs::write(src_dir.join("lib.rs"), lib_rs).expect("Failed to write generated lib.rs");

    println!("✓ Generated config crate from build.kb");
    output_dir.to_path_buf()
}

fn validate_plugins(generated_config_dir: &Path) -> bool {
    let style = ProgressStyle::default_spinner()
        .template("{spinner:.green} {msg}")
        .unwrap();
    let spinner = ProgressBar::new_spinner()
        .with_message("Validating plugins...")
        .with_style(style);
    spinner.enable_steady_tick(Duration::from_millis(100));

    let child = std::process::Command::new("cargo")
        .args(["check", "--message-format", "json"])
        .current_dir(generated_config_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let output = match child {
        Ok(c) => match c.wait_with_output() {
            Ok(o) => o,
            Err(e) => {
                spinner.finish_and_clear();
                eprintln!("✗ Failed to wait on cargo check: {}", e);
                return false;
            }
        },
        Err(e) => {
            spinner.finish_and_clear();
            eprintln!("✗ Failed to run cargo check: {}", e);
            return false;
        }
    };

    spinner.finish_and_clear();

    if output.status.success() {
        println!("✓ Plugins validated");
        return true;
    }

    eprintln!("\n✗ Plugin validation failed:\n");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut printed_any = false;
    for line in stdout.lines() {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if json["reason"] != "compiler-message" {
            continue;
        }
        let msg = &json["message"];
        if msg["level"] != "error" {
            continue;
        }
        if let Some(rendered) = msg["rendered"].as_str() {
            eprint!("{}", rendered);
            printed_any = true;
        }
    }

    if !printed_any {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprint!("{}", stderr);
    }

    eprintln!("\n  Make sure each plugin exposes: pub async fn init(state: &mut State)");

    false
}

fn build_kerbin(kerbin_dir: &Path, config_path: Option<PathBuf>, info: &mut KerbinInfo) {
    let mut cargo_toml_path = kerbin_dir.to_path_buf();
    cargo_toml_path.push("./build/kerbin/Cargo.toml");

    let mut build_dir = kerbin_dir.to_path_buf();
    build_dir.push("./build");

    let final_config_path = if let Some(config) = config_path {
        config
    } else {
        // Try to use the saved config path from previous installation
        PathBuf::from(&info.config_path)
    };

    println!("Using config path: {}", canon(&final_config_path).display());

    // Generate the config crate from build.kb in the user's config directory
    let generated_config_path = generate_config_crate(
        &final_config_path,
        &build_dir,
        &build_dir.join("generated-config"),
    );

    // Validate plugins before the slow release build
    if !validate_plugins(&generated_config_path) {
        std::fs::remove_dir_all(&generated_config_path).ok();
        eprintln!("✗ Fix the errors above, then run 'booster rebuild'");
        std::process::exit(1);
    }

    // Update kerbin/Cargo.toml to point at the generated config crate
    let cargo_content =
        std::fs::read_to_string(&cargo_toml_path).expect("Failed to read Cargo.toml");

    let generated_config_path_str = generated_config_path
        .to_str()
        .expect("Invalid generated config path");

    // Use regex to find and replace any existing config path
    let re = regex::Regex::new(r#"config\s*=\s*\{\s*path\s*=\s*"[^"]*"\s*\}"#)
        .expect("Failed to create regex");

    let updated_content = if re.is_match(&cargo_content) {
        re.replace(
            &cargo_content,
            format!(r#"config = {{ path = "{}" }}"#, generated_config_path_str),
        )
        .to_string()
    } else {
        eprintln!("[!] Warning: Could not find kerbin-config path in Cargo.toml");
        eprintln!("   The Cargo.toml might have an unexpected format");
        cargo_content
    };

    std::fs::write(&cargo_toml_path, updated_content).expect("Failed to write updated Cargo.toml");
    println!("✓ Updated config path in Cargo.toml");

    let style = ProgressStyle::default_spinner()
        .template("{spinner:.green} {msg}")
        .unwrap();

    let build_bar = ProgressBar::new_spinner()
        .with_message("Building Kerbin with Cargo".to_string())
        .with_style(style);
    build_bar.enable_steady_tick(Duration::from_millis(100));

    let child = std::process::Command::new("cargo")
        .args(["build", "--release", "-p", "kerbin"])
        .current_dir(&build_dir)
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn();

    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            build_bar.finish_and_clear();
            eprintln!("✗ Failed to start cargo build");
            eprintln!("  Error: {}", e);
            eprintln!("  Build directory: {}", canon(&build_dir).display());
            eprintln!("  Make sure cargo is installed and in your PATH");
            panic!("Failed to spawn cargo process");
        }
    };

    if let Some(stderr) = child.stderr.take() {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        while reader
            .read_line(&mut line)
            .expect("Lines should still exist")
            != 0
        {
            // Parse cargo build information
            let clean_line = line.trim();
            if clean_line.starts_with("Compiling") {
                // Extract package name from "Compiling package_name v1.0.0"
                if let Some(pkg) = clean_line.split_whitespace().nth(1) {
                    build_bar.set_message(format!("Building: {}", pkg));
                }
            } else if clean_line.starts_with("Finished") {
                build_bar.set_message("Finalizing build...".to_string());
            } else if clean_line.contains("Downloading") || clean_line.contains("Updating") {
                build_bar.set_message(format!("Cargo: {}", clean_line));
            } else if clean_line.contains("error") || clean_line.starts_with("error:") {
                eprintln!("{}", clean_line);
            }
            line.clear();
        }
    }

    let status = child.wait().expect("Failed to wait on cargo process");
    build_bar.finish_and_clear();

    if !status.success() {
        eprintln!("✗ Failed to build Kerbin");
        eprintln!("  Build exited with status: {}", status);
        eprintln!("  Check the error output above for details");
        panic!("Failed to build kerbin");
    }

    println!("✓ Successfully built Kerbin");

    // Move binary to ~/.kerbin/bin/kerbin
    let mut bin_dir = kerbin_dir.to_path_buf();
    bin_dir.push("./bin");
    std::fs::create_dir_all(&bin_dir).expect("Failed to create bin directory");

    let mut source_binary = build_dir;
    source_binary.push("./target/release/kerbin");

    let mut dest_binary = bin_dir;
    dest_binary.push("kerbin");

    std::fs::copy(&source_binary, &dest_binary)
        .expect("Failed to copy kerbin binary to bin directory");

    println!("[✓] Installed Kerbin to: {}", canon(&dest_binary).display());

    // Update info — save the user's config dir path, not the generated crate path
    info.config_path = final_config_path
        .to_str()
        .expect("Invalid config path")
        .to_string();
    info.last_build_date = get_timestamp();
    info.save(kerbin_dir);
}

/// Resolve a session argument that may be either a UUID or a human-readable name.
/// Returns the session UUID if found, otherwise returns the input unchanged.
fn resolve_session(input: &str) -> String {
    let dir = sessions_dir();

    // Direct UUID match: the .in file exists
    let in_file = format!("{}/{}.in", dir, input);
    if std::path::Path::new(&in_file).exists() {
        return input.to_string();
    }

    // Search .name sidecar files for a match
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let filename = entry.file_name().into_string().unwrap_or_default();
            if let Some(session_id) = filename.strip_suffix(".name") {
                let name_path = format!("{}/{}", dir, filename);
                if let Ok(stored) = std::fs::read_to_string(&name_path)
                    && stored.trim() == input
                {
                    return session_id.to_string();
                }
            }
        }
    }

    input.to_string()
}

/// Recursively lex all `.kb` files in `dir`, printing errors and returning the error count.
/// `skip` is a list of absolute paths to exclude from the walk.
fn check_kb_dir(dir: &std::path::Path, root: &std::path::Path, skip: &[PathBuf]) -> usize {
    let mut total_errors = 0;

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("{}: error reading directory: {}", canon(dir).display(), e);
            return 1;
        }
    };

    let mut paths: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    paths.sort();

    for path in paths {
        if skip.contains(&path) {
            continue;
        }
        if path.is_dir() {
            total_errors += check_kb_dir(&path, root, &[]);
        } else if path.extension().and_then(|e| e.to_str()) == Some("kb") {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("{}: error reading file: {}", rel.display(), e);
                    total_errors += 1;
                    continue;
                }
            };
            let mut file_ok = true;
            for (i, line) in content.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Err(e) = tokenize(line) {
                    eprintln!("{}: line {}: {}", rel.display(), i + 1, e);
                    total_errors += 1;
                    file_ok = false;
                }
            }
            if file_ok {
                println!("{}: OK", rel.display());
            }
        }
    }

    total_errors
}

fn main() {
    let args = Args::parse();

    let mut kerbin_dir =
        std::env::home_dir().expect("Home directory must exist for booster to work");
    kerbin_dir.push("./.kerbin");

    let global_config = args.config;

    match args.command {
        SubCommand::Info => {
            if let Some(info) = KerbinInfo::load(&kerbin_dir) {
                println!("Kerbin Installation Info");
                println!();
                println!("Version:          {}", info.version);
                println!("Config Path:      {}", info.config_path);
                println!("Install Date:     {}", info.install_date);
                println!("Last Build:       {}", info.last_build_date);
                println!();

                let mut bin_path = kerbin_dir.clone();
                bin_path.push("./bin/kerbin");
                if bin_path.exists() {
                    println!("Binary Location:  {}", canon(&bin_path).display());
                    println!("✓ Binary exists");
                } else {
                    println!("✗ Binary not found at expected location");
                }

                let build_kb_path = PathBuf::from(&info.config_path).join("build.kb");
                if let Ok(content) = std::fs::read_to_string(&build_kb_path) {
                    println!();
                    match build_kb::parse(&content) {
                        Ok(plugins) if plugins.is_empty() => {
                            println!("Plugins:          (none)");
                        }
                        Ok(plugins) => {
                            println!("Plugins ({}):", plugins.len());
                            for plugin in &plugins {
                                let source_label = match &plugin.source {
                                    build_kb::PluginSource::Core => "core".to_string(),
                                    build_kb::PluginSource::Git(url) => format!("git: {}", url),
                                    build_kb::PluginSource::Path(p) => {
                                        format!("path: {}", p.display())
                                    }
                                };
                                println!("  {} ({})", plugin.name, source_label);
                            }
                        }
                        Err(_) => {
                            println!("Plugins:          (build.kb parse error)");
                        }
                    }
                }
            } else {
                println!("No Kerbin installation found.");
                println!("Run 'kerbin-booster install' to install Kerbin.");
            }
        }
        SubCommand::Install => {
            // Fetch available tags from GitHub
            println!("Fetching available versions from GitHub...");
            let tags_output = std::process::Command::new("git")
                .args([
                    "ls-remote",
                    "--tags",
                    "--refs",
                    "https://www.github.com/EmeraldPandaTurtle/Kerbin.git",
                ])
                .output()
                .expect("Failed to fetch tags from GitHub");

            if !tags_output.status.success() {
                eprintln!("✗ Failed to fetch tags from GitHub");
                eprintln!("   Make sure you have internet connection and git installed");
                std::process::exit(1);
            }

            let tags_str = String::from_utf8_lossy(&tags_output.stdout);
            let mut versions: Vec<String> = tags_str
                .lines()
                .filter_map(|line| {
                    // Extract tag name from "hash\trefs/tags/tagname"
                    line.split("refs/tags/").nth(1).map(|s| s.to_string())
                })
                .collect();

            // Add "git" option for master branch at the beginning
            versions.insert(0, "git (master branch)".to_string());

            if versions.len() == 1 {
                eprintln!("⚠️  Warning: No version tags found in repository");
                eprintln!("   Only master branch is available");
            }

            let version = Select::with_theme(&theme::ColorfulTheme::default())
                .with_prompt("Pick your flavor")
                .default(0)
                .items(&versions[..])
                .interact()
                .unwrap();

            let selected_version = if versions[version].starts_with("git") {
                "git".to_string()
            } else {
                versions[version].clone()
            };

            println!("Installing version: {}", selected_version);

            // Ensure .kerbin is created
            let _ = std::fs::create_dir_all(&kerbin_dir);

            // Change dir into .kerbin
            std::env::set_current_dir(&kerbin_dir).unwrap();

            let mut build_dir = kerbin_dir.clone();
            build_dir.push("./build");
            let _ = std::fs::remove_dir_all(&build_dir);

            let style = ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap();

            let git_bar = ProgressBar::new_spinner()
                .with_message(format!("Cloning Kerbin from GitHub ({})", selected_version))
                .with_style(style.clone());
            git_bar.enable_steady_tick(Duration::from_millis(100));

            // Determine git arguments based on version
            let git_args = if selected_version == "git" {
                // Clone master branch
                vec![
                    "clone",
                    "--depth=1",
                    "--branch=master",
                    "--progress",
                    "https://www.github.com/EmeraldPandaTurtle/Kerbin.git",
                    "build",
                ]
            } else {
                // Clone specific tag
                vec![
                    "clone",
                    "--depth=1",
                    "--branch",
                    &selected_version,
                    "--progress",
                    "https://www.github.com/EmeraldPandaTurtle/Kerbin.git",
                    "build",
                ]
            };

            let mut child = std::process::Command::new("git")
                .args(&git_args)
                .stderr(Stdio::piped())
                .stdout(Stdio::piped())
                .stdin(Stdio::piped())
                .spawn()
                .expect("Failed to clone kerbin");

            if let Some(stderr) = child.stderr.take() {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                while reader
                    .read_line(&mut line)
                    .expect("Lines should still exist")
                    != 0
                {
                    // Parse git progress information
                    if line.contains("Receiving objects:") || line.contains("Resolving deltas:") {
                        let clean_line = line.trim().replace('\r', "");
                        git_bar.set_message(format!("Cloning Kerbin: {}", clean_line));
                    }
                    line.clear();
                }
            }

            let status = child.wait().expect("Failed to wait on git process");
            if status.success() {
                git_bar.finish_and_clear();
                if selected_version == "git" {
                    println!("✓ Successfully cloned Kerbin (master branch)");
                } else {
                    println!("✓ Successfully cloned Kerbin ({})", selected_version);
                }
            } else {
                git_bar.finish_and_clear();
                eprintln!("✗ Failed to clone Kerbin");
                eprintln!(
                    "   Make sure the tag/branch '{}' exists in the repository",
                    selected_version
                );
                panic!("Failed to clone kerbin");
            }

            // Handle config setup
            let config_path = if let Some(ref path) = global_config {
                println!("Using config path: {}", canon(path).display());
                Some(path.clone())
            } else {
                handle_config_copy(&kerbin_dir)
            };

            // Create initial info
            let mut info = KerbinInfo {
                version: selected_version,
                config_path: config_path.as_ref().unwrap().to_str().unwrap().to_string(),
                install_date: get_timestamp(),
                last_build_date: String::new(),
            };

            // Build kerbin after cloning
            build_kerbin(&kerbin_dir, config_path, &mut info);

            println!();
            println!("✓ Installation complete!");
            println!("  Ensure to add ~/.kerbin/bin to your PATH");
        }
        SubCommand::Exec { session, command } => {
            let full_command = command.join(" ");
            if let Err(e) = ClientIpc::send_command(&session, full_command) {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
        SubCommand::Session { command } => match command {
            SessionCommand::List => {
                let dir = sessions_dir();
                let mut sessions: Vec<(String, Option<String>)> = std::fs::read_dir(&dir)
                    .map(|rd| {
                        rd.filter_map(|e| e.ok())
                            .filter_map(|e| {
                                let filename = e.file_name().into_string().ok()?;
                                let id = filename.strip_suffix(".in")?.to_string();
                                let name = std::fs::read_to_string(format!("{}/{}.name", dir, id))
                                    .ok()
                                    .map(|s| s.trim().to_string());
                                Some((id, name))
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                sessions.sort_by(|a, b| a.0.cmp(&b.0));

                if sessions.is_empty() {
                    println!("No running sessions.");
                } else {
                    println!("{} session(s):", sessions.len());
                    for (id, name) in sessions {
                        match name {
                            Some(n) => println!("  {}  ({})", id, n),
                            None => println!("  {}", id),
                        }
                    }
                }
            }
            SessionCommand::Close { session } => {
                let session = resolve_session(&session);
                if let Err(e) = ClientIpc::send_command(&session, "q!".to_string()) {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
            SessionCommand::Open { session, file } => {
                let session = resolve_session(&session);
                let command = format!("open {}", file);
                if let Err(e) = ClientIpc::send_command(&session, command) {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
            SessionCommand::Kill { session } => {
                let session = resolve_session(&session);
                let pid_path = session_pid_path(&session);
                match std::fs::read_to_string(&pid_path) {
                    Ok(pid_str) => {
                        let pid = pid_str.trim();
                        let status = std::process::Command::new("kill")
                            .args(["-9", pid])
                            .status();
                        match status {
                            Ok(s) if s.success() => {}
                            Ok(_) => {
                                eprintln!(
                                    "kill returned non-zero (process may have already exited)."
                                );
                                std::process::exit(1);
                            }
                            Err(e) => {
                                eprintln!("Failed to run kill: {}", e);
                                std::process::exit(1);
                            }
                        }
                    }
                    Err(_) => {
                        eprintln!("Session '{}' not found.", session);
                        std::process::exit(1);
                    }
                }
            }
            SessionCommand::Rename { session, name } => {
                let session = resolve_session(&session);
                let name_path = session_name_path(&session);
                // Verify the session is actually running before naming it
                let in_file = format!("{}/{}.in", sessions_dir(), session);
                if !std::path::Path::new(&in_file).exists() {
                    eprintln!("Session '{}' not found.", session);
                    std::process::exit(1);
                }
                if let Err(e) = std::fs::write(&name_path, &name) {
                    eprintln!("Failed to write name file: {}", e);
                    std::process::exit(1);
                }
                println!("Session {} renamed to '{}'.", session, name);
            }
        },
        SubCommand::Generate { build_dir } => {
            let config_dir = global_config.unwrap_or_else(|| PathBuf::from("./config"));
            let build_dir = build_dir.unwrap_or_else(|| std::env::current_dir().unwrap());
            let output_dir = build_dir.join("generated-config");

            let generated = generate_config_crate(&config_dir, &build_dir, &output_dir);
            if !validate_plugins(&generated) {
                std::process::exit(1);
            }
        }

        SubCommand::Check => {
            let config_dir = global_config.unwrap_or_else(get_default_config_path);

            let mut total_errors = 0usize;

            // Check build.kb
            let build_kb_path = config_dir.join("build.kb");
            match std::fs::read_to_string(&build_kb_path) {
                Err(e) => {
                    eprintln!("build.kb: error reading file: {}", e);
                    total_errors += 1;
                }
                Ok(content) => match build_kb::parse(&content) {
                    Ok(plugins) => {
                        println!("build.kb: OK ({} plugin(s))", plugins.len());
                    }
                    Err(errors) => {
                        for err in &errors {
                            eprintln!("build.kb: line {}: {}", err.line, err.message);
                        }
                        total_errors += errors.len();
                    }
                },
            }

            // Lex-check all .kb files in the config dir (excluding build.kb)
            if config_dir.exists() {
                let config_dir = canon(&config_dir);
                let skip = vec![config_dir.join("build.kb")];
                total_errors += check_kb_dir(&config_dir, &config_dir, &skip);
            }

            if total_errors == 0 {
                println!("No errors found.");
            } else {
                eprintln!("{} error(s) found.", total_errors);
                std::process::exit(1);
            }
        }

        SubCommand::Rebuild => {
            if !kerbin_dir.exists() {
                eprintln!(
                    "✗ Kerbin installation not found at {}",
                    kerbin_dir.display()
                );
                eprintln!("  Please run 'booster install' first");
                std::process::exit(1);
            }

            let mut info = KerbinInfo::load(&kerbin_dir)
                .expect("Failed to load kerbin-info.json. Installation may be corrupted.");

            let final_config = if let Some(path) = global_config {
                println!("Using config: {}", canon(&path).display());
                Some(path)
            } else {
                println!("Using config: {}", info.config_path);
                println!("  (Use -c to override)");
                Some(PathBuf::from(&info.config_path))
            };

            build_kerbin(&kerbin_dir, final_config, &mut info);
            println!("✓ Rebuild complete!");
        }
    }
}
