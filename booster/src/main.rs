use clap::*;
use dialoguer::*;
use indicatif::*;
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
    #[clap(subcommand)]
    pub command: SubCommand,
}

#[derive(Subcommand, Clone)]
pub enum SubCommand {
    /// Tells you info about your current `kerbin` version
    Info,

    /// Manage your kerbin installation
    Install,

    /// Rebuild kerbin with `rust` config changes applied.
    /// Toml file changes dont require rebuilds!
    Rebuild {
        /// Path to a new config directory
        #[clap(short, long)]
        config: Option<PathBuf>,
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
            default_config_dest.display()
        )
    } else {
        format!(
            "Would you like to copy the default config to {}?",
            default_config_dest.display()
        )
    };

    if config_exists {
        println!(
            "⚠️  Warning: Config already exists at {}",
            default_config_dest.display()
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
                println!("✓ Backed up existing config to: {}", backup_path.display());
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

        println!("✓ Config copied to: {}", default_config_dest.display());

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

fn build_kerbin(kerbin_dir: &Path, config_path: Option<PathBuf>, info: &mut KerbinInfo) {
    let mut cargo_toml_path = kerbin_dir.to_path_buf();
    cargo_toml_path.push("./build/kerbin/Cargo.toml");

    let final_config_path = if let Some(config) = config_path {
        config
    } else {
        // Try to use the saved config path from previous installation
        PathBuf::from(&info.config_path)
    };

    println!("Using config path: {}", final_config_path.display());

    // Update Cargo.toml with config path
    let cargo_content =
        std::fs::read_to_string(&cargo_toml_path).expect("Failed to read Cargo.toml");

    let config_path_str = final_config_path.to_str().expect("Invalid config path");

    // Use regex to find and replace any existing kerbin-config path
    let re = regex::Regex::new(r#"config\s*=\s*\{\s*path\s*=\s*"[^"]*"\s*\}"#)
        .expect("Failed to create regex");

    let updated_content = if re.is_match(&cargo_content) {
        re.replace(
            &cargo_content,
            format!(r#"config = {{ path = "{}" }}"#, config_path_str),
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

    let mut build_dir = kerbin_dir.to_path_buf();
    build_dir.push("./build");

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
            eprintln!("  Build directory: {}", build_dir.display());
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

    println!("[✓] Installed Kerbin to: {}", dest_binary.display());

    // Update info
    info.config_path = config_path_str.to_string();
    info.last_build_date = get_timestamp();
    info.save(kerbin_dir);
}

fn main() {
    let args = Args::parse();

    let mut kerbin_dir =
        std::env::home_dir().expect("Home directory must exist for booster to work");
    kerbin_dir.push("./.kerbin");

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
                    println!("Binary Location:  {}", bin_path.display());
                    println!("✓ Binary exists");
                } else {
                    println!("✗ Binary not found at expected location");
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
            let config_path = handle_config_copy(&kerbin_dir);

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
        SubCommand::Rebuild { config } => {
            if !kerbin_dir.exists() {
                eprintln!(
                    "✗ Kerbin installation not found at {}",
                    kerbin_dir.display()
                );
                eprintln!("  Please run 'kerbin-booster install' first");
                std::process::exit(1);
            }

            let mut info = KerbinInfo::load(&kerbin_dir)
                .expect("Failed to load kerbin-info.json. Installation may be corrupted.");

            println!("Rebuilding Kerbin...");
            println!("Current config: {}", info.config_path);

            let final_config = if config.is_some() {
                config
            } else {
                let use_current = Confirm::with_theme(&theme::ColorfulTheme::default())
                    .with_prompt(format!("Use current config path ({})?", info.config_path))
                    .default(true)
                    .interact()
                    .unwrap();

                if use_current {
                    Some(PathBuf::from(&info.config_path))
                } else {
                    let custom_path: String = Input::with_theme(&theme::ColorfulTheme::default())
                        .with_prompt("Enter new config path")
                        .default(info.config_path.clone())
                        .interact_text()
                        .unwrap();
                    Some(PathBuf::from(custom_path))
                }
            };

            build_kerbin(&kerbin_dir, final_config, &mut info);
            println!("✓ Rebuild complete!");
        }
    }
}
