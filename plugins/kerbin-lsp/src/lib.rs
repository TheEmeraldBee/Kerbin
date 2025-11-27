use kerbin_core::{LogSender, State};

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
            .system(process_lsp_events);
    }

    state
        .lock_state::<LogSender>()
        .await
        .low("kerbin-lsp", format!("Registered Language: `{}`", name));
}

pub async fn init(state: &mut State) {
    state
        .state(LspHandlerManager::default())
        .state(LspManager::default());

    // {
    //     let mut command_registry = state.lock_state::<CommandRegistry>().await;

    //     command_registry.register::<HoverCommand>();
    // }

    // Setup global state handlers
    {
        let mut handler_manager = state.lock_state::<crate::LspHandlerManager>().await;

        handler_manager.on_global_notify("$/progress", |state, msg| Box::pin(log_init(state, msg)));
    }
}
