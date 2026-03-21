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

pub async fn init(state: &mut State) {
    state.state(GrammarManager::default());

    let mut commands = state.lock_state::<CommandRegistry>().await;
    commands.register::<TreeSitterCommand>();
    commands.register::<InstallCommand>();
    commands.register::<ScopeInfoCommand>();
    commands.register::<TreeSitterMotion>();
    drop(commands);

    state
        .lock_state::<CommandInterceptorRegistry>()
        .await
        .on_command::<BufferCommand>(|cmd, state| {
            Box::pin(crate::indent::newline_intercept(cmd, state))
        });
}
