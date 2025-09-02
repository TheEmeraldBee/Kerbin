#![allow(improper_ctypes_definitions)]

pub use kerbin_state_machine::{
    State,
    storage::StateStorage,
    system::param::{res::Res, res_mut::ResMut, *},
};

pub use kerbin_plugin::Plugin;

pub mod regex;
pub use regex::*;

pub mod grammar;
pub use grammar::*;

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
