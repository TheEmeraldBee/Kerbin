use kerbin_core::*;

use crate::*;

pub async fn file_saved(
    buffers: ResMut<Buffers>,
    lsp_manager: ResMut<LspManager>,
    data: EventData<SaveEvent>,
) {
    get!(mut buffers, Some(data), mut lsp_manager);

    let Some(mut cur_buf) = buffers.get_mut_path(&data.path).await else {
        return;
    };

    let Some(client_info) = cur_buf.get_state::<OpenedFile>().await else {
        return;
    };

    let client = lsp_manager
        .get_or_create_client(&client_info.lang)
        .await
        .unwrap();

    client
        .notification(
            "textDocument/didSave",
            DidSaveTextDocumentParams {
                text_document: TextDocumentIdentifier {
                    uri: Uri::file_path(&cur_buf.path).expect("File path should be fine"),
                },
                text: None,
            },
        )
        .await
        .unwrap();
}
