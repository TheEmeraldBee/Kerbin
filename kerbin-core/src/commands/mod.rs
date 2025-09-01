use ascii_forge::prelude::*;
use kerbin_state_machine::State;

use crate::Theme;

pub mod state;
pub use state::*;

pub mod buffer;
pub use buffer::*;

pub mod mode;
pub use mode::*;

pub mod shell;
pub use shell::*;

pub mod motion;
pub use motion::*;

pub mod cursor;
pub use cursor::*;

pub mod palette;
pub use palette::*;

pub trait Command: Send + Sync {
    fn apply(&self, state: &mut State) -> bool;
}

#[derive(Debug)]
pub struct CommandInfo {
    pub valid_names: Vec<String>,
    pub args: Vec<(String, String)>,
    pub desc: Vec<String>,
}

impl CommandInfo {
    pub fn new(
        names: impl IntoIterator<Item = impl ToString>,
        args: impl IntoIterator<Item = (impl ToString, impl ToString)>,
        desc: impl IntoIterator<Item = impl ToString>,
    ) -> Self {
        Self {
            valid_names: names.into_iter().map(|x| x.to_string()).collect(),
            args: args
                .into_iter()
                .map(|x| (x.0.to_string(), x.1.to_string()))
                .collect(),
            desc: desc.into_iter().map(|x| x.to_string()).collect(),
        }
    }

    pub fn check_name(&self, name: impl ToString) -> bool {
        self.valid_names.contains(&name.to_string())
    }

    pub fn desc_buf(&self, theme: &Theme) -> Option<Buffer> {
        if self.desc.is_empty() {
            return None;
        }

        let mut buf = Buffer::new((100, 100));

        let desc_style = theme.get_fallback_default(["ui.commandline.desc", "ui.text"]);

        for (i, text) in self.desc.iter().enumerate() {
            render!(buf, (0, i as u16) => [desc_style.apply(&text)]);
        }

        buf.shrink();

        Some(buf)
    }

    pub fn as_suggestion(&self, will_autocomplete: bool, theme: &Theme) -> Buffer {
        let mut buf = Buffer::new((500, 1));
        let mut loc = render!(
            buf,
            (0, 0) =>
            [
                StyledContent::new(theme.get_fallback_default(if will_autocomplete {
                    [
                        "ui.commandline.auto_name",
                        "ui.commandline.primary_name",
                        "ui.commandline.names",
                        "ui.text",
                    ]
                        .as_slice()
                } else {[
                    "ui.commandline.primary_name",
                    "ui.commandline.names",
                    "ui.text"
                ].as_slice()}), &self.valid_names[0]),
                " ",
            ]
        );
        if self.valid_names.len() >= 2 {
            loc = render!(buf, loc => [
                theme.get_fallback_default(["ui.commandline.names", "ui.text"]).apply(format!("({}) ", self.valid_names[1..].join(", ")))
            ]);
        }
        let name_style = theme.get_fallback_default(["ui.commandline.arg_name", "ui.text"]);
        let type_style = theme.get_fallback_default([
            "ui.commandline.arg_type",
            "ui.commandline.arg_name",
            "ui.text",
        ]);

        for (name, ty) in &self.args {
            loc = render!(buf, loc => [
                name_style.apply(name),
                ": ",
                type_style.apply(ty),
                " ",
            ]);
        }

        buf.shrink();
        buf
    }
}

/// This trait will allow you to use commands from 'c' mode. This will give you verification info,
/// as well as argument expectations and types. This shouldn't need to be implemented manually.
/// Just use the #[derive(Command)] and the additional attributes on the struct.
pub trait AsCommandInfo: Command + CommandFromStr {
    fn infos() -> Vec<CommandInfo>;
}

/// This trait should be implemented on anything you want to be able to define within a config
/// This will turn the command into an executable command based on the string input.
/// Used for config, as well as the command pallette Serde + a parsing library can make this much
/// easier to implement
pub trait CommandFromStr: Command {
    fn from_str(val: &[String]) -> Option<Result<Box<dyn Command>, String>>;
}
