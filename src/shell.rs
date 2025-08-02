use crate::EditorCommand;
use stategine::prelude::*;

#[derive(rune::Any)]
pub struct ShellLink {
    pub session_id: String,
    pub receiver: ipmpsc::Receiver,
}

impl ShellLink {
    pub fn id(&self) -> String {
        self.session_id.clone()
    }

    pub fn spawn(&self, shell: String, command: String) {
        match std::process::Command::new(shell)
            .arg("-c")
            .arg(command)
            .env("KERBIN_SESSION", self.session_id.clone())
            .spawn()
        {
            Ok(_) => {}
            Err(e) => tracing::error!("{e}"),
        };
    }
}

pub fn catch_events(link: Res<ShellLink>, mut commands: ResMut<Commands>) {
    match link.receiver.try_recv::<EditorCommand>() {
        Ok(t) => {
            if let Some(t) = t {
                commands.add(t);
            }
        }
        Err(e) => {
            panic!("{e}")
        }
    }
}
