use kerbin_core::*;

use lsp_types::Uri;

use crate::*;

#[derive(State)]
pub struct OpenedFile {
    pub lang: String,
    pub uri: Uri,
    pub change_id: i32,
}

impl OpenedFile {
    pub fn new(lang: String, uri: Uri) -> Self {
        Self {
            lang,
            uri,
            change_id: 0,
        }
    }
}

/// System that automatically opens files in LSP when they're accessed
pub async fn open_files(buffers: ResMut<Buffers>, lsp_manager: ResMut<LspManager>) {
    get!(mut buffers, mut lsp_manager);

    let Some(mut current_buffer) = buffers.cur_text_buffer_mut().await else { return; };
    let file_path = current_buffer.path.clone();
    let ext = current_buffer.ext.clone();
    drop(buffers);

    if current_buffer.flags.contains("lsp_opened") {
        return;
    }

    let lang = match lsp_manager.ext_map.get(&ext) {
        Some(lang) => lang.clone(),
        None => return, // No LSP for this extension
    };

    let lang_info = lsp_manager.lang_info_map.get(&lang).cloned();

    let client = match lsp_manager.get_or_create_client(&lang).await {
        Some(client) => client,
        None => return,
    };

    let Some(root_uri) = find_workspace_root(&file_path, lang_info.as_ref()).or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|d| Uri::file_path(&d.to_string_lossy()).ok())
    }) else {
        return;
    };

    if !client.is_flag_set("init") && client.init(root_uri).await.is_ok() {
        let _ = client
            .notification("initialized", serde_json::json!({}))
            .await;

        client.set_flag("init");
    }

    if client.open(&file_path).await.is_ok() {
        let Some(uri) = Uri::file_path(&file_path).ok() else { return; };
        current_buffer.flags.insert("lsp_opened");
        current_buffer.set_state(OpenedFile::new(lang, uri));
    }
}
