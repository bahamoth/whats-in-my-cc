//! Slice-17 — MCP resource catalogue.
//!
//! Six URI templates. `resources/list` returns concrete URIs from DB.
//! `resources/read` delegates to per-resource fetchers.

pub mod parse;

use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::db::{repo_diff_hunk, repo_finding, repo_graph, repo_observed};
use crate::model::meta::SCHEMA_VERSION;

/// The six resource URI templates per design §7.
pub fn resource_templates() -> Value {
    json!({
        "resourceTemplates": [
            {
                "uriTemplate": "whats-in-my-cc://sessions/{session_id}",
                "name": "Session summary",
                "mimeType": "application/json"
            },
            {
                "uriTemplate": "whats-in-my-cc://sessions/{session_id}/graph",
                "name": "Session graph",
                "mimeType": "application/json"
            },
            {
                "uriTemplate": "whats-in-my-cc://sessions/{session_id}/findings",
                "name": "Session findings",
                "mimeType": "application/json"
            },
            {
                "uriTemplate": "whats-in-my-cc://findings/{finding_id}",
                "name": "Finding detail",
                "mimeType": "application/json"
            },
            {
                "uriTemplate": "whats-in-my-cc://file-lineage/{session_id}",
                "name": "File lineage",
                "mimeType": "application/json"
            },
            {
                "uriTemplate": "whats-in-my-cc://otel/traces/{trace_id}",
                "name": "OTel trace",
                "mimeType": "application/json"
            }
        ]
    })
}

/// Build the `resources/list` response: concrete URIs from the DB.
pub async fn resources_list(pool: &SqlitePool) -> Value {
    let sessions = repo_observed::list_sessions(pool, 100)
        .await
        .unwrap_or_default();

    let mut resources: Vec<Value> = Vec::new();
    for s in &sessions {
        resources.push(json!({
            "uri": format!("whats-in-my-cc://sessions/{}", s.session_id),
            "name": format!("Session {}", s.session_id),
            "mimeType": "application/json"
        }));
        resources.push(json!({
            "uri": format!("whats-in-my-cc://sessions/{}/graph", s.session_id),
            "name": format!("Graph for {}", s.session_id),
            "mimeType": "application/json"
        }));
        resources.push(json!({
            "uri": format!("whats-in-my-cc://sessions/{}/findings", s.session_id),
            "name": format!("Findings for {}", s.session_id),
            "mimeType": "application/json"
        }));
    }

    json!({ "resources": resources })
}

/// Dispatch `resources/read` for a given URI.
pub async fn read_resource(uri: &str, pool: &SqlitePool) -> Result<Value, String> {
    use parse::ResourceUri;
    let parsed = parse::parse(uri).ok_or_else(|| format!("unknown resource URI: {uri}"))?;
    match parsed {
        ResourceUri::Session(session_id) => read_session(&session_id, uri, pool).await,
        ResourceUri::SessionGraph(session_id) => read_graph(&session_id, uri, pool).await,
        ResourceUri::SessionFindings(session_id) => read_findings(&session_id, uri, pool).await,
        ResourceUri::Finding(finding_id) => read_finding(&finding_id, uri, pool).await,
        ResourceUri::FileLineage(session_id) => read_file_lineage(&session_id, uri, pool).await,
        ResourceUri::OtelTrace(trace_id) => read_otel_trace(&trace_id, uri, pool).await,
    }
}

fn make_contents(uri: &str, data: Value) -> Value {
    let text = serde_json::to_string(&json!({
        "meta": { "schema_version": SCHEMA_VERSION },
        "data": data
    }))
    .unwrap_or_else(|_| "{}".into());
    json!({
        "contents": [{
            "uri": uri,
            "mimeType": "application/json",
            "text": text
        }]
    })
}

async fn read_session(session_id: &str, uri: &str, pool: &SqlitePool) -> Result<Value, String> {
    let summary = repo_observed::session_summary(pool, session_id)
        .await
        .map_err(|e| e.to_string())?;
    match summary {
        None => Err(format!("session not found: {session_id}")),
        Some((event_count, first_obs, last_obs)) => {
            Ok(make_contents(uri, json!({
                "session_id": session_id,
                "event_count": event_count,
                "first_observed_at": first_obs,
                "last_observed_at": last_obs
            })))
        }
    }
}

