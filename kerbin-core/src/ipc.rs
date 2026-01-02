use crate::*;
use ipmpsc::{Receiver, Sender, SharedRingBuffer};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, pin::Pin, sync::Arc, time::Duration};
use uuid::Uuid;

pub mod default_queries;
pub use default_queries::*;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientMessage {
    Command { id: Uuid, command: String },
    Query { id: Uuid, query: String },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerMessage {
    Response { id: Uuid, result: String },
    Error { id: Uuid, message: String },
}

fn get_queue_paths(session_id: &str) -> (String, String) {
    let temp_dir = dirs::data_dir().unwrap();
    let queue_dir = format!("{}/kerbin/sessions", temp_dir.to_string_lossy());
    let in_file = format!("{}/{}.in", queue_dir, session_id);
    let out_file = format!("{}/{}.out", queue_dir, session_id);
    (in_file, out_file)
}

#[derive(State)]
pub struct ServerIpc {
    in_queue: Receiver,
    out_queue: Sender,
    in_file: String,
    out_file: String,
}

impl ServerIpc {
    pub fn new(session_id: &str) -> Self {
        let (in_file, out_file) = get_queue_paths(session_id);

        // Ensure directory exists
        let temp_dir = dirs::data_dir().unwrap();
        let queue_dir = format!("{}/kerbin/sessions", temp_dir.to_string_lossy());
        let _ = std::fs::create_dir_all(queue_dir);

        // Create queues
        let in_queue = Receiver::new(SharedRingBuffer::create(&in_file, 16000).unwrap());
        let out_queue = Sender::new(SharedRingBuffer::create(&out_file, 16000).unwrap());

        Self {
            in_queue,
            out_queue,
            in_file,
            out_file,
        }
    }

    pub fn try_recv(&self) -> Option<ClientMessage> {
        self.in_queue.try_recv().ok().flatten()
    }

    pub fn send_response(&self, id: Uuid, result: String) {
        let _ = self.out_queue.send(&ServerMessage::Response { id, result });
    }

    pub fn send_error(&self, id: Uuid, message: String) {
        let _ = self.out_queue.send(&ServerMessage::Error { id, message });
    }
}

impl Drop for ServerIpc {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.in_file);
        let _ = std::fs::remove_file(&self.out_file);
    }
}

pub struct ClientIpc;

impl ClientIpc {
    pub fn send_command(session: &str, command: String) -> Result<(), String> {
        let (in_file, _) = get_queue_paths(session);

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

    pub fn query(session: &str, query: String) -> Result<String, String> {
        let (in_file, out_file) = get_queue_paths(session);

        let in_ring = SharedRingBuffer::open(&in_file)
            .map_err(|_| format!("Session '{}' not found.", session))?;
        let out_ring = SharedRingBuffer::open(&out_file)
            .map_err(|_| format!("Session '{}' output queue not found.", session))?;

        let in_queue = Sender::new(in_ring);
        let out_queue = Receiver::new(out_ring);

        let req_id = Uuid::new_v4();
        let msg = ClientMessage::Query { id: req_id, query };

        in_queue
            .send(&msg)
            .map_err(|e| format!("Error sending query: {:?}", e))?;

        // Wait for response
        let timeout = Duration::from_secs(5);
        let start = std::time::Instant::now();

        loop {
            if start.elapsed() > timeout {
                return Err("Query timed out.".to_string());
            }

            match out_queue.try_recv::<ServerMessage>() {
                Ok(Some(ServerMessage::Response { id, result })) if id == req_id => {
                    return Ok(result);
                }
                Ok(Some(ServerMessage::Error { id, message })) if id == req_id => {
                    return Err(format!("Query Error: {}", message));
                }
                Ok(Some(_)) => {
                    // Ignore messages for other clients
                    std::thread::sleep(Duration::from_millis(10));
                }
                Ok(None) => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => {
                    return Err(format!("Error receiving response: {:?}", e));
                }
            }
        }
    }
}

pub type QueryHandler = Arc<
    dyn for<'a> Fn(&'a mut State) -> Pin<Box<dyn Future<Output = String> + Send + 'a>>
        + Send
        + Sync,
>;

#[derive(State, Default)]
pub struct QueryRegistry {
    handlers: HashMap<String, QueryHandler>,
}

impl QueryRegistry {
    pub fn register<F>(&mut self, name: impl ToString, handler: F)
    where
        F: for<'a> Fn(&'a mut State) -> Pin<Box<dyn Future<Output = String> + Send + 'a>>
            + Send
            + Sync
            + 'static,
    {
        self.handlers.insert(name.to_string(), Arc::new(handler));
    }

    pub fn handler(&self, query: &str) -> Result<QueryHandler, String> {
        if let Some(handler) = self.handlers.get(query) {
            Ok(handler.clone())
        } else {
            Err(format!("Unknown query: {}", query))
        }
    }
}

#[allow(unused_assignments)]
pub async fn handle_ipc_messages(state: &mut State) {
    let log = state.lock_state::<LogSender>().await.clone();

    let mut server_ipc = Some(state.lock_state::<ServerIpc>().await);
    while let Some(msg) = server_ipc.as_ref().and_then(|x| x.try_recv()) {
        match msg {
            ClientMessage::Command { id: _, command } => {
                let commands = state.lock_state::<CommandRegistry>().await;
                let command_sender = state.lock_state::<CommandSender>().await;
                let prefix_registry = state.lock_state::<CommandPrefixRegistry>().await;
                let modes = state.lock_state::<ModeStack>().await;

                let words = word_split(&command);
                if let Some(cmd) = commands.parse_command(
                    words,
                    true,
                    false,
                    Some(&resolver_engine().await.as_resolver()),
                    true,
                    &prefix_registry,
                    &modes,
                )
                    && let Err(e) = command_sender.send(cmd) {
                        log.medium("IPC", format!("Failed to send command: {:?}", e));
                    }
            }
            ClientMessage::Query { id, query } => {
                let registry = state.lock_state::<QueryRegistry>().await;

                match registry.handler(&query) {
                    Ok(handler) => {
                        drop(registry);

                        // It's assigned???
                        server_ipc = None;

                        let res = handler(state).await;

                        server_ipc = Some(state.lock_state::<ServerIpc>().await);

                        server_ipc
                            .as_ref()
                            .expect("Should be some here")
                            .send_response(id, res);
                    }
                    Err(err) => {
                        log.medium("IPC", format!("Query error: {}", err));
                        server_ipc
                            .as_ref()
                            .expect("Should be some here")
                            .send_error(id, err);
                    }
                }
            }
        }
    }
}
