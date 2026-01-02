#![allow(improper_ctypes_definitions)]

use tracing::Level;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[macro_export]
/// Automatically calls the get method on all systems provided as arguments
macro_rules! get {
    (@inner $name:ident $(, $($t:tt)+)?) => {
        let $name = $name.get().await;
        get!(@inner $($($t)+)?)
    };
    (@inner mut $name:ident $(, $($t:tt)+)?) => {
        let mut $name = $name.get().await;
        get!(@inner $($($t)*)?)
    };
    (@inner Some($name:ident) $(, $($t:tt)+)?) => {
        let Some($name) = $name.get().await else {
            return;
        };
        get!(@inner $($($t)+)?)
    };
    (@inner Some(mut $name:ident) $(, $($t:tt)+)?) => {
        let Some(mut $name) = $name.get().await else {
            return;
        };
        get!(@inner $($($t)+)?)
    };
    (@inner $($t:tt)+) => {
        compile_error!("Expected comma-separated list of (mut item), (item), Some(item), or Some(mut item), but got an error while parsing. Make sure you don't have a trailing `,`");
    };
    (@inner) => {};
    ($($t:tt)*) => {
        get!(@inner $($t)*)
    };
}

/// Initializes the logging system for the core editor
pub fn init_log() {
    let mut log_file_path = home_dir().expect("Home Directory Should Exist");
    log_file_path.push(".kerbin/kerbin.log");

    let log_file = File::options()
        .create(true)
        .append(true)
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

pub use ascii_forge;

pub use kerbin_input::*;

pub use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Module for regular expression utilities
pub mod regex;
pub use regex::*;

/// Module containing core editor state definitions
pub mod states;
pub use states::*;

/// Module for managing text buffers
pub mod buffer;
pub use buffer::*;

/// Module for input handling and keybindings
pub mod input;
pub use input::*;

/// Module for command definitions and command execution
pub mod commands;
pub use commands::*;

/// Module for theme management and ContentStyle extensions
pub mod theme;
pub use theme::*;

/// Module for the command palette UI and logic
pub mod palette;
pub use palette::*;

/// Module for the statusline rendering and configuration
pub mod statusline;
pub use statusline::*;

/// Module for editor hooks and event handling
pub mod hooks;

/// Module for individual rendering chunks
pub mod chunk;
pub use chunk::*;

/// Module for managing multiple rendering chunks
pub mod chunks;
pub use chunks::*;

/// Module used to extend the functionality of rope
pub mod rope_exts;
pub use rope_exts::*;

/// Module used to extend the functionality of ContentStyle
pub mod style_exts;
pub use style_exts::*;

pub mod logging;
pub use logging::*;

pub mod signal;
pub use signal::*;

pub mod ipc;
pub use ipc::*;

pub mod resolver;
pub use resolver::*;

pub mod word_split;
pub use word_split::*;

pub mod debounce;
pub use debounce::*;
