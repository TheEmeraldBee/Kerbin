use crate::*;
use kerbin_core::*;

pub async fn apply_changes(buffers: ResMut<Buffers>, lsp_manager: ResMut<LspManager>) {
    get!(mut buffers, mut lsp_manager);

    let mut buf = buffers.cur_buffer_mut().await;

    if buf.byte_changes.is_empty() {
        return;
    }

    let Some(mut file) = buf.get_state_mut::<OpenedFile>().await else {
        // File hasn't been opened yet anyways
        return;
    };

    let client = lsp_manager.get_or_create_client(&file.lang).await.unwrap();

    file.change_id += 1;

    // Send full document content to ensure correct synchronization state
    // and avoid issues with incremental updates where intermediate text is unavailable.
    let changes = vec![TextDocumentContentChangeEvent {
        range: None,
        range_length: None,
        text: buf.rope.to_string(),
    }];

    let change = DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier {
            uri: Uri::file_path(buf.path.as_str()).unwrap(),
            version: file.change_id,
        },

        content_changes: changes,
    };

    client
        .notification("textDocument/didChange", change)
        .await
        .unwrap();
}
