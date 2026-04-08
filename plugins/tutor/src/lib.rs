use kerbin_core::*;

pub mod load;

#[derive(Command)]
pub enum TutorCommands {
    /// Create a tutor buffer
    #[command]
    Tutor,
}

#[async_trait::async_trait]
impl Command<State> for TutorCommands {
    async fn apply(&self, state: &mut State) -> bool {
        state.call(load::open_default_buffer).await;

        false
    }
}

define_plugin! {
    name: "tutor",

    commands: [
        TutorCommands,
    ],

    hooks: [
        hooks::UpdateFiletype::new("tutor") => load::update_buffer,
    ],
}
