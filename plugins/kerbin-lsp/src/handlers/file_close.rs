use kerbin_core::*;
use lsp_types::{DidCloseTextDocumentParams, TextDocumentIdentifier};

use crate::{LspManager, OpenedFile};

pub async fn file_close(
    event_data: EventData<CloseEvent>,
    manager: ResMut<LspManager>,
    log: Res<LogSender>,
) {
    get!(Some(event_data), mut manager, log);

    let Some(file) = event_data.buffer.get_state::<OpenedFile>().await else {
        // LSP not on file, so ignore anyways
        return;
    };

    let lsp = manager.get_or_create_client(&file.lang).await.unwrap();

    log.high("lsp::file_closed", "Closed File for lsp!");

    lsp.notification(
        "textDocument/didClose",
        DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier::new(file.uri.clone()),
        },
    )
    .await
    .unwrap();
}
