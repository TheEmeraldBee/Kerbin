use crate::jsonrpc::*;

use kerbin_core::{HookInfo, HookPathComponent};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};
use tokio::sync::Mutex;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

pub type EventHandler<State> = Box<dyn FnMut(&mut State, &JsonRpcMessage) + Send>;

pub struct RequestInfo {
    pub id: i32,
    pub method: String,
    pub params: Value,
}

struct HandlerEntry<State> {
    hook_info: HookInfo,
    handler: EventHandler<State>,
}

pub struct LspClient<W: AsyncWrite + Unpin + Send + 'static, State> {
    writer: Arc<Mutex<W>>,
    request_id: Arc<Mutex<i32>>,

    unproccessed_responses: Vec<JsonRpcResponse>,
    unprocessed_notifications: Vec<JsonRpcNotification>,

    /// Map of request IDs to their original request info
    request_info: std::collections::HashMap<i32, RequestInfo>,

    /// A list of response ids to ignore (Not Propogate into unproccessed_responses)
    ignore_ids: Vec<i32>,

    message_rx: UnboundedReceiver<JsonRpcMessage>,

    /// Event handlers for different message types
    response_handlers: Vec<HandlerEntry<State>>,
    notification_handlers: Vec<HandlerEntry<State>>,
    server_request_handlers: Vec<HandlerEntry<State>>,
}

impl<State> LspClient<ChildStdin, State> {
    pub async fn spawned(server_cmd: &str, args: Vec<&str>) -> std::io::Result<Self> {
        let mut process = Command::new(server_cmd)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let stdin = Arc::new(Mutex::new(process.stdin.take().unwrap()));
        let stdout = process.stdout.take().unwrap();

        Self::new(stdin, stdout)
    }
}

