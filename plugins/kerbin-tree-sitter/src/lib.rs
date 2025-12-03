use kerbin_core::*;

use crate::{
    grammar::GrammarEntry, grammar_manager::GrammarManager, install_command::InstallCommand,
    scope_info::ScopeInfoCommand,
};

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

pub async fn init(state: &mut State) {
    // Load grammars
    let grammar_list = match state
        .lock_state::<PluginConfig>()
        .await
        .get::<Vec<GrammarEntry>>("tree-sitter-grammars")
    {
        Some(Ok(t)) => t,
        None => vec![],
        Some(Err(e)) => {
            state.lock_state::<LogSender>().await.critical(
                "tree-sitter::init",
                format!("Failed to load grammar list due to error: {e}"),
            );
            vec![]
        }
    };

    let manager = match GrammarManager::from_definitions(grammar_list) {
        Ok(t) => t,
        Err((g, e)) => {
            state.lock_state::<LogSender>().await.critical(
                "tree-sitter::init",
                format!("Failed to install grammar due to error: {e}"),
            );
            g
        }
    };

    {
        let mut commands = state.lock_state::<CommandRegistry>().await;

        // Register command to install all grammars
        commands.register::<InstallCommand>();

        commands.register::<ScopeInfoCommand>();

        commands.register::<crate::indent::IndentCommand>();
    }

    manager.register_extension_handlers(state).await;

    // Initialize grammar state
    state.state(manager);
}
