use kerbin_core::*;

use crate::grammar_manager::GrammarManager;

#[derive(Command)]
pub enum InstallCommand {
    /// Installs all non-installed grammars onto your system in parallel
    #[command]
    InstallAllGrammars,
}

#[async_trait::async_trait]
impl Command for InstallCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            Self::InstallAllGrammars => {
                let grammars = state.lock_state::<GrammarManager>().await;

                // Spawn threads to install grammars
                grammars.install_all_grammars(state).await;
            }
        }
        false
    }
}
