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
        #[command(flag)]
        format_on_save: bool,
        #[command(flag)]
        lsp_format: bool,
        #[command(flag)]
        external_formatter: Option<Vec<Token>>,
    },
}

#[async_trait::async_trait]
impl Command<State> for LspCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            LspCommand::Register {
                name,
                exts,
                cmd,
                args,
                roots,
                format_on_save,
                lsp_format,
                external_formatter,
            } => {
                let ext_strings = tokens_to_strings(exts);
                let arg_strings = args.as_deref().map(tokens_to_strings).unwrap_or_default();
                let root_strings = roots.as_deref().map(tokens_to_strings).unwrap_or_default();

                let info = LangInfo::new(cmd)
                    .with_args(arg_strings)
                    .with_roots(root_strings);

                let info = if *lsp_format {
                    info.with_lsp_format(*format_on_save)
                } else if let Some(tokens) = external_formatter {
                    let parts = tokens_to_strings(tokens);
                    if let Some((ext_cmd, ext_args)) = parts.split_first() {
                        info.with_external_format(ext_cmd, ext_args.to_vec(), *format_on_save)
                    } else {
                        info
                    }
                } else {
                    info
                };

                {
                    let mut manager = state.lock_state::<LspManager>().await;
                    manager.register_language(name, info);
                }

                // Register filetype + extensions in central registry
                {
                    let mut registry = state.lock_state::<FiletypeRegistry>().await;
                    registry.register(name, "lsp");
                    for ext in &ext_strings {
                        registry.register_ext(ext.to_lowercase(), name);
                    }
                }

                // Register hook once on the filetype name
                state
                    .on_hook(kerbin_core::hooks::UpdateFiletype::new(name))
                    .system(crate::open_files)
                    .system(crate::apply_changes)
                    .system(crate::render_diagnostic_highlights)
                    .system(crate::process_lsp_events)
                    .system(crate::render_hover)
                    .system(crate::update_completions)
                    .system(crate::render_completions);
            }
        }
        false
    }
}
