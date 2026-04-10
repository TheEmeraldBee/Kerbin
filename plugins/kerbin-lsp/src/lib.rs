use kerbin_core::*;

pub mod commands;
pub use commands::LspCommand;

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

pub mod diagnostics;
pub use diagnostics::*;

pub mod hover;
pub use hover::*;

pub mod autocomplete;
pub use autocomplete::*;

pub mod navigation;
pub use navigation::*;

pub mod text_edit;
pub use text_edit::apply_text_edits;

pub mod format;
pub use format::*;

pub use lsp_types::*;

async fn reset_config_state(lsp_manager: ResMut<LspManager>) {
    let mut manager = lsp_manager.get().await;
    manager.server_map.clear();
    manager.lang_to_server.clear();
}

define_plugin! {
    name: "kerbin-lsp",
    init_as: plugin_init,

    state: [
        LspHandlerManager,
        LspManager,
        GlobalDiagnostics,
    ],

    commands: [
        LspCommand,
        HoverCommand,
        CompletionCommand,
        NavigationCommand,
        FormatCommand,
    ],

    hooks: [
        hooks::ResetState => reset_config_state,
    ],

    events: [
        SaveEvent => file_save::file_saved,
        CloseEvent => file_close::file_close,
    ],
}

pub async fn init(state: &mut State) {
    plugin_init(state).await;

    let mut handler_manager = state.lock_state::<LspHandlerManager>().await;

    handler_manager.on_global_notify("textDocument/publishDiagnostics", |state, msg| {
        Box::pin(publish_diagnostics(state, msg))
    });
    handler_manager.on_global_notify("$/progress", |state, msg| {
        Box::pin(log_init(state, msg))
    });
    handler_manager.on_global_response("textDocument/hover", |state, msg| {
        Box::pin(handle_hover(state, msg))
    });
    handler_manager.on_global_response("textDocument/completion", |state, msg| {
        Box::pin(handle_completion(state, msg))
    });
    handler_manager.on_global_response("completionItem/resolve", |state, msg| {
        Box::pin(handle_completion_resolve(state, msg))
    });
    handler_manager.on_global_response("textDocument/definition", |state, msg| {
        Box::pin(handle_navigation(state, msg))
    });
    handler_manager.on_global_response("textDocument/references", |state, msg| {
        Box::pin(handle_navigation(state, msg))
    });
    handler_manager.on_global_response("textDocument/implementation", |state, msg| {
        Box::pin(handle_navigation(state, msg))
    });
    handler_manager.on_global_response("textDocument/typeDefinition", |state, msg| {
        Box::pin(handle_navigation(state, msg))
    });
    handler_manager.on_global_response("textDocument/declaration", |state, msg| {
        Box::pin(handle_navigation(state, msg))
    });
    handler_manager.on_global_response("textDocument/formatting", |state, msg| {
        Box::pin(handle_format(state, msg))
    });
}
