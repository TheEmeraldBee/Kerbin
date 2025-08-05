use std::path::PathBuf;

use crate::{AppState, Arc, EditorCommand};
use ipmpsc::SharedRingBuffer;

#[derive(rune::Any)]
pub struct ShellLink {
    pub session_id: String,
    pub receiver: ipmpsc::Receiver,
}

impl Default for ShellLink {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellLink {
    pub fn new() -> Self {
        let session_id = uuid::Uuid::new_v4().to_string();

        let path = format!(
            "{}/kerbin/sessions/{}",
            dirs::data_dir().unwrap().display(),
            session_id
        );

        let mut dir = PathBuf::from(&path);
        dir.pop();

        let _ = std::fs::create_dir_all(&dir);

        let receiver = ipmpsc::Receiver::new(SharedRingBuffer::create(&path, 32 * 1024).unwrap());

        Self {
            session_id,
            receiver,
        }
    }

    pub fn cleanup(&mut self) {
        let _ = std::fs::remove_file(format!(
            "{}/kerbin/sessions/{}",
            dirs::data_dir().unwrap().display(),
            self.session_id
        ));
    }

    #[rune::function(keep)]
    pub fn id(&self) -> String {
        self.session_id.clone()
    }

    #[rune::function(keep)]
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

pub fn catch_events(state: Arc<AppState>) {
    let link = state.shell.read().unwrap();
    match link.receiver.try_recv::<EditorCommand>() {
        Ok(t) => {
            if let Some(t) = t {
                state.commands.send(Box::new(t)).unwrap();
            }
        }
        Err(e) => {
            panic!("{e}")
        }
    }
}
