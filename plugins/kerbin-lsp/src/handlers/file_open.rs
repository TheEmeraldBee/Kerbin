use kerbin_core::{kerbin_macros::State, *};

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

    // Get current buffer info
    let mut current_buffer = buffers.cur_buffer_mut().await;
    let file_path = current_buffer.path.clone();
    let ext = current_buffer.ext.clone();
    drop(buffers);

    // Check if we've already opened this file
    if current_buffer.flags.contains("lsp_opened") {
        return;
    }

    // Try to get the language from extension
    let lang = match lsp_manager.ext_map.get(&ext) {
        Some(lang) => lang.clone(),
        None => return, // No LSP for this extension
    };

    let lang_info = lsp_manager.lang_info_map.get(&lang).cloned();

    // Get or create the client
    let client = match lsp_manager.get_or_create_client(&lang).await {
        Some(client) => client,
        None => return,
    };

    // Initialize if this is a new client
    let root_uri = find_workspace_root(&file_path, lang_info.as_ref()).unwrap_or_else(|| {
        Uri::file_path(&std::env::current_dir().unwrap().to_string_lossy()).unwrap()
    });

    if !client.is_flag_set("init") && client.init(root_uri).await.is_ok() {
        // Send initialized notification
        let _ = client
            .notification("initialized", serde_json::json!({}))
            .await;

        // Set that we initialized the editor
        client.set_flag("init");
    }

    // Open the file
    if client.open(&file_path).await.is_ok() {
        current_buffer.flags.insert("lsp_opened");
        current_buffer.set_state(OpenedFile::new(lang, Uri::file_path(&file_path).unwrap()));
    }
}
