use crate::{HandlerEntry, LspHandlerManager, jsonrpc::*};

use kerbin_core::{HookPathComponent, State};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};
use tokio::sync::Mutex;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

pub mod facade;
pub use facade::*;

pub struct RequestInfo {
    pub id: i32,
    pub method: String,
    pub params: Value,
}

pub struct LspClient<W: AsyncWrite + Unpin + Send + 'static> {
    flags: HashSet<&'static str>,

    lang_id: String,

    writer: Arc<Mutex<W>>,
    request_id: Arc<Mutex<i32>>,

    /// Map of request IDs to their original request info
    request_info: std::collections::HashMap<i32, RequestInfo>,

    /// Response IDs to drop without processing
    ignore_ids: Vec<i32>,

    message_rx: UnboundedReceiver<JsonRpcMessage>,

    /// Server capabilities received from the initialize response
    pub server_capabilities: Option<lsp_types::ServerCapabilities>,
}

impl LspClient<ChildStdin> {
    pub async fn spawned(
        lang: impl ToString,
        server_cmd: &str,
        args: Vec<String>,
    ) -> std::io::Result<Self> {
        let mut process = Command::new(server_cmd)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let stdin = Arc::new(Mutex::new(process.stdin.take().unwrap()));
        let stdout = process.stdout.take().unwrap();
        let stderr = process.stderr.take().unwrap();

        tokio::spawn(async move {
            Self::log_errors(stderr).await;
        });

        Self::new(lang.to_string(), stdin, stdout)
    }
}

impl<W: AsyncWrite + Unpin + Send + 'static> LspClient<W> {
    pub fn new(
        lang: String,
        input: Arc<Mutex<W>>,
        output: impl AsyncRead + Unpin + Send + 'static,
    ) -> std::io::Result<Self> {
        let (message_tx, message_rx) = unbounded_channel();
        let writer = input.clone();

        tokio::spawn(async move {
            Self::read_messages(output, message_tx, writer).await;
        });

        Ok(LspClient {
            flags: HashSet::default(),

            lang_id: lang,
            writer: input,
            request_id: Arc::new(Mutex::new(0)),
            request_info: std::collections::HashMap::new(),
            ignore_ids: vec![],
            message_rx,
            server_capabilities: None,
        })
    }

    pub fn set_flag(&mut self, flag: &'static str) {
        self.flags.insert(flag);
    }

    pub fn unset_flag(&mut self, flag: &'static str) {
        self.flags.remove(flag);
    }

    pub fn is_flag_set(&mut self, flag: &'static str) -> bool {
        self.flags.contains(flag)
    }

    async fn log_errors(stderr: impl AsyncRead + std::marker::Unpin) {
        let mut reader = BufReader::new(stderr);

        loop {
            let mut text = String::new();
            if reader.read_line(&mut text).await.unwrap_or(0) == 0 {
                return;
            }

            tracing::error!("LSP Error: {text}");
        }
    }

    async fn read_messages(
        stdout: impl AsyncRead + std::marker::Unpin,
        tx: UnboundedSender<JsonRpcMessage>,
        writer: Arc<Mutex<W>>,
    ) {
        let mut reader = BufReader::new(stdout);

        loop {
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

            let mut content = vec![0u8; content_length];
            if reader.read_exact(&mut content).await.is_err() {
                return;
            }

            if let Ok(value) = serde_json::from_slice::<Value>(&content) {
                let message = Self::parse_message(value, &writer).await;
                if let Some(msg) = message {
                    let _ = tx.send(msg);
                }
            }
        }
    }

    async fn parse_message(value: Value, writer: &Arc<Mutex<W>>) -> Option<JsonRpcMessage> {
        if value.get("id").is_some() {
            if value.get("method").is_none() {
                if let Ok(response) = serde_json::from_value::<JsonRpcResponse>(value) {
                    return Some(JsonRpcMessage::Response(response));
                }
            } else {
                if let Ok(request) = serde_json::from_value::<JsonRpcServerRequest>(value.clone()) {
                    // Auto-respond to certain requests
                    Self::handle_server_request(&request, writer).await;
                    return Some(JsonRpcMessage::ServerRequest(request));
                }
            }
        } else if value.get("method").is_some() {
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

    async fn call_matching_handlers<'a>(
        handlers: impl Iterator<Item = &'a HandlerEntry> + 'a,
        method: &str,
        state: &State,
        msg: &JsonRpcMessage,
    ) {
        let method_path = HookPathComponent::parse_custom_split(method, "/");

        let mut matches: Vec<(&'a HandlerEntry, i8)> = handlers
            .filter_map(|entry| {
                entry
                    .hook_info
                    .matches(&method_path)
                    .map(|rank| (entry, rank))
            })
            .collect();

        // Sort by rank (highest first)
        matches.sort_by(|a, b| b.1.cmp(&a.1));

        for (entry, _) in matches {
            (entry.handler)(state, msg).await;
        }
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

    /// Process events with the given state, calling all registered handlers
    pub async fn process_events(&mut self, handler_manager: &LspHandlerManager, state: &State) {
        while let Ok(msg) = self.message_rx.try_recv() {
            match &msg {
                JsonRpcMessage::Response(val) => {
                    if self.ignore_ids.contains(&val.id) {
                        self.ignore_ids.retain(|x| *x != val.id);
                        continue;
                    }

                    // Get the method from request info for pattern matching
                    if let Some(req_info) = self.request_info.get(&val.id) {
                        // Cache server capabilities from the initialize response
                        if req_info.method == "initialize"
                            && let Some(result) = &val.result
                                && let Ok(init_result) =
                                    serde_json::from_value::<lsp_types::InitializeResult>(
                                        result.clone(),
                                    )
                                {
                                    self.server_capabilities =
                                        Some(init_result.capabilities);
                                }

                        let handlers = handler_manager.iter_response_handlers(&self.lang_id);
                        Self::call_matching_handlers(handlers, &req_info.method, state, &msg).await;
                    }
                }
                JsonRpcMessage::Notification(notif) => {
                    let handlers = handler_manager.iter_notification_handlers(&self.lang_id);
                    Self::call_matching_handlers(handlers, &notif.method, state, &msg).await;
                }
                JsonRpcMessage::ServerRequest(req) => {
                    let handlers = handler_manager.iter_server_request_handlers(&self.lang_id);
                    Self::call_matching_handlers(handlers, &req.method, state, &msg).await;
                }
            }
        }
    }

    /// Get the original request info for a given request ID
    pub fn get_request_info(&self, id: i32) -> Option<&RequestInfo> {
        self.request_info.get(&id)
    }

    /// Tell the system to ignore a response id. Should be used if the result can be ignored
    pub fn ignore_id(&mut self, id: i32) {
        self.ignore_ids.push(id);
    }
}
