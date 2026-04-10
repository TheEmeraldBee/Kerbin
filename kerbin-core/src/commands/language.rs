use crate::*;

fn tokens_to_strings(tokens: &[Token]) -> Vec<String> {
    tokens
        .iter()
        .filter_map(|t| {
            if let Token::Word(s) = t {
                Some(s.clone())
            } else {
                None
            }
        })
        .collect()
}

#[derive(Debug, Clone, Command)]
pub enum RegisterLanguageCommand {
    /// Register a language name and the file patterns that detect it.
    #[command(drop_ident, name = "register_language")]
    Register {
        name: String,
        #[command(flag)]
        exts: Option<Vec<Token>>,
        #[command(flag)]
        filenames: Option<Vec<Token>>,
        #[command(flag)]
        regex: Option<String>,
    },
}

#[async_trait::async_trait]
impl Command<State> for RegisterLanguageCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            Self::Register {
                name,
                exts,
                filenames,
                regex,
            } => {
                let ext_strings = exts.as_deref().map(tokens_to_strings).unwrap_or_default();
                let filename_strings = filenames
                    .as_deref()
                    .map(tokens_to_strings)
                    .unwrap_or_default();

                let mut registry = state.lock_state::<FiletypeRegistry>().await;
                for ext in &ext_strings {
                    registry.register_ext(ext.to_lowercase(), name);
                }
                for filename in &filename_strings {
                    registry.register_filename(filename, name);
                }
                if let Some(pattern) = regex {
                    registry.register_first_line(pattern, name);
                }
            }
        }
        false
    }
}
