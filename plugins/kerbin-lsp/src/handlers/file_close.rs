use kerbin_core::*;
use lsp_types::{DidCloseTextDocumentParams, TextDocumentIdentifier};

use crate::{LspManager, OpenedFile};

pub async fn file_close(event_data: EventData<CloseEvent>, manager: ResMut<LspManager>) {
    get!(Some(event_data), mut manager);

    let locked = event_data.buffer.read().await;
    let Some(text_buf) = locked.downcast::<TextBuffer>() else { return; };
    let Some(file) = text_buf.get_state::<OpenedFile>().await else {
        return;
    };

    let lsp = manager.get_or_create_client(&file.lang).await.unwrap();

    lsp.notification(
        "textDocument/didClose",
        DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier::new(file.uri.clone()),
        },
    )
    .await
    .unwrap();
}
