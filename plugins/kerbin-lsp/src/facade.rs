use std::io;

use lsp_types::*;
use tokio::io::AsyncWrite;

use crate::{LspClient, UriExt};

#[allow(async_fn_in_trait)]
/// Internal trait for implementing a simple facade to make
/// working with the client less clunky
///
/// **DO NOT IMPLEMENT**
pub trait ClientFacade {
    async fn init(&mut self, root_uri: Uri) -> io::Result<i32>;
    async fn open(&self, path: impl ToString) -> io::Result<()>;
}

impl<W: AsyncWrite + Unpin + Send + 'static> ClientFacade for LspClient<W> {
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
}
