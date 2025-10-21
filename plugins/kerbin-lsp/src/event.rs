use crate::*;
use kerbin_core::{kerbin_macros::State, *};
use lsp_types::*;
use serde_json::json;
use std::collections::HashMap;

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

/// Tracks which files have been opened with LSP
#[derive(Default, State)]
pub struct OpenedFiles {
    pub opened: HashMap<String, OpenedFile>,
}

/// Command to process LSP events for all active clients
pub struct ProcessLspEventsCommand;

#[async_trait::async_trait]
impl kerbin_core::Command for ProcessLspEventsCommand {
    async fn apply(&self, state: &mut State) -> bool {
        let mut lsp_manager = state.lock_state::<LspManager>().await.unwrap();
        let handler_manager = state.lock_state::<LspHandlerManager>().await.unwrap();

        // Process events for all active clients
        for (_lang, client) in lsp_manager.client_map.iter_mut() {
            client.process_events(&handler_manager, state).await;
        }

        true
    }
}

pub async fn apply_changes(
    buffers: ResMut<Buffers>,
    opened_files: ResMut<OpenedFiles>,
    lsp_manager: ResMut<LspManager>,
) {
    get!(mut buffers, mut opened_files, mut lsp_manager);

    let buf = buffers.cur_buffer().await;
    let file_path = buf.path.clone();

    let Some(file) = opened_files.opened.get_mut(&file_path) else {
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

/// System that automatically opens files in LSP when they're accessed
pub async fn open_files(
    buffers: ResMut<Buffers>,
    opened_files: ResMut<OpenedFiles>,
    lsp_manager: ResMut<LspManager>,
) {
    get!(mut buffers, mut opened_files, mut lsp_manager);

    // Get current buffer info
    let current_buffer = buffers.cur_buffer().await;
    let file_path = current_buffer.path.clone();
    let ext = current_buffer.ext.clone();
    drop(buffers);

    // Check if we've already opened this file
    if opened_files.opened.contains_key(&file_path) {
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

    if client.init(root_uri).await.is_ok() {
        // Send initialized notification
        let _ = client.notification("initialized", json!({})).await;

        // Open the file
        if client.open(&file_path).await.is_ok() {
            opened_files.opened.insert(
                file_path.clone(),
                OpenedFile::new(lang, Uri::file_path(&file_path).unwrap()),
            );
        }
    }
}

/// System that processes LSP events each frame for any opened files
pub async fn process_lsp_events(
    opened_files: Res<OpenedFiles>,
    command_sender: Res<CommandSender>,
) {
    get!(opened_files, command_sender);

    // Only process events if we have opened files
    if !opened_files.opened.is_empty() {
        let _ = command_sender.send(Box::new(ProcessLspEventsCommand));
    }
}

pub async fn log_init(state: &State, msg: &JsonRpcMessage) {
    let log = state.lock_state::<LogSender>().await.unwrap();
    if let crate::JsonRpcMessage::Notification(notif) = msg
        && let Some(value) = notif.params.get("value")
        && let Some(kind) = value.get("kind").and_then(|k| k.as_str())
    {
        match kind {
            "begin" => {
                if let Some(title) = value.get("title").and_then(|t| t.as_str()) {
                    log.medium("lsp::client", format!("[Progress] {}", title));
                }
            }
            "end" => {}
            _ => {}
        }
    }
}

/// Helper function to find workspace root based on root files
fn find_workspace_root(file_path: &str, lang_info: Option<&crate::LanguageInfo>) -> Option<Uri> {
    use std::path::Path;
    use std::str::FromStr;

    let lang_info = lang_info?;
    let path = Path::new(file_path);
    let mut current = path.parent()?;

    // Search upwards for root markers
    loop {
        for root_marker in &lang_info.roots {
            let marker_path = current.join(root_marker);
            if marker_path.exists() {
                return Uri::from_str(&format!("file://{}", current.display())).ok();
            }
        }

        current = current.parent()?;
    }
}
