use kerbin_core::*;
use lsp_types::{
    GotoDefinitionParams, GotoDefinitionResponse, Location, LocationLink, Position,
    ReferenceContext, ReferenceParams, TextDocumentIdentifier, TextDocumentPositionParams,
    WorkDoneProgressParams,
};

use crate::{diagnostics::Diagnostics, JsonRpcMessage, LspManager, OpenedFile, UriExt};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NavigationKind {
    Definition,
    References,
    Implementation,
    TypeDefinition,
    Declaration,
}

pub struct NavigationPending {
    pub request_id: i32,
    pub kind: NavigationKind,
    pub multi: Option<Vec<Token>>,
}

#[derive(State, Default)]
pub struct NavigationState {
    pub pending: Option<NavigationPending>,
}

#[derive(Debug, Clone, Command)]
pub enum NavigationCommand {
    #[command(drop_ident, name = "lsp-goto-definition")]
    GotoDefinition {
        #[command(flag, name = "multi", type_name = "[command]?")]
        multi: Option<Vec<Token>>,
    },
    #[command(drop_ident, name = "lsp-goto-references")]
    GotoReferences {
        #[command(flag, name = "multi", type_name = "[command]?")]
        multi: Option<Vec<Token>>,
    },
    #[command(drop_ident, name = "lsp-goto-implementation")]
    GotoImplementation {
        #[command(flag, name = "multi", type_name = "[command]?")]
        multi: Option<Vec<Token>>,
    },
    #[command(drop_ident, name = "lsp-goto-type-definition")]
    GotoTypeDefinition {
        #[command(flag, name = "multi", type_name = "[command]?")]
        multi: Option<Vec<Token>>,
    },
    #[command(drop_ident, name = "lsp-goto-declaration")]
    GotoDeclaration {
        #[command(flag, name = "multi", type_name = "[command]?")]
        multi: Option<Vec<Token>>,
    },
    /// Navigate to "file_path:line:col" (1-indexed). Used after picker selection.
    #[command(drop_ident, name = "lsp-goto-location")]
    GotoLocation { location: String },
    /// Aggregate all buffer diagnostics and open a picker.
    #[command(drop_ident, name = "lsp-goto-diagnostics")]
    GotoDiagnostics {
        #[command(flag, name = "multi", type_name = "[command]?")]
        multi: Option<Vec<Token>>,
        #[command(flag, name = "workspace")]
        workspace: bool,
    },
}

async fn send_goto_request(
    state: &mut State,
    kind: NavigationKind,
    multi: Option<Vec<Token>>,
) -> bool {
    let mut bufs = state.lock_state::<Buffers>().await;
    let mut lsps = state.lock_state::<LspManager>().await;

    let Some(mut buf) = bufs.cur_buffer_as_mut::<TextBuffer>().await else { return false; };

    let Some(file) = buf.get_state::<OpenedFile>().await else {
        return false;
    };

    let client = lsps
        .get_or_create_client(&file.lang)
        .await
        .expect("Lsp should exist");

    let cursor = buf.primary_cursor();
    let cursor_byte = cursor.get_cursor_byte().min(buf.len());

    let line = buf.byte_to_line_clamped(cursor_byte);
    let character = cursor_byte - buf.line_to_byte_clamped(line);

    let uri = file.uri.clone();

    let text_doc_pos = TextDocumentPositionParams {
        text_document: TextDocumentIdentifier { uri },
        position: Position::new(line as u32, character as u32),
    };

    resolver_engine_mut().await.remove_template("lsp_locations");

    let method = match kind {
        NavigationKind::Definition => "textDocument/definition",
        NavigationKind::References => "textDocument/references",
        NavigationKind::Implementation => "textDocument/implementation",
        NavigationKind::TypeDefinition => "textDocument/typeDefinition",
        NavigationKind::Declaration => "textDocument/declaration",
    };

    let request_id = if kind == NavigationKind::References {
        let params = ReferenceParams {
            text_document_position: text_doc_pos,
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
            context: ReferenceContext {
                include_declaration: true,
            },
        };
        client.request(method, params).await.ok()
    } else {
        let params = GotoDefinitionParams {
            text_document_position_params: text_doc_pos,
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        };
        client.request(method, params).await.ok()
    };

    let Some(request_id) = request_id else {
        return false;
    };

    let mut nav_state = buf.get_or_insert_state_mut(NavigationState::default).await;
    nav_state.pending = Some(NavigationPending {
        request_id,
        kind,
        multi,
    });

    true
}

