//! Slice-17 — JSON-RPC 2.0 framing types.
//!
//! Minimal implementation covering the MCP subset:
//! - Request (single or batch)
//! - Response (success or error)
//! - Error codes used by MCP
//!
//! Pin: MCP protocolVersion "2024-11-05" (DEV-S17-02).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC 2.0 request.
#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    /// `id` is None for notifications (e.g., notifications/initialized).
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// JSON-RPC 2.0 response (success).
#[derive(Debug, Clone, Serialize)]
pub struct Response {
    pub jsonrpc: String,
    pub id: Value,
    pub result: Value,
}

/// JSON-RPC 2.0 error response.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorResponse {
    pub jsonrpc: String,
    pub id: Value,
    pub error: RpcError,
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Standard JSON-RPC error codes (subset used by MCP).
pub mod codes {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;
}

impl Response {
    pub fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result,
        }
    }
}

impl ErrorResponse {
    pub fn new(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            error: RpcError {
                code,
                message: message.into(),
                data: None,
            },
        }
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.error.data = Some(data);
        self
    }

    pub fn method_not_found(id: Value) -> Self {
        Self::new(id, codes::METHOD_NOT_FOUND, "Method not found")
    }

    pub fn internal(id: Value, msg: impl Into<String>) -> Self {
        Self::new(id, codes::INTERNAL_ERROR, msg)
    }
}

/// Either a success or error JSON-RPC response, serialised as the correct variant.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum AnyResponse {
    Ok(Response),
    Err(ErrorResponse),
}

impl AnyResponse {
    pub fn ok(id: Value, result: Value) -> Self {
        Self::Ok(Response::ok(id, result))
    }

    pub fn err(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self::Err(ErrorResponse::new(id, code, message))
    }

    pub fn method_not_found(id: Value) -> Self {
        Self::Err(ErrorResponse::method_not_found(id))
    }
}
