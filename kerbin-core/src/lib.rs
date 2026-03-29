#![allow(improper_ctypes_definitions)]

extern crate self as kerbin_core;

use tracing::Level;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Initializes the logging system for the core editor
pub fn init_log() {
    let mut log_file_path = home_dir().expect("Home Directory Should Exist");
    log_file_path.push(".kerbin/kerbin.log");

    let log_file = File::options()
        .create(true)
        .write(true)
        .truncate(true)
        .open(log_file_path)
        .expect("file should be able to open");

    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(Level::INFO)
        .with_writer(Mutex::new(log_file))
        .init();
}

pub extern crate async_trait;

pub use kerbin_macros::*;

use std::{env::home_dir, fs::File, sync::Mutex};

pub use kerbin_state_machine::*;

pub use kerbin_input::*;

pub use kerbin_command_lang::{
    AsCommandInfo, Command, CommandAny, CommandFromStr, CommandInfo, CommandPrefix, CommandState,
};

pub use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub mod regex;
pub use regex::*;

pub mod states;
pub use states::*;

pub mod buffer;
pub use buffer::*;

pub mod input;
pub use input::*;

pub mod commands;
pub use commands::*;

pub mod theme;
pub use theme::*;

pub mod palette;
pub use palette::*;

pub mod statusline;
pub use statusline::*;

pub mod hooks;

pub mod chunk;
pub use chunk::*;

pub mod chunks;
pub use chunks::*;

pub mod rope_exts;
pub use rope_exts::*;

pub mod logging;
pub use logging::*;

pub mod signal;
pub use signal::*;

pub mod ipc;
pub use ipc::*;

pub mod resolver;
pub use resolver::*;

pub mod debounce;
pub use debounce::*;

pub mod kb;
pub use kb::*;

pub mod auto_pairs;
pub use auto_pairs::*;
