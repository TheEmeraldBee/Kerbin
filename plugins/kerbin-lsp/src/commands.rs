use kerbin_core::*;

use crate::{
    handlers::file_open::OpenedFile,
    manager::{LangInfo, LspManager},
};

async fn resolve_target_lang(lang: Option<&str>, state: &State) -> Option<String> {
    if let Some(l) = lang {
        return Some(l.to_string());
    }
    let bufs = state.lock_state::<Buffers>().await;
    let buf = bufs.cur_text_buffer().await?;
    let file = buf.get_state::<OpenedFile>().await?;
    Some(file.lang.clone())
}

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

    /// Show the status of a language server (defaults to current buffer's language).
    #[command(drop_ident, name = "lsp_status")]
    Status {
        lang: Option<String>,
    },

    /// Kill and respawn a language server (defaults to current buffer's language).
    #[command(drop_ident, name = "lsp_restart")]
    Restart {
        lang: Option<String>,
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
            LspCommand::Status { lang } => {
                let Some(target_lang) = resolve_target_lang(lang.as_deref(), &state).await else {
                    state.lock_state::<LogSender>().await.low("lsp", "no LSP language for current buffer");
                    return false;
                };
                let manager = state.lock_state::<LspManager>().await;
                let status = manager.lang_status(&target_lang);
                state.lock_state::<LogSender>().await.low("lsp", &format!("{target_lang}: {status}"));
            }
            LspCommand::Restart { lang } => {
                let Some(target_lang) = resolve_target_lang(lang.as_deref(), &state).await else {
                    state.lock_state::<LogSender>().await.low("lsp", "no LSP language for current buffer");
                    return false;
                };

                if !state.lock_state::<LspManager>().await.reset_client(&target_lang) {
                    state.lock_state::<LogSender>().await.low("lsp", &format!("{target_lang}: no running client to restart"));
                    return false;
                }

                // Clear the lsp_opened flag so open_files re-opens all affected buffers
                let bufs = state.lock_state::<Buffers>().await;
                for buf in &bufs.buffers {
                    let mut buf_guard = buf.clone().write_owned().await;
                    if let Some(text_buf) = buf_guard.downcast_mut::<TextBuffer>() {
                        let is_match = text_buf
                            .get_state::<OpenedFile>()
                            .await
                            .map_or(false, |f| f.lang == target_lang);
                        if is_match {
                            text_buf.flags.remove("lsp_opened");
                        }
                    }
                }

                state.lock_state::<LogSender>().await.low("lsp", &format!("{target_lang}: restarted"));
            }
        }
        false
    }
}
