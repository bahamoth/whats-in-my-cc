//! Slice-17 — MCP tool: whats_in_my_cc.search_findings

use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::api::mcp::tools::{tool_error, tool_success};
use crate::db::repo_finding;

pub async fn call(args: &Value, pool: &SqlitePool) -> Value {
    let filter = repo_finding::ListFilter {
        session_id: args["session_id"].as_str().map(str::to_string),
        category: args["category"].as_str().map(str::to_string),
        severity: args["severity"].as_str().map(str::to_string),
        status: None, // return all statuses when called from MCP
        subkind: args["subkind"].as_str().map(str::to_string),
        limit: args["limit"].as_i64().unwrap_or(50).clamp(1, 200),
    };

    let rows = match repo_finding::list(pool, &filter).await {
        Ok(r) => r,
        Err(e) => return tool_error(format!("db error: {e}")),
    };

    let data: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            let evidence_refs: Vec<Value> =
                serde_json::from_str(&r.evidence_refs).unwrap_or_default();
            json!({
                "finding_id": r.finding_id,
                "session_id": r.session_id,
                "category": r.category,
                "severity": r.severity,
                "confidence": r.confidence,
                "summary": r.summary,
                "status": r.status,
                "evidence_refs": evidence_refs,
                "created_at": r.created_at
            })
        })
        .collect();

    tool_success(json!({ "data": data }))
}
