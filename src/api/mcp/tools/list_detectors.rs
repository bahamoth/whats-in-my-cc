//! Plan 4 — MCP tool: whats_in_my_cc.list_detectors
//!
//! Returns the manifest catalog for all registered detectors. No arguments.
//! Spec §6.4: LLM calls this to understand what each detector detects, from
//! which raw fields, by what rule, and why — before proposing config changes.

use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::api::mcp::tools::tool_success;

pub async fn call(_args: &Value, _pool: &SqlitePool) -> Value {
    let catalog: Vec<_> = crate::insight::pipeline::all_detectors()
        .iter()
        .map(|d| d.manifest())
        .collect();
    tool_success(json!({ "data": catalog }))
}
