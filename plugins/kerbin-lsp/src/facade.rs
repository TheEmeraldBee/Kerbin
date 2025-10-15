use std::io;

use lsp_types::*;
use tokio::io::AsyncWrite;

use crate::{LspClient, UriExt};

#[allow(async_fn_in_trait)]
/// Internal trait for implementing a simple facade to make
/// working with the client less clunky
///
/// **DO NOT IMPLEMENT**
pub trait ClientFacade<State> {
    async fn init(&mut self, root_uri: Uri) -> io::Result<i32>;
    async fn open(&self, path: impl ToString) -> io::Result<()>;
    async fn wait_for_response<T: serde::de::DeserializeOwned>(
        &mut self,
        id: i32,
        state: &mut State,
    ) -> io::Result<T>;
    async fn code_lens(&mut self, uri: Uri, state: &mut State)
    -> io::Result<Option<Vec<CodeLens>>>;
    async fn document_symbols(
        &mut self,
        uri: Uri,
        state: &mut State,
    ) -> io::Result<Option<DocumentSymbolResponse>>;
    async fn hover(
        &mut self,
        uri: Uri,
        position: Position,
        state: &mut State,
    ) -> io::Result<Option<Hover>>;
    async fn diagnostics(
        &mut self,
        uri: Uri,
        state: &mut State,
    ) -> io::Result<Option<Vec<Diagnostic>>>;
    async fn completion(
        &mut self,
        uri: Uri,
        position: Position,
        state: &mut State,
    ) -> io::Result<Option<CompletionResponse>>;
    async fn definition(
        &mut self,
        uri: Uri,
        position: Position,
        state: &mut State,
    ) -> io::Result<Option<GotoDefinitionResponse>>;
    async fn references(
        &mut self,
        uri: Uri,
        position: Position,
        state: &mut State,
    ) -> io::Result<Option<Vec<Location>>>;
    async fn code_action(
        &mut self,
        uri: Uri,
        range: Range,
        state: &mut State,
    ) -> io::Result<Option<CodeActionResponse>>;
    async fn rename(
        &mut self,
        uri: Uri,
        position: Position,
        new_name: String,
        state: &mut State,
    ) -> io::Result<Option<WorkspaceEdit>>;
    async fn formatting(
        &mut self,
        uri: Uri,
        state: &mut State,
    ) -> io::Result<Option<Vec<TextEdit>>>;
    async fn wait_for_indexing(&mut self, state: &mut State, timeout_ms: u64) -> io::Result<()>;
}

