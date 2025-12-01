use kerbin_core::{CloseEvent, CommandRegistry, EVENT_BUS, LogSender, SaveEvent, State};

pub mod jsonrpc;
pub use jsonrpc::*;

pub mod client;
pub use client::*;

pub mod uriext;
pub use uriext::*;

pub mod handlers;
pub use handlers::*;

pub mod events;
pub use events::*;

pub mod manager;
pub use manager::*;

mod safe_ops_test;

pub mod diagnostics;
pub use diagnostics::*;

pub mod hover;
pub use hover::*;

pub mod autocomplete;
pub use autocomplete::*;

// Re-Exports
pub use lsp_types::*;

/// Register a language with its LSP server
pub async fn register_lang(
    state: &mut State,
    name: impl ToString,
    extensions: impl IntoIterator<Item = impl ToString>,
    info: LangInfo,
) {
    let name = name.to_string();
    let exts: Vec<String> = extensions.into_iter().map(|e| e.to_string()).collect();

    // Register with the manager
    let mut manager = state.lock_state::<LspManager>().await;
    manager.register_language(&name, exts.clone(), info);
    drop(manager);

    // Register systems for each extension
    for ext in exts {
        state
            .on_hook(kerbin_core::hooks::UpdateFiletype::new(ext))
            .system(open_files)
            .system(apply_changes)
            .system(render_diagnostic_highlights)
            .system(process_lsp_events)
            .system(render_hover)
            .system(update_completions)
            .system(render_completions);
    }
}

pub async fn init(state: &mut State) {
    state
        .state(LspHandlerManager::default())
        .state(LspManager::default());

    // Setup reaction to file save event
    EVENT_BUS
        .subscribe::<SaveEvent>()
        .await
        .system(file_save::file_saved);

    EVENT_BUS
        .subscribe::<CloseEvent>()
        .await
        .system(file_close::file_close);

    {
        let mut command_registry = state.lock_state::<CommandRegistry>().await;

        command_registry.register::<HoverCommand>();
        command_registry.register::<CompletionCommand>();
    }

    // Setup global state handlers
    {
        let mut handler_manager = state.lock_state::<LspHandlerManager>().await;

        handler_manager.on_global_notify("textDocument/publishDiagnostics", |state, msg| {
            Box::pin(publish_diagnostics(state, msg))
        });

        handler_manager.on_global_notify("$/progress", |state, msg| Box::pin(log_init(state, msg)));

        handler_manager.on_global_response("textDocument/hover", |state, msg| {
            Box::pin(handle_hover(state, msg))
        });

        handler_manager.on_global_response("textDocument/completion", |state, msg| {
            Box::pin(handle_completion(state, msg))
        });
    }
}
