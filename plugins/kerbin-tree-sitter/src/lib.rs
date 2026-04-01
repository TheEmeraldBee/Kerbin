use kerbin_core::*;

use crate::{
    install_command::InstallCommand, motions::TreeSitterMotion, scope_info::ScopeInfoCommand,
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

async fn reset_config_state(grammar_manager: ResMut<GrammarManager>) {
    let mut manager = grammar_manager.get().await;
    manager.ext_map.clear();
    manager.lang_map.clear();
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
