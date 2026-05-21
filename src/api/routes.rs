use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::SqlitePool;

use crate::api::dto::*;
use crate::db::{repo_graph, repo_observed, repo_raw};
use crate::model::meta::{Envelope, ResponseMeta, SCHEMA_VERSION};

#[derive(Deserialize)]
pub struct ListQuery {
    pub limit: Option<i64>,
}

pub async fn health() -> impl IntoResponse {
    Json(json!({"status":"ok","build_sha": option_env!("GIT_SHA").unwrap_or("dev")}))
}

/// slice-6 — per-source freshness summary for `witmcc doctor`.
///
/// Groups `raw_event` by source_type, returns the fixed taxonomy with
/// `null`/`0` rows for sources that have never ingested anything. The mapping
/// from DB `source_type` to UI label is normalised here (e.g., DB stores
/// `"otel"` for traces, label is `"otel-traces"`; `"file_git"` becomes
/// `"file-git"`; `"claude_transcript"` becomes `"transcript"`).
pub async fn health_sources(State(pool): State<SqlitePool>) -> impl IntoResponse {
    use sqlx::Row;
    // (db source_type, ui label)
    let taxonomy: &[(&str, &str)] = &[
        ("claude_transcript", "transcript"),
        ("otel", "otel-traces"),
        ("otel-metrics", "otel-metrics"),
        ("otel-logs", "otel-logs"),
        ("hook", "hook"),
        ("file_git", "file-git"),
    ];
    let rows = sqlx::query(
        "SELECT source_type,
                MAX(captured_at) AS last_ingested_at,
                SUM(CASE WHEN captured_at >= datetime('now','-1 day') THEN 1 ELSE 0 END) AS row_count_24h,
                COUNT(*) AS total_rows
           FROM raw_event GROUP BY source_type",
    )
    .fetch_all(&pool)
    .await
    .expect("db");
    let mut by_type: std::collections::HashMap<String, (Option<String>, i64, i64)> =
        std::collections::HashMap::new();
    for r in rows {
        let st: String = r.get("source_type");
        let last: Option<String> = r.try_get("last_ingested_at").ok();
        let cnt24: i64 = r.try_get("row_count_24h").unwrap_or(0);
        let total: i64 = r.try_get("total_rows").unwrap_or(0);
        by_type.insert(st, (last, cnt24, total));
    }
    let sources: Vec<SourceHealth> = taxonomy
        .iter()
        .map(|(db, label)| {
            let entry = by_type.get(*db);
            SourceHealth {
                label: (*label).to_string(),
                last_ingested_at: entry.and_then(|(t, _, _)| t.clone()),
                row_count_24h: entry.map(|(_, c, _)| *c).unwrap_or(0),
                total_rows: entry.map(|(_, _, t)| *t).unwrap_or(0),
            }
        })
        .collect();
    Json(Envelope {
        meta: ResponseMeta::now(),
        data: HealthSourcesResponse { sources },
    })
}

pub async fn list_sessions(
    State(pool): State<SqlitePool>,
    Query(q): Query<ListQuery>,
) -> impl IntoResponse {
    let limit = clamp_limit(q.limit);
    let rows = repo_observed::list_sessions(&pool, limit)
        .await
        .expect("db");
    let data: Vec<SessionListItem> = rows
        .into_iter()
        .map(|r| SessionListItem {
            session_id: r.session_id,
            first_observed_at: r.first_observed_at,
            last_observed_at: r.last_observed_at,
            event_count: r.event_count,
            source_uris: vec![],
            by_kind: r.by_kind,
        })
        .collect();
    Json(Envelope {
        meta: ResponseMeta::now(),
        data,
    })
}

pub async fn session_detail(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Envelope<SessionDetail>>, (StatusCode, Json<serde_json::Value>)> {
    let limit = clamp_limit(q.limit);

    // Summary first — accurate count + first/last across the WHOLE session,
    // independent of the `latest <limit>` events window we ship to the WebUI.
    // A `None` here means no rows for this session_id → 404.
    let Some((event_count, first_obs, last_obs)) = repo_observed::session_summary(&pool, &id)
        .await
        .expect("db")
    else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(
                json!({"type":"about:blank","title":"RESOURCE_NOT_FOUND","detail":format!("session {id} not found")}),
            ),
        ));
    };

    // Newest `limit` events, then reverse so the wire order is chronological
    // (the WebUI Timeline component still expects ASC).
    let mut evs = repo_observed::list_session_latest(&pool, &id, limit)
        .await
        .expect("db");
    evs.reverse();

    let mut by_kind = std::collections::BTreeMap::new();
    for e in &evs {
        *by_kind.entry(e.kind.as_str().to_string()).or_insert(0) += 1;
    }
    let events: Vec<serde_json::Value> = evs
        .iter()
        .map(|e| serde_json::to_value(observed_to_dto(e)).unwrap())
        .collect();
    Ok(Json(Envelope {
        meta: ResponseMeta::now(),
        data: SessionDetail {
            session_id: id,
            summary: SessionSummary {
                event_count,
                by_kind,
                first_observed_at: first_obs,
                last_observed_at: last_obs,
            },
            events,
        },
    }))
}

