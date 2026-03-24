use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    path::PathBuf,
};

use crossterm::event::Event;
use ratatui::{Terminal, backend::CrosstermBackend, layout::Rect, style::Color};
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
    pub disable_auto_pairs: bool,
    pub tab_display_unit: String,
    pub default_tab_unit: usize,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            framerate: 60,
            disable_auto_pairs: false,
            tab_display_unit: "    ".to_string(),
            default_tab_unit: 4,
        }
    }
}

/// Layout dimensions for the editor's chrome (gutter, statusline, bufferline).
/// Plugins may mutate these during init (before `PostInit`) to resize or hide chrome areas.
#[derive(State)]
pub struct LayoutConfig {
    pub bufferline_height: u16,
    pub statusline_height: u16,
    pub gutter_width: u16,
    pub gutter_pad: u16,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            bufferline_height: 1,
            statusline_height: 1,
            gutter_width: 5,
            gutter_pad: 2,
        }
    }
}

/// Stores the list of configured debounce events
#[derive(State, Default)]
pub struct DebounceConfig(pub Vec<DebounceEvent>);

/// Resolved palette: name → Color
#[derive(State, Default)]
pub struct PaletteState(pub HashMap<String, Color>);

/// State wrapper around the ratatui terminal
#[derive(State)]
pub struct WindowState(pub Terminal<CrosstermBackend<std::io::Stdout>>);

impl WindowState {
    /// Returns the terminal's current size as a `Rect`
    pub fn size(&self) -> Rect {
        let s = self.0.size().unwrap_or_default();
        Rect::new(0, 0, s.width, s.height)
    }
}

impl Deref for WindowState {
    type Target = Terminal<CrosstermBackend<std::io::Stdout>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for WindowState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Stores input events captured from crossterm for the current frame
#[derive(State, Default)]
pub struct CrosstermEvents(pub Vec<Event>);

/// Accumulates config load errors for user-visible reporting via `config-errors`.
#[derive(State, Default)]
pub struct ConfigErrors(pub Vec<KbLoadError>);

/// Identifies a mouse event type for binding purposes.
/// Named `MouseTrigger` to avoid conflict with `crossterm::event::MouseEvent`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MouseTrigger {
    LeftDown,
    LeftUp,
    RightDown,
    RightUp,
    MiddleDown,
    ScrollUp,
    ScrollDown,
}

/// Mouse event bindings: maps mouse triggers to lists of command strings.
#[derive(Default, State)]
pub struct MouseBindings {
    pub bindings: HashMap<MouseTrigger, Vec<String>>,
}

