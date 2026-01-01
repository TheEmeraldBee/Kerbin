use crate::*;
use ipmpsc::{Receiver, Sender, SharedRingBuffer};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

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
        let _ = self
            .out_queue
            .send(&ServerMessage::Response { id, result });
    }

    pub fn send_error(&self, id: Uuid, message: String) {
        let _ = self
            .out_queue
            .send(&ServerMessage::Error { id, message });
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
        let msg = ClientMessage::Query {
            id: req_id,
            query,
        };

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

pub async fn handle_ipc_messages(
    server_ipc: Res<ServerIpc>,
    commands: Res<CommandRegistry>,
    command_sender: Res<CommandSender>,
    prefix_registry: Res<CommandPrefixRegistry>,
    modes: Res<ModeStack>,
    bufs: Res<Buffers>,
    log: Res<LogSender>,
) {
    get!(
        server_ipc,
        commands,
        command_sender,
        prefix_registry,
        modes,
        bufs,
        log
    );

    while let Some(msg) = server_ipc.try_recv() {
        match msg {
            ClientMessage::Command { id: _, command } => {
                let words = word_split(&command);
                if let Some(cmd) = commands.parse_command(
                    words,
                    true,
                    false,
                    Some(&resolver_engine().await.as_resolver()),
                    true,
                    &prefix_registry,
                    &modes,
                ) {
                    if let Err(e) = command_sender.send(cmd) {
                        log.medium("IPC", format!("Failed to send command: {:?}", e));
                    }
                }
            }
            ClientMessage::Query { id, query } => {
                // Simple query handler
                let response = if query == "file_info" {
                    let buf = bufs.cur_buffer().await;
                    format!(
                        "{{ \"path\": \"{:?}\", \"dirty\": {} }}",
                        buf.path, buf.dirty
                    )
                } else {
                    let err_msg = format!("{{ \"error\": \"Unknown query: {}\" }}", query);
                    log.medium("IPC", format!("Received unknown query: {}", query));
                    err_msg
                };

                server_ipc.send_response(id, response);
            }
        }
    }
}