impl<W: AsyncWrite + Unpin + Send + 'static, State> ClientFacade<State> for LspClient<W, State> {
    async fn init(&mut self, root_uri: Uri) -> io::Result<i32> {
        // Initialize
        let init_params = InitializeParams {
            process_id: Some(std::process::id()),
            capabilities: ClientCapabilities {
                window: Some(WindowClientCapabilities {
                    work_done_progress: Some(true),
                    ..Default::default()
                }),
                workspace: Some(WorkspaceClientCapabilities {
                    apply_edit: Some(true),
                    ..Default::default()
                }),
                text_document: Some(TextDocumentClientCapabilities {
                    code_lens: Some(CodeLensClientCapabilities {
                        dynamic_registration: Some(true),
                    }),
                    hover: Some(HoverClientCapabilities {
                        dynamic_registration: Some(true),
                        content_format: Some(vec![MarkupKind::Markdown, MarkupKind::PlainText]),
                    }),
                    completion: Some(CompletionClientCapabilities {
                        dynamic_registration: Some(true),
                        completion_item: Some(CompletionItemCapability {
                            snippet_support: Some(true),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    definition: Some(GotoCapability {
                        dynamic_registration: Some(true),
                        link_support: Some(true),
                    }),
                    references: Some(ReferenceClientCapabilities {
                        dynamic_registration: Some(true),
                    }),
                    document_symbol: Some(DocumentSymbolClientCapabilities {
                        dynamic_registration: Some(true),
                        hierarchical_document_symbol_support: Some(true),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: root_uri,
                name: "workspace".to_string(),
            }]),
            ..Default::default()
        };

        self.request("initialize", init_params).await
    }

    async fn open(&self, path: impl ToString) -> io::Result<()> {
        let path_str = path.to_string();
        let text = tokio::fs::read_to_string(&path_str).await?;

        self.notification(
            "textDocument/didOpen",
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: Uri::file_path(&path_str).map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidInput, "Invalid file path")
                    })?,
                    language_id: "rust".to_string(),
                    version: 0,
                    text,
                },
            },
        )
        .await?;
        Ok(())
    }

    async fn wait_for_response<T: serde::de::DeserializeOwned>(
        &mut self,
        id: i32,
        state: &mut State,
    ) -> io::Result<T> {
        for _ in 0..100 {
            self.process_events(state);

            if let Some(result) = self.response::<T>(id) {
                return result.map_err(|e| {
                    io::Error::new(io::ErrorKind::Other, format!("LSP error: {:?}", e))
                });
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }

        Err(io::Error::new(io::ErrorKind::TimedOut, "Response timeout"))
    }

    async fn code_lens(
        &mut self,
        uri: Uri,
        state: &mut State,
    ) -> io::Result<Option<Vec<CodeLens>>> {
        let id = self
            .request(
                "textDocument/codeLens",
                CodeLensParams {
                    text_document: TextDocumentIdentifier { uri },
                    work_done_progress_params: Default::default(),
                    partial_result_params: Default::default(),
                },
            )
            .await?;

        self.wait_for_response(id, state).await
    }

    async fn document_symbols(
        &mut self,
        uri: Uri,
        state: &mut State,
    ) -> io::Result<Option<DocumentSymbolResponse>> {
        let id = self
            .request(
                "textDocument/documentSymbol",
                DocumentSymbolParams {
                    text_document: TextDocumentIdentifier { uri },
                    work_done_progress_params: Default::default(),
                    partial_result_params: Default::default(),
                },
            )
            .await?;

        self.wait_for_response(id, state).await
    }

    async fn hover(
        &mut self,
        uri: Uri,
        position: Position,
        state: &mut State,
    ) -> io::Result<Option<Hover>> {
        let id = self
            .request(
                "textDocument/hover",
                HoverParams {
                    text_document_position_params: TextDocumentPositionParams {
                        text_document: TextDocumentIdentifier { uri },
                        position,
                    },
                    work_done_progress_params: Default::default(),
                },
            )
            .await?;

        self.wait_for_response(id, state).await
    }

    async fn diagnostics(
        &mut self,
        _uri: Uri,
        _state: &mut State,
    ) -> io::Result<Option<Vec<Diagnostic>>> {
        // Diagnostics are pushed by the server, not requested
        // This is a placeholder - actual implementation would need to track diagnostics in state
        Ok(None)
    }

    async fn completion(
        &mut self,
        uri: Uri,
        position: Position,
        state: &mut State,
    ) -> io::Result<Option<CompletionResponse>> {
        let id = self
            .request(
                "textDocument/completion",
                CompletionParams {
                    text_document_position: TextDocumentPositionParams {
                        text_document: TextDocumentIdentifier { uri },
                        position,
                    },
                    work_done_progress_params: Default::default(),
                    partial_result_params: Default::default(),
                    context: None,
                },
            )
            .await?;

        self.wait_for_response(id, state).await
    }

    async fn definition(
        &mut self,
        uri: Uri,
        position: Position,
        state: &mut State,
    ) -> io::Result<Option<GotoDefinitionResponse>> {
        let id = self
            .request(
                "textDocument/definition",
                GotoDefinitionParams {
                    text_document_position_params: TextDocumentPositionParams {
                        text_document: TextDocumentIdentifier { uri },
                        position,
                    },
                    work_done_progress_params: Default::default(),
                    partial_result_params: Default::default(),
                },
            )
            .await?;

        self.wait_for_response(id, state).await
    }

    async fn references(
        &mut self,
        uri: Uri,
        position: Position,
        state: &mut State,
    ) -> io::Result<Option<Vec<Location>>> {
        let id = self
            .request(
                "textDocument/references",
                ReferenceParams {
                    text_document_position: TextDocumentPositionParams {
                        text_document: TextDocumentIdentifier { uri },
                        position,
                    },
                    work_done_progress_params: Default::default(),
                    partial_result_params: Default::default(),
                    context: ReferenceContext {
                        include_declaration: true,
                    },
                },
            )
            .await?;

        self.wait_for_response(id, state).await
    }

    async fn code_action(
        &mut self,
        uri: Uri,
        range: Range,
        state: &mut State,
    ) -> io::Result<Option<CodeActionResponse>> {
        let id = self
            .request(
                "textDocument/codeAction",
                CodeActionParams {
                    text_document: TextDocumentIdentifier { uri },
                    range,
                    context: CodeActionContext {
                        diagnostics: vec![],
                        only: None,
                        trigger_kind: None,
                    },
                    work_done_progress_params: Default::default(),
                    partial_result_params: Default::default(),
                },
            )
            .await?;

        self.wait_for_response(id, state).await
    }

    async fn rename(
        &mut self,
        uri: Uri,
        position: Position,
        new_name: String,
        state: &mut State,
    ) -> io::Result<Option<WorkspaceEdit>> {
        let id = self
            .request(
                "textDocument/rename",
                RenameParams {
                    text_document_position: TextDocumentPositionParams {
                        text_document: TextDocumentIdentifier { uri },
                        position,
                    },
                    new_name,
                    work_done_progress_params: Default::default(),
                },
            )
            .await?;

        self.wait_for_response(id, state).await
    }

    async fn formatting(
        &mut self,
        uri: Uri,
        state: &mut State,
    ) -> io::Result<Option<Vec<TextEdit>>> {
        let id = self
            .request(
                "textDocument/formatting",
                DocumentFormattingParams {
                    text_document: TextDocumentIdentifier { uri },
                    options: FormattingOptions {
                        tab_size: 4,
                        insert_spaces: true,
                        ..Default::default()
                    },
                    work_done_progress_params: Default::default(),
                },
            )
            .await?;

        self.wait_for_response(id, state).await
    }

    async fn wait_for_indexing(&mut self, state: &mut State, timeout_ms: u64) -> io::Result<()> {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(timeout_ms);

        loop {
            self.process_events(state);

            // Check timeout
            if start.elapsed() > timeout {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "Indexing timeout"));
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }
}
