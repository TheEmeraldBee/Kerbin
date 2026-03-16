use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    path::PathBuf,
};

use ascii_forge::{prelude::Color, window::Window};
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

pub use crate::*;

/// Stores the path of the configuration folder
#[derive(State)]
pub struct ConfigFolder(pub String);

/// Stores the Uuid of the current editor process
#[derive(State)]
pub struct SessionUuid(pub Uuid);

/// Primary state marking whether the core editor is running
#[derive(State)]
pub struct Running(pub bool);

/// State for sending commands through an unbounded MPSC sender
#[derive(State)]
pub struct CommandSender(pub UnboundedSender<Box<dyn Command>>);

impl Deref for CommandSender {
    type Target = UnboundedSender<Box<dyn Command>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for CommandSender {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Config directory used to resolve `source` paths in .kb files
#[derive(State)]
pub struct ConfigDir(pub PathBuf);

/// Core runtime settings (framerate, etc.)
#[derive(State)]
pub struct CoreConfig {
    pub framerate: u64,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self { framerate: 60 }
    }
}

/// Stores the list of configured debounce events
#[derive(State, Default)]
pub struct DebounceConfig(pub Vec<DebounceEvent>);

/// Resolved palette: name → Color
#[derive(State, Default)]
pub struct PaletteState(pub HashMap<String, Color>);

/// State wrapper around the ascii_forge window
#[derive(State)]
pub struct WindowState(pub Window);

impl Deref for WindowState {
    type Target = Window;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for WindowState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