pub async fn session_graph(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> Result<Json<Envelope<GraphPayload>>, (StatusCode, Json<serde_json::Value>)> {
    let (nodes, edges) = repo_graph::load_session(&pool, &id).await.expect("db");
    // Empty graph_node is a VALID transient state — rebuild_session uses a
    // DELETE-then-INSERT pattern (not in a transaction), and a SELECT that
    // races between those two statements observes zero rows even though the
    // session has thousands of events. Returning 404 here caused the WebUI
    // SessionDetailPage to flicker its Timeline to "no observations" on
    // every transcript line landing during active sessions. Now we always
    // return 200 with whatever rows exist; the client decides how to render
    // an empty graph (no flicker, just an empty timeline until the rebuild
    // finishes and the next refetch lands).
    Ok(Json(Envelope {
        meta: ResponseMeta::now(),
        data: GraphPayload {
            nodes: nodes
                .iter()
                .map(|n| serde_json::to_value(n).unwrap())
                .collect(),
            edges: edges
                .iter()
                .map(|e| serde_json::to_value(e).unwrap())
                .collect(),
        },
    }))
}

pub async fn event_raw(
    State(pool): State<SqlitePool>,
    Path(event_id): Path<String>,
) -> Result<Json<Envelope<RawEventResponse>>, (StatusCode, Json<serde_json::Value>)> {
    let row = repo_raw::get_for_event_id(&pool, &event_id)
        .await
        .expect("db");
    let row = match row {
        Some(r) => r,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({
                    "type": "about:blank",
                    "title": "RESOURCE_NOT_FOUND",
                    "detail": format!("event {event_id} not found")
                })),
            ));
        }
    };

    let record = match std::str::from_utf8(&row.payload)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
    {
        Some(v) => v,
        None => serde_json::Value::Null,
    };

    let observed_payload_value: serde_json::Value =
        serde_json::from_str(&row.observed_payload).unwrap_or(serde_json::Value::Null);
    let telemetry = match &observed_payload_value {
        serde_json::Value::Object(map) => map.get("telemetry").cloned(),
        _ => None,
    };

    Ok(Json(Envelope {
        meta: ResponseMeta::now(),
        data: RawEventResponse {
            schema_version: SCHEMA_VERSION.into(),
            event_id: row.event_id,
            session_id: row.session_id,
            source: RawSource {
                kind: row.source_type,
                file_path: row.source_uri,
                line_no: row.source_line_no,
                ingested_at: row.captured_at,
            },
            record,
            record_type: row.kind,
            redaction_state: "none".into(),
            telemetry,
        },
    }))
}

fn clamp_limit(l: Option<i64>) -> i64 {
    let v = l.unwrap_or(500);
    v.clamp(1, 5000)
}

// Avoid coupling model::observed to serde details by hand-projecting.
fn observed_to_dto(e: &crate::model::observed::ObservedEvent) -> serde_json::Value {
    let telemetry = e
        .telemetry
        .as_ref()
        .map(|t| serde_json::to_value(t).unwrap_or(serde_json::Value::Null));
    json!({
        "event_id": e.event_id,
        "raw_event_id": e.raw_event_id,
        "session_id": e.session_id,
        "event_uuid": e.event_uuid,
        "parent_uuid": e.parent_uuid,
        "observed_at": e.observed_at.to_rfc3339(),
        "actor": e.actor.as_str(),
        "kind": e.kind.as_str(),
        "subkind": e.subkind,
        "tool_use_id": e.tool_use_id,
        "tool_name": e.tool_name,
        "turn_id": e.turn_id,
        "is_sidechain": e.is_sidechain,
        "is_meta": e.is_meta,
        "trace_id": e.trace_id,
        "span_id": e.span_id,
        "parent_span_id": e.parent_span_id,
        "latency_ms": e.latency_ms,
        "telemetry": telemetry,
        "payload": e.payload,
    })
}