async fn read_graph(session_id: &str, uri: &str, pool: &SqlitePool) -> Result<Value, String> {
    let (nodes, edges) = repo_graph::load_session(pool, session_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(make_contents(uri, json!({
        "nodes": nodes.iter().map(|n| serde_json::to_value(n).unwrap_or(Value::Null)).collect::<Vec<_>>(),
        "edges": edges.iter().map(|e| serde_json::to_value(e).unwrap_or(Value::Null)).collect::<Vec<_>>()
    })))
}

async fn read_findings(session_id: &str, uri: &str, pool: &SqlitePool) -> Result<Value, String> {
    let filter = repo_finding::ListFilter {
        session_id: Some(session_id.to_string()),
        status: Some("active".into()),
        limit: 200,
        ..Default::default()
    };
    let rows = repo_finding::list(pool, &filter)
        .await
        .map_err(|e| e.to_string())?;
    let findings: Vec<Value> = rows.into_iter().map(|r| json!({
        "finding_id": r.finding_id,
        "category": r.category,
        "severity": r.severity,
        "confidence": r.confidence,
        "summary": r.summary,
        "status": r.status
    })).collect();
    Ok(make_contents(uri, json!({ "findings": findings })))
}

async fn read_finding(finding_id: &str, uri: &str, pool: &SqlitePool) -> Result<Value, String> {
    let row = repo_finding::get(pool, finding_id)
        .await
        .map_err(|e| e.to_string())?;
    match row {
        None => Err(format!("finding not found: {finding_id}")),
        Some(r) => {
            let evidence_refs: Vec<Value> =
                serde_json::from_str(&r.evidence_refs).unwrap_or_default();
            Ok(make_contents(uri, json!({
                "finding_id": r.finding_id,
                "session_id": r.session_id,
                "category": r.category,
                "severity": r.severity,
                "confidence": r.confidence,
                "summary": r.summary,
                "evidence_refs": evidence_refs,
                "status": r.status
            })))
        }
    }
}

async fn read_file_lineage(session_id: &str, uri: &str, pool: &SqlitePool) -> Result<Value, String> {
    let hunks = repo_diff_hunk::list_session(pool, session_id)
        .await
        .map_err(|e| e.to_string())?;
    let hunks_json: Vec<Value> = hunks.into_iter().map(|h| json!({
        "diff_hunk_id": h.diff_hunk_id,
        "file_path": h.file_path,
        "change_type": h.change_type,
        "lines_added": h.lines_added,
        "lines_removed": h.lines_removed,
        "introduced_by_event_id": h.introduced_by_event_id
    })).collect();
    Ok(make_contents(uri, json!({
        "session_id": session_id,
        "diff_hunks": hunks_json
    })))
}

async fn read_otel_trace(trace_id: &str, uri: &str, pool: &SqlitePool) -> Result<Value, String> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT event_id, session_id, observed_at, span_id, parent_span_id \
         FROM observed_event \
         WHERE kind = 'otel_span' AND trace_id = ? \
         ORDER BY observed_at ASC \
         LIMIT 200",
    )
    .bind(trace_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let spans: Vec<Value> = rows.into_iter().map(|r| {
        let event_id: String = r.get("event_id");
        let session_id: String = r.get("session_id");
        let observed_at: String = r.get("observed_at");
        let span_id: Option<String> = r.try_get("span_id").ok().flatten();
        let parent_span_id: Option<String> = r.try_get("parent_span_id").ok().flatten();
        json!({
            "event_id": event_id,
            "session_id": session_id,
            "observed_at": observed_at,
            "span_id": span_id,
            "parent_span_id": parent_span_id
        })
    }).collect();

    Ok(make_contents(uri, json!({
        "trace_id": trace_id,
        "spans": spans
    })))
}
