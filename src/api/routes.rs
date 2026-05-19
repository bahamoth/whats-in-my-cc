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
    let evs = repo_observed::list_session(&pool, &id, limit)
        .await
        .expect("db");
    if evs.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(
                json!({"type":"about:blank","title":"RESOURCE_NOT_FOUND","detail":format!("session {id} not found")}),
            ),
        ));
    }
    let mut by_kind = std::collections::BTreeMap::new();
    for e in &evs {
        *by_kind.entry(e.kind.as_str().to_string()).or_insert(0) += 1;
    }
    let first = evs.first().unwrap().observed_at.to_rfc3339();
    let last = evs.last().unwrap().observed_at.to_rfc3339();
    let events: Vec<serde_json::Value> = evs
        .iter()
        .map(|e| serde_json::to_value(observed_to_dto(e)).unwrap())
        .collect();
    Ok(Json(Envelope {
        meta: ResponseMeta::now(),
        data: SessionDetail {
            session_id: id,
            summary: SessionSummary {
                event_count: events.len() as i64,
                by_kind,
                first_observed_at: first,
                last_observed_at: last,
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
    if nodes.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(
                json!({"type":"about:blank","title":"RESOURCE_NOT_FOUND","detail":format!("session {id} has no graph")}),
            ),
        ));
    }
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
        },
    }))
}

fn clamp_limit(l: Option<i64>) -> i64 {
    let v = l.unwrap_or(500);
    v.clamp(1, 5000)
}

// Avoid coupling model::observed to serde details by hand-projecting.
fn observed_to_dto(e: &crate::model::observed::ObservedEvent) -> serde_json::Value {
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
        "payload": e.payload,
    })
}
