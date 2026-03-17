use kerbin_core::*;

pub mod load;

#[derive(Command)]
pub enum TutorCommands {
    /// Create a tutor buffer
    #[command]
    Tutor,
}

#[async_trait::async_trait]
impl Command for TutorCommands {
    async fn apply(&self, state: &mut State) -> bool {
        state.call(load::open_default_buffer).await;

        // These functions can never repeat
        false
    }
}

pub async fn init(state: &mut State) {
    {
        state
            .lock_state::<CommandRegistry>()
            .await
            .register::<TutorCommands>();
    }

    state
        .on_hook(hooks::UpdateFiletype::new("<tutor>"))
        .system(load::update_buffer);
}
