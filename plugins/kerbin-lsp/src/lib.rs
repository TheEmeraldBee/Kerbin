use kerbin_core::ascii_forge;
use kerbin_core::ascii_forge::window::Stylize;
use kerbin_core::{kerbin_macros::State, *};
use lsp_types::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

pub mod client;
pub use client::*;

#[derive(State)]
pub struct LspManager {
    /// Maps language names to LSP clients
    clients: HashMap<String, Arc<LspClient>>,

    /// Maps file paths to open documents
    documents: HashMap<String, Arc<Document>>,

    /// Current completion suggestions per buffer
    completions: HashMap<String, Vec<CompletionItem>>,

    /// Maps a file extension (e.g., "rs") to a language name ("rust").
    pub extension_map: HashMap<String, String>,

    /// Configuration for LSP servers
    config: LspConfig,
}

#[derive(Default)]
pub struct LspConfig {
    /// Maps language ID to (command, args)
    pub servers: HashMap<String, (String, Vec<String>)>,
}

impl Default for LspManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LspManager {
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
            documents: HashMap::new(),
            completions: HashMap::new(),
            extension_map: HashMap::new(),
            config: LspConfig::default(),
        }
    }

    pub fn register_server(
        &mut self,
        lang: impl ToString,
        exts: impl IntoIterator<Item = impl ToString>,
        command: impl ToString,
        args: impl IntoIterator<Item = String>,
    ) {
        for ext in exts.into_iter() {
            self.extension_map.insert(ext.to_string(), lang.to_string());
        }

        self.config.servers.insert(
            lang.to_string(),
            (
                command.to_string(),
                args.into_iter().map(|x| x.to_string()).collect(),
            ),
        );
    }

    pub async fn get_or_create_client(
        &mut self,
        lang: impl AsRef<str>,
        root_path: impl Into<PathBuf>,
    ) -> Result<Arc<LspClient>, LspError> {
        let lang = lang.as_ref();
        if let Some(client) = self.clients.get(lang) {
            return Ok(client.clone());
        }

        let (command, args) = self
            .config
            .servers
            .get(lang)
            .ok_or_else(|| LspError::ServerError(format!("No LSP server for {}", lang)))?;

        let client = LspClientBuilder::new(command)
            .args(args.clone())
            .root_path(root_path)
            .client_name("kerbin")
            .client_version(env!("CARGO_PKG_VERSION").to_string())
            .build()
            .await?;

        self.clients.insert(lang.to_string(), client.clone());
        Ok(client)
    }

    pub async fn open_document(
        &mut self,
        client: Arc<LspClient>,
        path: impl AsRef<std::path::Path>,
    ) -> Result<Arc<Document>, LspError> {
        let path_str = path.as_ref().to_string_lossy().to_string();

        if let Some(doc) = self.documents.get(&path_str) {
            return Ok(doc.clone());
        }

        let doc = client.open_document(path).await?;
        let doc_arc = Arc::new(doc);
        self.documents.insert(path_str, doc_arc.clone());
        Ok(doc_arc)
    }

    pub fn store_completions(&mut self, buffer_path: &str, items: Vec<CompletionItem>) {
        self.completions.insert(buffer_path.to_string(), items);
    }

    pub fn get_completions(&self, buffer_path: &str) -> Option<&Vec<CompletionItem>> {
        self.completions.get(buffer_path)
    }

    pub fn clear_completions(&mut self, buffer_path: &str) {
        self.completions.remove(buffer_path);
    }
}

/// Trigger completion request when user types
pub async fn trigger_lsp_completion(lsp_manager: ResMut<LspManager>, buffers: Res<Buffers>) {
    get!(mut lsp_manager, buffers);

    let buf = buffers.cur_buffer().await;

    // Get language for current buffer
    let lang = lsp_manager.extension_map.get(&buf.ext).cloned();

    let Some(lang) = lang else {
        return;
    };

    let cursor_byte = buf.primary_cursor().get_cursor_byte();
    let cursor_pos = buf.rope.byte_to_line_idx(cursor_byte, LineType::LF_CR);
    let line_start = buf.rope.line_to_byte_idx(cursor_pos, LineType::LF_CR);
    let col = cursor_byte.saturating_sub(line_start);
    let path = buf.path.clone();

    // Get or create LSP client
    let client = match lsp_manager.get_or_create_client(lang, &path).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to get LSP client: {}", e);
            return;
        }
    };

    // Open document if not already open
    let doc = match lsp_manager.open_document(client, &path).await {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("Failed to open document: {}", e);
            return;
        }
    };

    // Request completions
    match doc.completion(cursor_pos as u32, col as u32).await {
        Ok(Some(items)) => {
            lsp_manager.store_completions(&path, items);
        }
        Ok(None) => {
            lsp_manager.clear_completions(&path);
        }
        Err(e) => {
            tracing::warn!("Completion request failed: {}", e);
        }
    }
}

/// Render LSP completions as inline extmarks
pub async fn render_lsp_completions(
    lsp_manager: Res<LspManager>,
    buffers: ResMut<Buffers>,
    theme: Res<Theme>,
) {
    get!(lsp_manager, mut buffers, theme);

    let mut buf = buffers.cur_buffer_mut().await;

    // Clear previous completion extmarks
    buf.renderer.clear_extmark_ns("lsp::completion");

    let Some(completions) = lsp_manager.get_completions(&buf.path) else {
        return;
    };

    if completions.is_empty() {
        return;
    }

    // Get cursor position
    let cursor_byte = buf.primary_cursor().get_cursor_byte();

    // Take the first (best) completion
    if let Some(item) = completions.first() {
        // Determine what text to show
        let suggestion_text = match &item.text_edit {
            Some(CompletionTextEdit::Edit(edit)) => &edit.new_text,
            Some(CompletionTextEdit::InsertAndReplace(edit)) => &edit.new_text,
            None => item.insert_text.as_ref().unwrap_or(&item.label),
        };

        // Get the completion style from theme, fallback to a dim style
        let completion_style = theme.get("lsp.completion.inline").unwrap_or_else(|| {
            ascii_forge::prelude::ContentStyle::default().on(ascii_forge::prelude::Color::DarkGrey)
        });

        // Add virtual text at cursor position
        buf.renderer.add_extmark(
            "lsp::completion",
            cursor_byte,
            100, // High priority
            vec![ExtmarkDecoration::VirtText {
                text: suggestion_text.clone(),
                hl: Some(completion_style),
            }],
        );
    }
}

pub async fn init(state: &mut State) {
    state.state(LspManager::default());

    // Hook into the update cycle
    state
        .on_hook(hooks::PostUpdate)
        .system(trigger_lsp_completion)
        .system(render_lsp_completions);
}
