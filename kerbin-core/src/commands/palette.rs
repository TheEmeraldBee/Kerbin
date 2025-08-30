use kerbin_macros::Command;

use crate::*;

#[derive(Command)]
pub enum PaletteCommand {
    /// Pushes the given string into the content of the palette
    PushPalette(String),

    /// Clears the content in the command palette
    ClearPalette,

    /// Executes the content in the command palette
    ExecutePalette,
}

impl Command for PaletteCommand {
    fn apply(&self, state: std::sync::Arc<State>) -> bool {
        let mut palette = state.palette.write().unwrap();

        match self {
            Self::PushPalette(content) => {
                palette.input.push_str(content.as_str());
                true
            }

            Self::ClearPalette => {
                palette.input.clear();
                true
            }

            Self::ExecutePalette => {
                let content = palette.input.clone();
                drop(palette);
                state.call_command(&content)
            }
        }
    }
}