#[async_trait::async_trait]
impl Command<State> for NavigationCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            Self::GotoDefinition { multi } => {
                send_goto_request(state, NavigationKind::Definition, multi.clone()).await
            }
            Self::GotoReferences { multi } => {
                send_goto_request(state, NavigationKind::References, multi.clone()).await
            }
            Self::GotoImplementation { multi } => {
                send_goto_request(state, NavigationKind::Implementation, multi.clone()).await
            }
            Self::GotoTypeDefinition { multi } => {
                send_goto_request(state, NavigationKind::TypeDefinition, multi.clone()).await
            }
            Self::GotoDeclaration { multi } => {
                send_goto_request(state, NavigationKind::Declaration, multi.clone()).await
            }
            Self::GotoDiagnostics { multi, workspace } => {
                if *workspace {
                    // Workspace-wide diagnostics from the global push-model store.
                    // publishDiagnostics notifications are stored for all files,
                    // including those not currently open as buffers.
                    let global = state.lock_state::<crate::GlobalDiagnostics>().await;
                    let mut entries: Vec<String> = Vec::new();

                    for (path, diags) in &global.0 {
                        for diag in diags {
                            entries.push(crate::diagnostics::format_diagnostic(path, diag));
                        }
                    }
                    drop(global);

                    if entries.is_empty() {
                        state.lock_state::<LogSender>().await.low("lsp", "No Diagnostics");
                        return false;
                    }

                    resolver_engine_mut().await.set_template("lsp_diagnostics", Token::list_from(entries));

                    if let Some(tokens) = multi.clone() {
                        let token_lists: Vec<Vec<Token>> =
                            if tokens.iter().all(|t| matches!(t, Token::List(_))) {
                                tokens
                                    .into_iter()
                                    .filter_map(|t| {
                                        if let Token::List(items) = t {
                                            Some(tokenize(&tokens_to_command_string(&items)).unwrap_or_default())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect()
                            } else {
                                vec![tokens]
                            };

                        for token_list in token_lists {
                            let command = state.lock_state::<CommandRegistry>().await.parse_command(
                                token_list,
                                true,
                                false,
                                Some(&resolver_engine().await.as_resolver()),
                                true,
                                &*state.lock_state::<CommandPrefixRegistry>().await,
                                &*state.lock_state::<ModeStack>().await,
                            );
                            if let Some(command) = command {
                                state.lock_state::<CommandSender>().await.send(command).unwrap();
                            }
                        }
                    }

                    true
                } else {
                    // Open-buffer diagnostics (existing behaviour)
                    let bufs = state.lock_state::<Buffers>().await;
                    let mut entries: Vec<String> = Vec::new();

                    for buf in &bufs.buffers {
                        let buf_guard = buf.read().await;
                        let Some(buf) = buf_guard.downcast::<TextBuffer>() else { continue };
                        let Some(file) = buf.get_state::<OpenedFile>().await else { continue };
                        let path = file.uri.path().to_string();
                        let Some(diagnostics) = buf.get_state::<Diagnostics>().await else { continue };
                        for diag in &diagnostics.0 {
                            entries.push(crate::diagnostics::format_diagnostic(&path, diag));
                        }
                    }
                    drop(bufs);

                    if entries.is_empty() {
                        state.lock_state::<LogSender>().await.low("lsp", "No Diagnostics");
                        return false;
                    }

                    resolver_engine_mut().await.set_template("lsp_diagnostics", Token::list_from(entries));

                    if let Some(tokens) = multi.clone() {
                        let token_lists: Vec<Vec<Token>> =
                            if tokens.iter().all(|t| matches!(t, Token::List(_))) {
                                tokens
                                    .into_iter()
                                    .filter_map(|t| {
                                        if let Token::List(items) = t {
                                            Some(
                                                tokenize(&tokens_to_command_string(&items))
                                                    .unwrap_or_default(),
                                            )
                                        } else {
                                            None
                                        }
                                    })
                                    .collect()
                            } else {
                                vec![tokens]
                            };

                        for token_list in token_lists {
                            let command = state.lock_state::<CommandRegistry>().await.parse_command(
                                token_list,
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
                        }
                    }

                    true
                }
            }
            Self::GotoLocation { location } => {
                // Parse "path:line:col" — use rsplitn(3) to handle Unix paths with colons
                let parts: Vec<&str> = location.rsplitn(3, ':').collect();
                if parts.len() < 3 {
                    return false;
                }
                let col_str = parts[0];
                let line_str = parts[1];
                let path = parts[2];

                let Ok(col_1indexed) = col_str.parse::<usize>() else {
                    return false;
                };
                let Ok(line_1indexed) = line_str.parse::<usize>() else {
                    return false;
                };

                // Convert to 0-indexed
                let line = line_1indexed.saturating_sub(1);
                let col = col_1indexed.saturating_sub(1);

                let default_tab_unit = state.lock_state::<CoreConfig>().await.default_tab_unit;
                let mut bufs = state.lock_state::<Buffers>().await;
                if bufs.open(path.to_string(), default_tab_unit).await.is_err() {
                    return false;
                }

                let Some(mut buf) = bufs.cur_buffer_as_mut::<TextBuffer>().await else { return false; };
                let line_byte = buf.line_to_byte_clamped(line);
                let line_end_byte = buf.line_to_byte_clamped(line + 1);
                let byte = buf
                    .slice(line_byte, line_end_byte)
                    .map(|s| {
                        let mut utf16_rem = col;
                        let mut byte_off = 0usize;
                        for ch in s.chars() {
                            if utf16_rem == 0 {
                                break;
                            }
                            let w = ch.len_utf16();
                            if utf16_rem < w {
                                break;
                            }
                            utf16_rem -= w;
                            byte_off += ch.len_utf8();
                        }
                        line_byte + byte_off
                    })
                    .unwrap_or(line_byte)
                    .min(buf.len());
                buf.primary_cursor_mut().set_sel(byte..=byte);

                true
            }
        }
    }
}

fn normalize_goto_response(r: GotoDefinitionResponse) -> Vec<Location> {
    match r {
        GotoDefinitionResponse::Scalar(loc) => vec![loc],
        GotoDefinitionResponse::Array(locs) => locs,
        GotoDefinitionResponse::Link(links) => links
            .into_iter()
            .map(|l: LocationLink| Location {
                uri: l.target_uri,
                range: l.target_selection_range,
            })
            .collect(),
    }
}

fn format_location(loc: &Location) -> String {
    let path = lsp_types::Uri::to_file_path(&loc.uri).unwrap_or_default();
    format!(
        "{}:{}:{}",
        path,
        loc.range.start.line + 1,
        loc.range.start.character + 1
    )
}

pub async fn handle_navigation(state: &State, msg: &JsonRpcMessage) {
    let JsonRpcMessage::Response(response) = msg else {
        return;
    };

    let bufs = state.lock_state::<Buffers>().await;

    let mut buffer = None;
    let mut pending_kind = None;
    let mut pending_multi: Option<Vec<Token>> = None;
    for buf in &bufs.buffers {
        let buf_guard = buf.read().await;
        if let Some(text_buf) = buf_guard.downcast::<TextBuffer>()
            && let Some(nav_state) = text_buf.get_state::<NavigationState>().await
            && let Some(pending) = &nav_state.pending
            && pending.request_id == response.id
        {
            pending_kind = Some(pending.kind);
            pending_multi = pending.multi.clone();
            buffer = Some(buf.clone());
            break;
        }
    }

    let (Some(buf), Some(kind)) = (buffer, pending_kind) else {
        return;
    };

    drop(bufs);

    {
        let mut buf_guard = buf.write_owned().await;
        if let Some(buf) = buf_guard.downcast_mut::<TextBuffer>()
            && let Some(mut nav_state) = buf.get_state_mut::<NavigationState>().await
        {
            nav_state.pending = None;
        }
    }

    let Some(result) = &response.result else {
        return;
    };

    let locations: Vec<Location> = if kind == NavigationKind::References {
        match serde_json::from_value::<Vec<Location>>(result.clone()) {
            Ok(locs) => locs,
            Err(_) => return,
        }
    } else {
        match serde_json::from_value::<GotoDefinitionResponse>(result.clone()) {
            Ok(resp) => normalize_goto_response(resp),
            Err(_) => return,
        }
    };

    if locations.is_empty() {
        state
            .lock_state::<LogSender>()
            .await
            .low("lsp", "No Locations Found");
        return;
    }

    if locations.len() == 1 {
        let loc = &locations[0];
        let Some(path) = lsp_types::Uri::to_file_path(&loc.uri) else {
            return;
        };
        let line = loc.range.start.line as usize;
        let col = loc.range.start.character as usize;

        let default_tab_unit = state.lock_state::<CoreConfig>().await.default_tab_unit;
        let mut bufs = state.lock_state::<Buffers>().await;
        if bufs.open(path, default_tab_unit).await.is_err() {
            return;
        }
        let Some(mut buf) = bufs.cur_buffer_as_mut::<TextBuffer>().await else { return; };
        let line_byte = buf.line_to_byte_clamped(line);
        let byte = (line_byte + col).min(buf.len());
        buf.primary_cursor_mut().set_sel(byte..=byte);
    } else {
        let formatted: Vec<String> = locations.iter().map(format_location).collect();
        resolver_engine_mut()
            .await
            .set_template("lsp_locations", Token::list_from(formatted));

        // If --multi was provided, parse and send those commands.
        // Supports [[cmd1] [cmd2]] (all-list) or [cmd] (single command).
        if let Some(tokens) = pending_multi {
            let token_lists: Vec<Vec<Token>> = if tokens.iter().all(|t| matches!(t, Token::List(_)))
            {
                tokens
                    .into_iter()
                    .filter_map(|t| {
                        if let Token::List(items) = t {
                            Some(tokenize(&tokens_to_command_string(&items)).unwrap_or_default())
                        } else {
                            None
                        }
                    })
                    .collect()
            } else {
                vec![tokens]
            };

            for token_list in token_lists {
                let command = state.lock_state::<CommandRegistry>().await.parse_command(
                    token_list,
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
            }
        }
    }
}
