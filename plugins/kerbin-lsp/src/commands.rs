use kerbin_core::*;

use crate::manager::{LangInfo, LspManager};

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
pub enum LspCommand {
    /// Register a language server for a set of file extensions.
    #[command(drop_ident, name = "lsp-register")]
    Register {
        name: String,
        #[command(flag)]
        exts: Vec<Token>,
        #[command(flag)]
        cmd: String,
        #[command(flag)]
        args: Option<Vec<Token>>,
        #[command(flag)]
        roots: Option<Vec<Token>>,
    },
}

#[async_trait::async_trait]
impl Command for LspCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            LspCommand::Register {
                name,
                exts,
                cmd,
                args,
                roots,
            } => {
                let ext_strings = tokens_to_strings(exts);
                let arg_strings = args.as_deref().map(tokens_to_strings).unwrap_or_default();
                let root_strings = roots.as_deref().map(tokens_to_strings).unwrap_or_default();

                let info = LangInfo::new(cmd)
                    .with_args(arg_strings)
                    .with_roots(root_strings);

                {
                    let mut manager = state.lock_state::<LspManager>().await;
                    manager.register_language(name, ext_strings.clone(), info);
                }

                for ext in ext_strings {
                    state
                        .on_hook(kerbin_core::hooks::UpdateFiletype::new(&ext))
                        .system(crate::open_files)
                        .system(crate::apply_changes)
                        .system(crate::render_diagnostic_highlights)
                        .system(crate::process_lsp_events)
                        .system(crate::render_hover)
                        .system(crate::update_completions)
                        .system(crate::render_completions);
                }
            }
        }
        false
    }
}
