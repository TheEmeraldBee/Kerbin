#![allow(improper_ctypes_definitions)]

#[macro_export]
macro_rules! get {
    (@inner $name:ident $(, $($t:tt)+)?) => {
        let $name = $name.get();
        get!(@inner $($($t)+)?)
    };
    (@inner mut $name:ident $(, $($t:tt)+)?) => {
        let mut $name = $name.get();
        get!(@inner $($($t)*)?)
    };
    (@inner $($t:tt)+) => {
        compile_error!("Expected comma-separated list of (mut item) or (item), but got an error while parsing. Make sure you don't have a trailing `,`");
    };
    (@inner) => {};
    ($($t:tt)*) => {
        get!(@inner $($t)*)
    };
}

pub fn init_log() {
    let log_file = File::options()
        .create(true)
        .append(true)
        .open("kerbin.log")
        .expect("file should be able to open");

    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(Level::INFO)
        .with_writer(Mutex::new(log_file))
        .init();
}

pub fn init_conf() {
    init_log();

    let original_hook = std::panic::take_hook();

    // Since we can't handle plugin panics in the editor,
    // just log them. This will allow for quickly looking over crashes
    std::panic::set_hook(Box::new(move |e| {
        tracing::error!("{e}");
        original_hook(e);
    }));
}

// Export useful types
pub extern crate kerbin_macros;

use std::{fs::File, sync::Mutex};

pub use kerbin_plugin::Plugin;
pub use kerbin_state_machine::*;

pub use ascii_forge;

pub mod regex;
pub use regex::*;

pub mod state;
pub use state::*;

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
pub use hooks::*;

pub mod layout;
pub use layout::*;

pub mod chunk;
pub use chunk::*;

pub mod chunks;
pub use chunks::*;
use tracing::Level;