impl<W: AsyncWrite + Unpin + Send + 'static, State> LspClient<W, State> {
    pub fn new(
        input: Arc<Mutex<W>>,
        output: impl AsyncRead + Unpin + Send + 'static,
    ) -> std::io::Result<Self> {
        let (message_tx, message_rx) = unbounded_channel();
        let writer = input.clone();

        // Spawn task to read messages
        tokio::spawn(async move {
            Self::read_messages(output, message_tx, writer).await;
        });

        Ok(LspClient {
            writer: input,
            request_id: Arc::new(Mutex::new(0)),
            unproccessed_responses: vec![],
            unprocessed_notifications: vec![],
            request_info: std::collections::HashMap::new(),
            ignore_ids: vec![],
            message_rx,
            response_handlers: vec![],
            notification_handlers: vec![],
            server_request_handlers: vec![],
        })
    }

    async fn read_messages(
        stdout: impl AsyncRead + std::marker::Unpin,
        tx: UnboundedSender<JsonRpcMessage>,
        writer: Arc<Mutex<W>>,
    ) {
        let mut reader = BufReader::new(stdout);

        loop {
            // Read headers
            let mut content_length = 0;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).await.unwrap_or(0) == 0 {
                    return; // EOF
                }

                let header = header.trim();
                if header.is_empty() {
                    break; // End of headers
                }

                if let Some(value) = header.strip_prefix("Content-Length: ") {
                    content_length = value.parse().unwrap_or(0);
                }
            }

            if content_length == 0 {
                continue;
            }

            // Read content
            let mut content = vec![0u8; content_length];
            if reader.read_exact(&mut content).await.is_err() {
                return;
            }

            // Parse message
            if let Ok(value) = serde_json::from_slice::<Value>(&content) {
                let message = Self::parse_message(value, &writer).await;
                if let Some(msg) = message {
                    let _ = tx.send(msg);
                }
            }
        }
    }

    async fn parse_message(value: Value, writer: &Arc<Mutex<W>>) -> Option<JsonRpcMessage> {
        // Check if it's a response (has id but no method)
        if let Some(_) = value.get("id") {
            if value.get("method").is_none() {
                // It's a response
                if let Ok(response) = serde_json::from_value::<JsonRpcResponse>(value) {
                    return Some(JsonRpcMessage::Response(response));
                }
            } else {
                // It's a server request
                if let Ok(request) = serde_json::from_value::<JsonRpcServerRequest>(value.clone()) {
                    // Auto-respond to certain requests
                    Self::handle_server_request(&request, writer).await;
                    return Some(JsonRpcMessage::ServerRequest(request));
                }
            }
        } else if value.get("method").is_some() {
            // It's a notification
            if let Ok(notification) = serde_json::from_value::<JsonRpcNotification>(value) {
                return Some(JsonRpcMessage::Notification(notification));
            }
        }
        None
    }

    async fn handle_server_request(request: &JsonRpcServerRequest, writer: &Arc<Mutex<W>>) {
        // Auto-respond to workDoneProgress/create
        if request.method == "window/workDoneProgress/create" {
            let response = JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(serde_json::json!(null)),
                error: None,
            };

            if let Ok(message) = serde_json::to_string(&response) {
                let mut w = writer.lock().await;
                let _ = w
                    .write_all(
                        format!("Content-Length: {}\r\n\r\n{}", message.len(), message).as_bytes(),
                    )
                    .await;
                let _ = w.flush().await;
            }
        }
    }

    async fn get_next_id(&self) -> i32 {
        let mut id = self.request_id.lock().await;
        *id += 1;
        *id
    }

    async fn write_message(&self, message: &str) -> std::io::Result<()> {
        let mut writer = self.writer.lock().await;
        writer
            .write_all(format!("Content-Length: {}\r\n\r\n{}", message.len(), message).as_bytes())
            .await?;
        writer.flush().await?;
        Ok(())
    }

    pub async fn request<T: Serialize>(
        &mut self,
        method: impl ToString,
        params: T,
    ) -> std::io::Result<i32> {
        let id = self.get_next_id().await;
        let method_str = method.to_string();
        let params_value = serde_json::to_value(params)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: method_str.clone(),
            params: params_value.clone(),
        };

        // Store request info
        self.request_info.insert(
            id,
            RequestInfo {
                id,
                method: method_str,
                params: params_value,
            },
        );

        let message = serde_json::to_string(&request)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        self.write_message(&message).await?;
        Ok(id)
    }

    pub async fn notification<T: Serialize>(
        &self,
        method: impl ToString,
        params: T,
    ) -> std::io::Result<()> {
        let notification = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params: serde_json::to_value(params)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
        };

        let message = serde_json::to_string(&notification)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        self.write_message(&message).await?;
        Ok(())
    }

    /// Add a handler for responses matching a specific pattern
    /// Patterns use :: as separator and support:
    /// - Exact match: "textDocument/didOpen"
    /// - Wildcard: "*" or "textDocument::*"
    /// - OneOf: "textDocument|workspace::didOpen"
    pub fn on_response<F>(&mut self, pattern: &str, handler: F)
    where
        F: FnMut(&mut State, &JsonRpcMessage) + Send + 'static,
    {
        self.response_handlers.push(HandlerEntry {
            hook_info: HookInfo::new_custom_split(pattern, "/"),
            handler: Box::new(handler),
        });
    }

    /// Add a handler for notifications matching a specific pattern
    /// Patterns use :: as separator and support:
    /// - Exact match: "$/progress"
    /// - Wildcard: "*" matches everything
    /// - Multiple methods: "$/progress/begin|report|end"
    pub fn on_notification<F>(&mut self, pattern: &str, handler: F)
    where
        F: FnMut(&mut State, &JsonRpcMessage) + Send + 'static,
    {
        self.notification_handlers.push(HandlerEntry {
            hook_info: HookInfo::new_custom_split(pattern, "/"),
            handler: Box::new(handler),
        });
    }

    /// Add a handler for server requests matching a specific pattern
    pub fn on_server_request<F>(&mut self, pattern: &str, handler: F)
    where
        F: FnMut(&mut State, &JsonRpcMessage) + Send + 'static,
    {
        self.server_request_handlers.push(HandlerEntry {
            hook_info: HookInfo::new_custom_split(pattern, "/"),
            handler: Box::new(handler),
        });
    }

    fn call_matching_handlers(
        handlers: &mut [HandlerEntry<State>],
        method: &str,
        state: &mut State,
        msg: &JsonRpcMessage,
    ) {
        // Parse the method path
        let method_path = HookPathComponent::parse_custom_split(method, "/");

        // Find all matching handlers with their ranks
        let mut matches: Vec<(usize, i8)> = handlers
            .iter()
            .enumerate()
            .filter_map(|(idx, entry)| {
                entry
                    .hook_info
                    .matches(&method_path)
                    .map(|rank| (idx, rank))
            })
            .collect();

        // Sort by rank (highest first)
        matches.sort_by(|a, b| b.1.cmp(&a.1));

        // Call handlers in order of rank
        for (idx, _) in matches {
            (handlers[idx].handler)(state, msg);
        }
    }

    /// Process events with the given state, calling all registered handlers
    pub fn process_events(&mut self, state: &mut State) {
        while let Ok(msg) = self.message_rx.try_recv() {
            match &msg {
                JsonRpcMessage::Response(val) => {
                    if self.ignore_ids.contains(&val.id) {
                        self.ignore_ids.retain(|x| *x != val.id);
                        continue;
                    }

                    // Get the method from request info for pattern matching
                    if let Some(req_info) = self.request_info.get(&val.id) {
                        Self::call_matching_handlers(
                            &mut self.response_handlers,
                            &req_info.method,
                            state,
                            &msg,
                        );
                    }

                    self.unproccessed_responses.push(val.clone());
                }
                JsonRpcMessage::Notification(notif) => {
                    Self::call_matching_handlers(
                        &mut self.notification_handlers,
                        &notif.method,
                        state,
                        &msg,
                    );

                    self.unprocessed_notifications.push(notif.clone());
                }
                JsonRpcMessage::ServerRequest(req) => {
                    Self::call_matching_handlers(
                        &mut self.server_request_handlers,
                        &req.method,
                        state,
                        &msg,
                    );
                }
            }
        }
    }

    /// Propogates results of code into the state, handling the ids for later
    /// (Kept for backwards compatibility, but process_events is preferred)
    pub fn update_responses(&mut self) {
        while let Ok(msg) = self.message_rx.try_recv() {
            match msg {
                JsonRpcMessage::Response(val) => {
                    if self.ignore_ids.contains(&val.id) {
                        self.ignore_ids.retain(|x| *x != val.id);
                        continue;
                    }
                    self.unproccessed_responses.push(val);
                }
                JsonRpcMessage::Notification(notif) => {
                    self.unprocessed_notifications.push(notif);
                }
                JsonRpcMessage::ServerRequest(_) => {
                    // Already handled in read_messages
                }
            }
        }
    }

    /// Get the original request info for a given request ID
    pub fn get_request_info(&self, id: i32) -> Option<&RequestInfo> {
        self.request_info.get(&id)
    }

    /// Retrieves a response from the server by a given id
    pub fn response<T: DeserializeOwned>(&mut self, id: i32) -> Option<Result<T, Value>> {
        let idx = self
            .unproccessed_responses
            .iter()
            .enumerate()
            .find(|(_, x)| x.id == id)?
            .0;

        let res = self.unproccessed_responses.remove(idx);

        return match (res.result, res.error) {
            (_, Some(x)) => Some(Err(x)),
            (Some(x), None) => Some(Ok(serde_json::from_value(x).unwrap())),
            (None, None) => None,
        };
    }

    /// Get a notification by method name
    pub fn notification_by_method(&mut self, method: &str) -> Option<JsonRpcNotification> {
        let idx = self
            .unprocessed_notifications
            .iter()
            .enumerate()
            .find(|(_, x)| x.method == method)?
            .0;

        Some(self.unprocessed_notifications.remove(idx))
    }

    /// Get any notification
    pub fn any_notification(&mut self) -> Option<JsonRpcNotification> {
        if self.unprocessed_notifications.is_empty() {
            None
        } else {
            Some(self.unprocessed_notifications.remove(0))
        }
    }

    /// Returns the number of unprocessed responses there are
    pub fn unprocessed_count(&self) -> usize {
        self.unproccessed_responses.len()
    }

    /// Clears the remaining unprocessed responses, returning them.
    pub fn clear_unprocessed(&mut self) -> Vec<JsonRpcResponse> {
        let mut res = vec![];
        std::mem::swap(&mut res, &mut self.unproccessed_responses);
        res
    }

    /// Tell the system to ignore a response id. Should be used if the result can be ignored
    pub fn ignore_id(&mut self, id: i32) {
        self.ignore_ids.push(id);
    }

    /// Retrieves a response from the server, blocking until one is available if none are available.
    /// This will cause response(id) to never succeed if an id is set, so be careful
    pub async fn response_any(&mut self) -> Option<JsonRpcResponse> {
        if let Some(val) = self.unproccessed_responses.pop() {
            return Some(val);
        }

        loop {
            if let Some(msg) = self.message_rx.recv().await {
                match msg {
                    JsonRpcMessage::Response(resp) => return Some(resp),
                    JsonRpcMessage::Notification(notif) => {
                        self.unprocessed_notifications.push(notif);
                    }
                    JsonRpcMessage::ServerRequest(_) => {
                        // Already handled
                    }
                }
            } else {
                return None;
            }
        }
    }
}
