use crate::*;
use kerbin_core::*;

pub async fn apply_changes(buffers: ResMut<Buffers>, lsp_manager: ResMut<LspManager>) {
    get!(mut buffers, mut lsp_manager);

    let buf = buffers.cur_buffer_mut().await;

    let Some(mut file) = buf.get_state::<OpenedFile>().await else {
        // File hasn't been opened yet anyways
        return;
    };

    let client = lsp_manager.get_or_create_client(&file.lang).await.unwrap();

    let mut changes = vec![];

    for change in &buf.byte_changes {
        changes.push(TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position::new(change[0].0.0 as u32, change[0].0.1 as u32),
                end: Position::new(change[2].0.0 as u32, change[2].0.1 as u32),
            }),
            range_length: None,
            text: buf.rope.slice(change[0].1..change[2].1).to_string(),
        })
    }

    if changes.is_empty() {
        // No changes to send
        return;
    }

    file.change_id += 1;

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
