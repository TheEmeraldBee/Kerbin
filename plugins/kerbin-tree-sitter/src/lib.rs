use kerbin_core::*;

use crate::{grammar::GrammarDefinition, grammar_manager::GrammarManager};

pub mod grammar;
pub mod grammar_manager;

pub async fn init(state: &mut State) {
    // Load grammars
    let grammar_list = match state
        .lock_state::<PluginConfig>()
        .await
        .get::<Vec<GrammarDefinition>>("tree-sitter-grammars")
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

    let manager = GrammarManager::from_definitions(grammar_list);
    manager.register_extension_handlers(state).await;

    // Initialize grammar state
    state.state(manager);
}
