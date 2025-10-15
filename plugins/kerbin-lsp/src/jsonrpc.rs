use serde::*;
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: i32,
    pub method: String,
    pub params: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsonRpcServerRequest {
    pub jsonrpc: String,
    pub id: i32,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone)]
pub enum JsonRpcMessage {
    Response(JsonRpcResponse),
    ServerRequest(JsonRpcServerRequest),
    Notification(JsonRpcNotification),
}
