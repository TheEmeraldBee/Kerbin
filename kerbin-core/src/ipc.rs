use crate::*;
use ipmpsc::{Receiver, Sender, SharedRingBuffer};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientMessage {
    Command { id: Uuid, command: String },
}

pub fn sessions_dir() -> String {
    let temp_dir = dirs::data_dir().unwrap();
    format!("{}/kerbin/sessions", temp_dir.to_string_lossy())
}

pub fn session_pid_path(session_id: &str) -> String {
    format!("{}/{}.pid", sessions_dir(), session_id)
}

pub fn session_name_path(session_id: &str) -> String {
    format!("{}/{}.name", sessions_dir(), session_id)
}

fn get_queue_path(session_id: &str) -> String {
    format!("{}/{}.in", sessions_dir(), session_id)
}

#[derive(State)]
pub struct ServerIpc {
    in_queue: Receiver,
    in_file: String,
    pid_file: String,
}

impl ServerIpc {
    pub fn new(session_id: &str) -> Self {
        let in_file = get_queue_path(session_id);
        let pid_file = session_pid_path(session_id);

        // Ensure directory exists
        let _ = std::fs::create_dir_all(sessions_dir());

        // Write PID
        let _ = std::fs::write(&pid_file, std::process::id().to_string());

        // Create queue
        let in_queue = Receiver::new(SharedRingBuffer::create(&in_file, 16000).unwrap());

        Self {
            in_queue,
            in_file,
            pid_file,
        }
    }

    pub fn try_recv(&self) -> Option<ClientMessage> {
        self.in_queue.try_recv().ok().flatten()
    }
}

impl Drop for ServerIpc {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.in_file);
        let _ = std::fs::remove_file(&self.pid_file);
    }
}

pub struct ClientIpc;

impl ClientIpc {
    pub fn send_command(session: &str, command: String) -> Result<(), String> {
        let in_file = get_queue_path(session);

        let ring = SharedRingBuffer::open(&in_file)
            .map_err(|_| format!("Session '{}' not found.", session))?;

        let in_queue = Sender::new(ring);

        let msg = ClientMessage::Command {
            id: Uuid::new_v4(),
            command,
        };

        in_queue
            .send(&msg)
            .map_err(|e| format!("Error sending command: {:?}", e))
    }
}

pub async fn handle_ipc_messages(state: &mut State) {
    let log = state.lock_state::<LogSender>().await.clone();

    let server_ipc = state.lock_state::<ServerIpc>().await;
    let messages: Vec<ClientMessage> = std::iter::from_fn(|| server_ipc.try_recv()).collect();
    drop(server_ipc);

    for msg in messages {
        match msg {
            ClientMessage::Command { id: _, command } => {
                let commands = state.lock_state::<CommandRegistry>().await;
                let command_sender = state.lock_state::<CommandSender>().await;
                let prefix_registry = state.lock_state::<CommandPrefixRegistry>().await;
                let modes = state.lock_state::<ModeStack>().await;

                if let Some(cmd) = commands.parse_command(
                    tokenize(&command).unwrap_or_default(),
                    true,
                    false,
                    Some(&resolver_engine().await.as_resolver()),
                    true,
                    &prefix_registry,
                    &modes,
                ) && let Err(e) = command_sender.send(cmd)
                {
                    log.medium("IPC", format!("Failed to send command: {:?}", e));
                }
            }
        }
    }
}
