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
                text_document: Some(TextDocumentClientCapabilities {
                    diagnostic: Some(DiagnosticClientCapabilities {
                        dynamic_registration: None,
                        related_document_support: None,
                    }),
                    publish_diagnostics: Some(PublishDiagnosticsClientCapabilities {
                        version_support: Some(false),
                        code_description_support: Some(true),
                        ..Default::default()
                    }),
                    hover: Some(HoverClientCapabilities {
                        dynamic_registration: None,
                        content_format: None,
                    }),
                    completion: Some(CompletionClientCapabilities {
                        dynamic_registration: None,

                        completion_item: None,
                        completion_list: None,
                        completion_item_kind: None,

                        insert_text_mode: None,
                        context_support: None,
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
