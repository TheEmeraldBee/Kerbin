#![allow(improper_ctypes_definitions)]

use tracing::Level;

#[macro_export]
/// Automatically calls the `.get()` method on all systems provided as arguments.
///
/// This macro is a convenience for system parameters like `Res` and `ResMut`
/// within Kerbin's system functions. It generates `let` bindings that
/// automatically call `.get()` (for immutable access) or `.get_mut()` (for mutable access)
/// on the input identifiers.
///
/// Each item can be prepended with `mut` if it's a mutable resource (like `ResMut`)
/// or any item that requires a write lock.
///
/// # Examples
///
/// ```rust
/// # use std::sync::{Arc, RwLock};
/// # use kerbin_core::*;
/// # use kerbin_macros::State;
/// # #[derive(State, Default)] pub struct A(pub u8);
/// # #[derive(State, Default)] pub struct B(pub u8);
/// # #[derive(State, Default)] pub struct C(pub u8);
/// # #[derive(State, Default)] pub struct D(pub u8);
/// # #[derive(State, Default)] pub struct E(pub u8);
/// fn my_system(a: Res<A>, b: ResMut<B>, c: Res<C>, d: Res<D>, e: ResMut<E>) {
///     // Instead of writing:
///     // let a = a.get();
///     // let mut b = b.get_mut(); // Or b.get() if it implements DerefMut
///     // ... for each parameter
///     //
///     // use the macro:
///     get!(a, mut b, c, d, mut e);
///
///     // Now `a`, `b`, `c`, `d`, `e` are the dereferenced values (e.g., &A or &mut B):
///     println!("A: {}, B: {}, C: {}, D: {}, E: {}", a.0, b.0, c.0, d.0, e.0);
///     b.0 += 1; // You can modify `b` and `e`
///     e.0 += 1;
/// }
/// ```
macro_rules! get {
    (@inner $name:ident $(, $($t:tt)+)?) => {
        let $name = $name.get();
        get!(@inner $($($t)+)?)
    };
    (@inner mut $name:ident $(, $($t:tt)+)?) => {
        let mut $name = $name.get(); // Using get_mut() for mutable access
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

/// Initializes the logging system for the core editor.
///
/// This function sets up `tracing-subscriber` to write log messages
/// to a file named "kerbin.log". It configures the logger to:
/// - Disable ANSI color codes for file output.
/// - Set the maximum logging level to `INFO`.
/// - Use a `Mutex` to safely write to the log file from multiple threads.
///
/// This function is called automatically within `init_conf`.
///
/// # Panics
///
/// Panics if the "kerbin.log" file cannot be opened or created.
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

/// Should **always** be called at the beginning of your editor's configuration.
///
/// This function performs essential initialization steps:
/// 1. Calls `init_log()` to set up file logging.
/// 2. Sets a global panic hook. This hook ensures that any panics occurring
///    within the editor (including those from plugins) are logged to the
///    configured log file (`kerbin.log`) before the original panic hook is called.
///    This is crucial for debugging crashes in a headless or GUI environment.
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

// Export useful types and modules from Kerbin's ecosystem.

pub extern crate kerbin_macros;

use std::{fs::File, sync::Mutex};

pub use kerbin_plugin::Plugin;
pub use kerbin_state_machine::*;

pub use ascii_forge;

/// Module for regular expression utilities, including a `ropey`-compatible cursor.
pub mod regex;
pub use regex::*;

/// Module containing core editor state definitions.
pub mod state;
pub use state::*;

/// Module for managing text buffers.
pub mod buffer;
pub use buffer::*;

/// Module for input handling and keybindings.
pub mod input;
pub use input::*;

/// Module for command definitions and command execution.
pub mod commands;
pub use commands::*;

/// Module for theme management and `ContentStyle` extensions.
pub mod theme;
pub use theme::*;

/// Module for the command palette UI and logic.
pub mod palette;
pub use palette::*;

/// Module for the statusline rendering and configuration.
pub mod statusline;
pub use statusline::*;

/// Module for editor hooks and event handling.
pub mod hooks;
pub use hooks::*;

/// Module for defining and managing editor layouts.
pub mod layout;
pub use layout::*;

/// Module for individual rendering chunks.
pub mod chunk;
pub use chunk::*;

/// Module for managing multiple rendering chunks.
pub mod chunks;
pub use chunks::*;
