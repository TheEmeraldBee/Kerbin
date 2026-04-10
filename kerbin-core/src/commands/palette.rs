use crate::*;

#[derive(Command)]
pub enum PaletteCommand {
    #[command]
    /// Pushes the given string into the content of the palette
    PushPalette(String),

    #[command]
    /// Pops items from the palette string
    PopPalette(usize),

    #[command]
    /// Clears the content in the command palette
    ClearPalette,

    #[command]
    /// Executes the content in the command palette
    ExecutePalette,

    #[command]
    /// Autocompletes the palette command
    CompletePalette,
}

#[async_trait::async_trait]
impl Command<State> for PaletteCommand {
    async fn apply(&self, state: &mut State) -> bool {
        let mut palette = state.lock_state::<CommandPaletteState>().await;

        match self {
            Self::PushPalette(content) => {
                palette.input.push_str(content.as_str());
                true
            }

            Self::PopPalette(chars) => {
                for _ in 0..*chars {
                    palette.input.pop();
                    if palette.input.is_empty() {
                        return false;
                    }
                }

                true
            }

            Self::ClearPalette => {
                palette.input.clear();
                true
            }

            Self::ExecutePalette => {
                let content = palette.input.clone();
                drop(palette);

                let tokens = tokenize(&content).unwrap_or_default();

                let resolver_engine = resolver_engine().await;
                let resolver = resolver_engine.as_resolver();

                let command = state.lock_state::<CommandRegistry>().await.parse_command(
                    tokens,
                    true,
                    false,
                    Some(&resolver),
                    true,
                    &*state.lock_state::<CommandPrefixRegistry>().await,
                    &*state.lock_state::<ModeStack>().await,
                );

                drop(resolver);
                drop(resolver_engine);
                if let Some(command) = command {
                    if let Err(e) = state.lock_state::<CommandSender>().await.send(command) {
                        state
                            .lock_state::<LogSender>()
                            .await
                            .high("palette", format!("Failed to send command: {e}"));
                    }
                } else {
                    state
                        .lock_state::<LogSender>()
                        .await
                        .medium("palette", format!("Invalid command: {content}"));
                }
                false
            }

            Self::CompletePalette => {
                if let Some(done) = &palette.completion {
                    palette.input = done.to_string()
                }

                false
            }
        }
    }
}
