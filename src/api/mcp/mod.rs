//! Slice-17 — MCP Streamable HTTP endpoint.
//!
//! Exposes POST /mcp (JSON-RPC) and GET /mcp (SSE notifications).
//! Protocol version pinned to "2024-11-05" (DEV-S17-02).

pub mod jsonrpc;
pub mod methods;
pub mod resources;
pub mod session;
pub mod tools;
pub mod transport;

pub use session::SessionRegistry;
