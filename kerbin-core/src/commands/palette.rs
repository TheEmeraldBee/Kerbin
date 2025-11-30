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
        let mut palette = state.lock_state::<CommandPaletteState>().await;

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
                let command = state.lock_state::<CommandRegistry>().await.parse_command(
                    word_split(&content),
                    true,
                    false,
                    Some(&resolver_engine().await.as_resolver()),
                    true,
                    &*state.lock_state::<CommandPrefixRegistry>().await,
                    &*state.lock_state::<ModeStack>().await,
                );
                if let Some(command) = command {
                    state
                        .lock_state::<CommandSender>()
                        .await
                        .send(command)
                        .unwrap();
                }
                false
            }
        }
    }
}
