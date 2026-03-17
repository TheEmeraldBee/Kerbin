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
impl Command for PaletteCommand {
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
                let resolver = resolver_engine().await;
                let resolver = resolver.as_resolver();

                let mut expansion_errors: Vec<String> = Vec::new();
                let expanded = resolver.expand_tokens_reporting(tokens, true, &mut expansion_errors);

                let log = state.lock_state::<LogSender>().await;
                for err in &expansion_errors {
                    log.high("palette", err);
                }
                drop(log);

                if !expansion_errors.is_empty() {
                    return false;
                }

                drop(resolver);

                let command = state.lock_state::<CommandRegistry>().await.parse_command(
                    expanded,
                    true,
                    false,
                    None,
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
