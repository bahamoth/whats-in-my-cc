//! Slice-17 — MCP method dispatcher.
//!
//! Handles all JSON-RPC methods sent via POST /mcp.

use serde_json::{json, Value};
use sqlx::SqlitePool;
use ulid::Ulid;

use crate::api::mcp::{
    jsonrpc::{codes, AnyResponse},
    resources,
    session::{McpSession, SessionRegistry},
    tools,
};

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "whats-in-my-cc";

/// Result of handling a single JSON-RPC request.
pub enum HandleResult {
    /// Send a JSON-RPC response to the caller.
    Response(AnyResponse),
    /// Notification: no response needed, but update state with new session id.
    Initialized,
    /// No response (notification with unknown method — silently ignore).
    Silent,
}

/// Dispatch one JSON-RPC request. Returns the response (or None for notifications).
pub async fn handle(
    method: &str,
    id: Option<Value>,
    params: &Value,
    pool: &SqlitePool,
    registry: &SessionRegistry,
    session_id: Option<&str>,
) -> HandleResult {
    // id is None for notifications; we use null in that case for error responses.
    let id_val = id.unwrap_or(Value::Null);

    match method {
        "initialize" => {
            // Generate a new session id.
            let new_sid = format!("mcps_{}", Ulid::new());
            let session = McpSession::new(new_sid.clone());
            registry.insert(session).await;

            let result = json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {
                    "tools": { "listChanged": false },
                    "resources": { "subscribe": true, "listChanged": false },
                    "prompts": { "listChanged": false },
                    "logging": {}
                },
                "serverInfo": {
                    "name": SERVER_NAME,
                    "version": env!("CARGO_PKG_VERSION")
                },
                "_sessionId": new_sid  // carried so transport can set the header
            });
            HandleResult::Response(AnyResponse::ok(id_val, result))
        }

        "notifications/initialized" => {
            if let Some(sid) = session_id {
                registry.mark_initialized(sid).await;
            }
            HandleResult::Initialized
        }

        "tools/list" => {
            HandleResult::Response(AnyResponse::ok(id_val, tools::tools_list_response()))
        }

        "tools/call" => {
            let name = params["name"].as_str().unwrap_or("");
            let args = &params["arguments"];
            let result = tools::dispatch(name, args, pool).await;
            HandleResult::Response(AnyResponse::ok(id_val, result))
        }

        "resources/list" => {
            let result = resources::resources_list(pool).await;
            HandleResult::Response(AnyResponse::ok(id_val, result))
        }

        "resources/templates/list" => {
            HandleResult::Response(AnyResponse::ok(id_val, resources::resource_templates()))
        }

        "resources/read" => {
            let uri = params["uri"].as_str().unwrap_or("");
            match resources::read_resource(uri, pool).await {
                Ok(contents) => HandleResult::Response(AnyResponse::ok(id_val, contents)),
                Err(msg) => {
                    // Unknown or not-found resource — return as an error result
                    HandleResult::Response(AnyResponse::err(
                        id_val,
                        codes::INVALID_PARAMS,
                        msg,
                    ))
                }
            }
        }

        "prompts/list" => {
            HandleResult::Response(AnyResponse::ok(id_val, json!({ "prompts": [] })))
        }

        _ => {
            // Notifications with unknown method: silent ignore.
            // Regular requests with unknown method: error.
            if id_val.is_null() {
                HandleResult::Silent
            } else {
                HandleResult::Response(AnyResponse::method_not_found(id_val))
            }
        }
    }
}
