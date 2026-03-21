use crate::*;

#[derive(Clone, Debug, Command)]
pub enum AutoPairsCommand {
    #[command(name = "auto-pairs-add")]
    Add { open: char, close: char },

    #[command(name = "auto-pairs-remove")]
    Remove { open: char },
}

#[async_trait::async_trait]
impl Command for AutoPairsCommand {
    async fn apply(&self, state: &mut State) -> bool {
        let mut auto_pairs = state.lock_state::<AutoPairs>().await;
        match self {
            Self::Add { open, close } => {
                auto_pairs.add_pair(*open, *close);
                true
            }
            Self::Remove { open } => {
                auto_pairs.remove_pair(*open);
                true
            }
        }
    }
}
