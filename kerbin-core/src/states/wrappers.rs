use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};

use ascii_forge::window::Window;
use serde::de::DeserializeOwned;
use tokio::sync::mpsc::UnboundedSender;
use toml::Value;
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

/// Stores plugin configuration as a map of plugin names to TOML Values
#[derive(State)]
pub struct PluginConfig(pub HashMap<String, Value>);

impl PluginConfig {
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Option<Result<T, String>> {
        self.0
            .get(key)
            .map(|x| x.clone().try_into().map_err(|x| format!("{x}")))
    }
}

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
