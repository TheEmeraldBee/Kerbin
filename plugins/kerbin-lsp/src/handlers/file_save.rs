use kerbin_core::*;

use crate::*;

pub async fn file_saved(
    buffers: ResMut<Buffers>,
    lsp_manager: ResMut<LspManager>,
    data: EventData<SaveEvent>,
) {
    get!(mut buffers, Some(data), mut lsp_manager);

    let Some(mut cur_buf_guard) = buffers.get_mut_path(&data.path).await else {
        return;
    };

    let Some(cur_buf) = cur_buf_guard.downcast_mut::<TextBuffer>() else {
        return;
    };

    let Some(client_info) = cur_buf.get_state_mut::<OpenedFile>().await else {
        return;
    };

    let lang = client_info.lang.clone();
    let uri = Uri::file_path(&cur_buf.path).expect("File path should be fine");

    let client = lsp_manager
        .get_or_create_client(&lang)
        .await
        .unwrap();

    client
        .notification(
            "textDocument/didSave",
            DidSaveTextDocumentParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                text: None,
            },
        )
        .await
        .unwrap();

    let fmt_config = lsp_manager
        .lang_info_map
        .get(&lang)
        .and_then(|i| i.format.clone());

    if let Some(fmt) = fmt_config
        && fmt.format_on_save
    {
        match fmt.kind {
            FormatterKind::Lsp => {
                send_lsp_format_request(cur_buf, &mut lsp_manager, &lang, uri).await;
            }
            FormatterKind::External(cmd, args) => {
                send_external_format_request(cur_buf, &cmd, &args).await;
            }
        }
    }
}
