use kerbin_macros::Command;
use kerbin_state_machine::State;

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

#[async_trait::async_trait]
impl Command for PaletteCommand {
    async fn apply(&self, state: &mut State) -> bool {
        let mut palette = state.lock_state::<CommandPaletteState>().await.unwrap();

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
                let command = state
                    .lock_state::<CommandRegistry>()
                    .await
                    .unwrap()
                    .parse_command(
                        CommandRegistry::split_command(&content),
                        true,
                        false,
                        &state.lock_state::<CommandPrefixRegistry>().await.unwrap(),
                        &state.lock_state::<ModeStack>().await.unwrap(),
                    );
                if let Some(command) = command {
                    state
                        .lock_state::<CommandSender>()
                        .await
                        .unwrap()
                        .send(command)
                        .unwrap();
                }
                false
            }
        }
    }
}
