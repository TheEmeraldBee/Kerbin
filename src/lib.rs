pub mod buffer;
pub use buffer::*;

pub mod commands;
pub use commands::*;

pub mod input;
use derive_more::{Deref, DerefMut};
pub use input::*;

pub mod key_check;

pub mod buffer_extensions;

pub mod plugin_manager;
pub use plugin_manager::*;

pub mod plugin_libs;

pub mod mode;
pub use mode::*;

pub mod command_palette;
pub use command_palette::*;

#[derive(Deref, DerefMut)]
pub struct Running(pub bool);
