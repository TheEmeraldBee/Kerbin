use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};

use ascii_forge::window::Window;
use tokio::sync::mpsc::UnboundedSender;
use toml::Value;
use uuid::Uuid;

pub use crate::*;

/// This state stores the String path of the configuration folder
#[derive(State)]
pub struct ConfigFolder(pub String);

/// This state stores the Uuid of the current editor process
#[derive(State)]
pub struct SessionUuid(pub Uuid);

/// Primary state marking whether the core editor is running at this moment.
///
/// When set to false, the editor will exit at the end of the current frame.
#[derive(State)]
pub struct Running(pub bool);

/// State storing plugin configuration as a map of plugin names to TOML `Value`.
///
/// This allows plugins to store and retrieve their configurations dynamically.
#[derive(State)]
pub struct PluginConfig(pub HashMap<String, Value>);

/// State for sending commands through an unbounded MPSC sender.
///
/// This provides a mechanism for different parts of the application to
/// enqueue commands to be processed asynchronously by the main event loop.
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

/// State wrapper around the `ascii_forge` window.
///
/// This allows the main `ascii_forge` window to be managed as part of the editor's state,
/// enabling other systems to access and manipulate it.
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
