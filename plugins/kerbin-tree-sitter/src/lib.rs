use kerbin_core::*;

use crate::{
    install_command::InstallCommand,
    motions::TreeSitterMotion,
    scope_info::ScopeInfoCommand,
    state::TreeSitterState,
};

pub mod commands;
pub use commands::TreeSitterCommand;
pub use grammar_manager::GrammarManager;

pub mod grammar;
pub mod grammar_install;
pub mod grammar_manager;

pub mod state;

pub mod text_provider;

pub mod query_walker;

pub mod highlighter;

pub mod install_command;

pub mod scope_info;

pub mod indent;

pub mod motions;

pub mod locals;

async fn reset_config_state(grammar_manager: ResMut<GrammarManager>, buffers: ResMut<Buffers>) {
    let mut manager = grammar_manager.get().await;
    manager.grammar_map.clear();
    manager.loaded_grammars.clear();
    manager.query_map.clear();
    manager.failed_queries.clear();
    manager.lang_to_grammar.clear();
    drop(manager);

    let bufs = buffers.get().await;
    for arc in &bufs.buffers {
        let Ok(mut buf) = arc.clone().try_write_owned() else {
            continue;
        };
        if let Some(tb) = buf.as_any_mut().downcast_mut::<TextBuffer>() {
            tb.flags.remove("tree-sitter-checked");
            tb.remove_state::<TreeSitterState>();
        }
    }
}

define_plugin! {
    name: "kerbin-tree-sitter",
    init_as: plugin_init,

    state: [
        GrammarManager,
    ],

    commands: [
        TreeSitterCommand,
        InstallCommand,
        ScopeInfoCommand,
        TreeSitterMotion,
    ],

    hooks: [
        hooks::ResetState => reset_config_state,
    ],
}

pub async fn init(state: &mut State) {
    plugin_init(state).await;

    // Extra: newline interceptor can't be expressed in define_plugin!
    state
        .lock_state::<CommandInterceptorRegistry>()
        .await
        .on_command::<BufferCommand>(|cmd, state| {
            Box::pin(crate::indent::newline_intercept(cmd, state))
        });
}
