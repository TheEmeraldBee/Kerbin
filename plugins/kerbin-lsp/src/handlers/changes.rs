use crate::*;

pub async fn apply_changes(buffers: ResMut<Buffers>, lsp_manager: ResMut<LspManager>) {
    get!(mut buffers, mut lsp_manager);

    let Some(mut buf) = buffers.cur_text_buffer_mut().await else { return; };

    if buf.byte_changes.is_empty() {
        return;
    }

    let Some(mut file) = buf.get_state_mut::<OpenedFile>().await else {
        // File hasn't been opened yet anyways
        return;
    };

    let Some(client) = lsp_manager.get_or_create_client(&file.lang).await.ok().flatten() else { return; };

    file.change_id += 1;

    // Avoid race conditions from incremental updates by sending the full document
    let changes = vec![TextDocumentContentChangeEvent {
        range: None,
        range_length: None,
        text: buf.to_string(),
    }];

    let Some(uri) = Uri::file_path(buf.path.as_str()).ok() else { return; };

    let change = DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier {
            uri,
            version: file.change_id,
        },

        content_changes: changes,
    };

    let _ = client.notification("textDocument/didChange", change).await;
}
