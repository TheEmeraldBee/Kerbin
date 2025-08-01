#[macro_export]
macro_rules! term_print {
    ($format:expr $(,$args:expr)* $(,)?) => {
        crokey::crossterm::execute!(std::io::stdout(), crokey::crossterm::cursor::MoveTo(50, 10), crokey::crossterm::style::Print(format!($format, $($args)*))).unwrap()
    };
}

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

pub mod grammar;
pub use grammar::*;

pub mod highlight;
pub use highlight::*;

pub mod theme;
pub use theme::*;

#[derive(Deref, DerefMut)]
pub struct Running(pub bool);
