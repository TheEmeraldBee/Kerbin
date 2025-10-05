use lsp_types::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::{Mutex, broadcast, oneshot};

type Result<T> = std::result::Result<T, LspError>;

#[derive(Debug, thiserror::Error)]
pub enum LspError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Server error: {0}")]
    ServerError(String),
    #[error("Request cancelled")]
    Cancelled,
    #[error("Invalid URI: {0}")]
    InvalidUri(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonRpcMessage {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
}

#[derive(Debug, Clone)]
pub enum ServerNotification {
    PublishDiagnostics(PublishDiagnosticsParams),
    ShowMessage(ShowMessageParams),
    LogMessage(LogMessageParams),
    Other { method: String, params: Value },
}

/// Builder for creating an LSP client
pub struct LspClientBuilder {
    command: String,
    args: Vec<String>,
    root_path: Option<PathBuf>,
    client_name: String,
    client_version: String,
    capabilities: ClientCapabilities,
}

impl LspClientBuilder {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            root_path: None,
            client_name: "lsp-client".to_string(),
            client_version: "0.0.0".to_string(),
            capabilities: Self::default_capabilities(),
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(|s| s.into()));
        self
    }

    pub fn root_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.root_path = Some(path.into());
        self
    }

    pub fn client_name(mut self, name: impl Into<String>) -> Self {
        self.client_name = name.into();
        self
    }

    pub fn client_version(mut self, ver: String) -> Self {
        self.client_version = ver;
        self
    }

    pub fn capabilities(mut self, capabilities: ClientCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    pub async fn build(self) -> Result<Arc<LspClient>> {
        LspClient::new_with_config(self).await
    }

    fn default_capabilities() -> ClientCapabilities {
        ClientCapabilities {
            text_document: Some(TextDocumentClientCapabilities {
                completion: Some(CompletionClientCapabilities {
                    completion_item: Some(CompletionItemCapability {
                        snippet_support: Some(true),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                hover: Some(HoverClientCapabilities {
                    content_format: Some(vec![MarkupKind::Markdown]),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            workspace: Some(WorkspaceClientCapabilities {
                apply_edit: Some(true),
                workspace_edit: Some(WorkspaceEditClientCapabilities {
                    document_changes: Some(true),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        }
    }
}

pub struct LspClient {
    writer: Arc<Mutex<ChildStdin>>,
    next_id: Arc<AtomicI64>,
    pending_requests: Arc<Mutex<HashMap<i64, oneshot::Sender<Value>>>>,
    server_capabilities: Arc<Mutex<Option<ServerCapabilities>>>,
    notification_tx: broadcast::Sender<ServerNotification>,
    _process: Child,
}

impl LspClient {
    async fn new_with_config(config: LspClientBuilder) -> Result<Arc<Self>> {
        let mut process = tokio::process::Command::new(&config.command)
            .args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdin = process.stdin.take().expect("Failed to get stdin");
        let stdout = process.stdout.take().expect("Failed to get stdout");

        let writer = Arc::new(Mutex::new(stdin));
        let next_id = Arc::new(AtomicI64::new(1));
        let pending_requests = Arc::new(Mutex::new(HashMap::new()));
        let server_capabilities = Arc::new(Mutex::new(None));
        let (notification_tx, _) = broadcast::channel(100);

        let mut client = LspClient {
            writer: writer.clone(),
            next_id: next_id.clone(),
            pending_requests: pending_requests.clone(),
            server_capabilities: server_capabilities.clone(),
            notification_tx: notification_tx.clone(),
            _process: process,
        };

        // Spawn message reader
        tokio::spawn(Self::read_messages(
            stdout,
            pending_requests.clone(),
            notification_tx,
        ));

        // Initialize
        client.initialize_with_config(config).await?;

        Ok(Arc::new(client))
    }

    async fn initialize_with_config(&mut self, config: LspClientBuilder) -> Result<()> {
        let root_uri = if let Some(root) = config.root_path {
            let absolute = root.canonicalize()?;
            Uri::from_str(&format!("file://{}", absolute.display()))
                .map_err(|_| LspError::InvalidUri(absolute.display().to_string()))?
        } else {
            let cwd = std::env::current_dir()?;
            Uri::from_str(&format!("file://{}", cwd.display()))
                .map_err(|_| LspError::InvalidUri(cwd.display().to_string()))?
        };

        let params = InitializeParams {
            process_id: Some(std::process::id()),
            capabilities: config.capabilities,
            trace: Some(TraceValue::Off),
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: root_uri.clone(),
                name: root_uri.path().to_string(),
            }]),
            client_info: Some(ClientInfo {
                name: config.client_name,
                version: Some(config.client_version),
            }),
            ..Default::default()
        };

        let result: InitializeResult = self.send_request("initialize", params).await?;
        *self.server_capabilities.lock().await = Some(result.capabilities.clone());

        self.send_notification("initialized", InitializedParams {})
            .await?;

        Ok(())
    }

    pub fn subscribe_notifications(&self) -> broadcast::Receiver<ServerNotification> {
        self.notification_tx.subscribe()
    }

    pub async fn server_capabilities(&self) -> Option<ServerCapabilities> {
        self.server_capabilities.lock().await.clone()
    }

    /// Open a document and get a handle for working with it
    pub async fn open_document(self: &Arc<Self>, path: impl AsRef<Path>) -> Result<Document> {
        Document::open(self.clone(), path).await
    }

    async fn read_messages(
        stdout: ChildStdout,
        pending_requests: Arc<Mutex<HashMap<i64, oneshot::Sender<Value>>>>,
        notification_tx: broadcast::Sender<ServerNotification>,
    ) {
        let mut reader = BufReader::new(stdout);

        loop {
            match Self::read_message(&mut reader).await {
                Ok(msg) => {
                    if let Err(e) =
                        Self::handle_message(msg, &pending_requests, &notification_tx).await
                    {
                        eprintln!("Error handling message: {}", e);
                    }
                }
                Err(e) => {
                    eprintln!("Error reading message: {}", e);
                    break;
                }
            }
        }
    }

    async fn read_message(reader: &mut BufReader<ChildStdout>) -> Result<JsonRpcMessage> {
        let mut headers = HashMap::new();

        loop {
            let mut line = String::new();
            reader.read_line(&mut line).await?;

            if line == "\r\n" || line == "\n" {
                break;
            }

            if let Some((key, value)) = line.split_once(':') {
                headers.insert(key.trim().to_lowercase(), value.trim().to_string());
            }
        }

        let length: usize = headers
            .get("content-length")
            .ok_or_else(|| LspError::ServerError("Missing Content-Length".to_string()))?
            .parse()
            .map_err(|_| LspError::ServerError("Invalid Content-Length".to_string()))?;

        let mut buffer = vec![0u8; length];
        reader.read_exact(&mut buffer).await?;

        let content = String::from_utf8(buffer)
            .map_err(|_| LspError::ServerError("Invalid UTF-8".to_string()))?;
        let msg: JsonRpcMessage = serde_json::from_str(&content)?;

        Ok(msg)
    }

    async fn handle_message(
        msg: JsonRpcMessage,
        pending_requests: &Arc<Mutex<HashMap<i64, oneshot::Sender<Value>>>>,
        notification_tx: &broadcast::Sender<ServerNotification>,
    ) -> Result<()> {
        if let Some(id) = msg.id {
            let mut pending = pending_requests.lock().await;
            if let Some(tx) = pending.remove(&id) {
                if let Some(result) = msg.result {
                    let _ = tx.send(result);
                } else if let Some(error) = msg.error {
                    eprintln!("Server error: {:?}", error);
                }
            }
        } else if let Some(method) = msg.method {
            let notification = match method.as_str() {
                "textDocument/publishDiagnostics" => {
                    if let Some(params) = msg.params {
                        let params: PublishDiagnosticsParams = serde_json::from_value(params)?;
                        ServerNotification::PublishDiagnostics(params)
                    } else {
                        return Ok(());
                    }
                }
                "window/showMessage" => {
                    if let Some(params) = msg.params {
                        let params: ShowMessageParams = serde_json::from_value(params)?;
                        ServerNotification::ShowMessage(params)
                    } else {
                        return Ok(());
                    }
                }
                "window/logMessage" => {
                    if let Some(params) = msg.params {
                        let params: LogMessageParams = serde_json::from_value(params)?;
                        ServerNotification::LogMessage(params)
                    } else {
                        return Ok(());
                    }
                }
                _ => ServerNotification::Other {
                    method,
                    params: msg.params.unwrap_or(Value::Null),
                },
            };

            let _ = notification_tx.send(notification);
        }

        Ok(())
    }

    async fn write_message(&self, msg: &JsonRpcMessage) -> Result<()> {
        let content = serde_json::to_string(msg)?;
        let message = format!("Content-Length: {}\r\n\r\n{}", content.len(), content);

        let mut writer = self.writer.lock().await;
        writer.write_all(message.as_bytes()).await?;
        writer.flush().await?;

        Ok(())
    }

    async fn send_request<P, R>(&self, method: &str, params: P) -> Result<R>
    where
        P: Serialize,
        R: serde::de::DeserializeOwned,
    {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();

        self.pending_requests.lock().await.insert(id, tx);

        let msg = JsonRpcMessage {
            jsonrpc: "2.0".to_string(),
            id: Some(id),
            method: Some(method.to_string()),
            params: Some(serde_json::to_value(params)?),
            result: None,
            error: None,
        };

        self.write_message(&msg).await?;

        let response = rx.await.map_err(|_| LspError::Cancelled)?;
        Ok(serde_json::from_value(response)?)
    }

    async fn send_notification<P>(&self, method: &str, params: P) -> Result<()>
    where
        P: Serialize,
    {
        let msg = JsonRpcMessage {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: Some(method.to_string()),
            params: Some(serde_json::to_value(params)?),
            result: None,
            error: None,
        };

        self.write_message(&msg).await?;
        Ok(())
    }

    pub async fn shutdown(&self) -> Result<()> {
        // Send shutdown request with a timeout
        let shutdown_result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.send_request::<_, Value>("shutdown", ()),
        )
        .await;

        match shutdown_result {
            Ok(Ok(_)) => {
                // Shutdown succeeded, send exit notification
                let _ = self.send_notification("exit", ()).await;
            }
            Ok(Err(e)) => {
                eprintln!("Shutdown request failed: {}, sending exit anyway", e);
                let _ = self.send_notification("exit", ()).await;
            }
            Err(_) => {
                eprintln!("Shutdown request timed out, sending exit anyway");
                let _ = self.send_notification("exit", ()).await;
            }
        }

        Ok(())
    }
}

/// A handle to an open document that provides convenient methods
pub struct Document {
    client: Arc<LspClient>,
    uri: Uri,
    version: Arc<Mutex<i32>>,
}

impl Document {
    async fn open(client: Arc<LspClient>, path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().canonicalize()?;
        let uri = Uri::from_str(&format!("file://{}", path.display()))
            .map_err(|_| LspError::InvalidUri(path.display().to_string()))?;

        let content = tokio::fs::read_to_string(&path).await?;
        let language_id = Self::detect_language(&path);

        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id,
                version: 1,
                text: content,
            },
        };

        client
            .send_notification("textDocument/didOpen", params)
            .await?;

        Ok(Self {
            client,
            uri,
            version: Arc::new(Mutex::new(1)),
        })
    }

    pub fn uri(&self) -> &Uri {
        &self.uri
    }

    pub async fn update(&self, text: String) -> Result<()> {
        let mut version = self.version.lock().await;
        *version += 1;

        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: self.uri.clone(),
                version: *version,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text,
            }],
        };

        self.client
            .send_notification("textDocument/didChange", params)
            .await
    }

    pub async fn completion(
        &self,
        line: u32,
        character: u32,
    ) -> Result<Option<Vec<CompletionItem>>> {
        let params = CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: self.uri.clone(),
                },
                position: Position { line, character },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        };

        let response: Option<CompletionResponse> = self
            .client
            .send_request("textDocument/completion", params)
            .await?;

        Ok(response.map(|r| match r {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        }))
    }

    pub async fn hover(&self, line: u32, character: u32) -> Result<Option<Hover>> {
        let params = HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: self.uri.clone(),
                },
                position: Position { line, character },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };

        self.client.send_request("textDocument/hover", params).await
    }

    pub async fn goto_definition(
        &self,
        line: u32,
        character: u32,
    ) -> Result<Option<Vec<Location>>> {
        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: self.uri.clone(),
                },
                position: Position { line, character },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let response: Option<GotoDefinitionResponse> = self
            .client
            .send_request("textDocument/definition", params)
            .await?;

        Ok(response.map(|r| match r {
            GotoDefinitionResponse::Scalar(loc) => vec![loc],
            GotoDefinitionResponse::Array(locs) => locs,
            GotoDefinitionResponse::Link(links) => links
                .into_iter()
                .map(|link| Location {
                    uri: link.target_uri,
                    range: link.target_selection_range,
                })
                .collect(),
        }))
    }

    pub async fn close(&self) -> Result<()> {
        let params = DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier {
                uri: self.uri.clone(),
            },
        };

        self.client
            .send_notification("textDocument/didClose", params)
            .await
    }

    fn detect_language(path: &Path) -> String {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| match ext {
                "rs" => "rust",
                "py" => "python",
                "js" => "javascript",
                "ts" => "typescript",
                "go" => "go",
                "cpp" | "cc" | "cxx" => "cpp",
                "c" => "c",
                _ => "plaintext",
            })
            .unwrap_or("plaintext")
            .to_string()
    }
}
