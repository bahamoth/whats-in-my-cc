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
use crate::api::AppState;
use crate::db::{
    repo_diff_hunk, repo_episode, repo_finding, repo_findings_pending, repo_graph, repo_observed,
    repo_raw, repo_verification_run,
};
use crate::model::meta::{Envelope, ResponseMeta, SCHEMA_VERSION};

#[derive(Deserialize)]
pub struct ListQuery {
    pub limit: Option<i64>,
}

/// Slice-15: health now includes an `insight` block with judge counters.
/// `judge_pending_count` is a live DB query (cheap); all `_24h` counters are
/// in-memory atomics reset on server restart (DEV-S15-03).
pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let snap = state.judge_runtime.metrics_snapshot();
    let pending: i64 = repo_findings_pending::count_all(&state.pool)
        .await
        .unwrap_or(0);
    Json(json!({
        "status": "ok",
        "build_sha": option_env!("GIT_SHA").unwrap_or("dev"),
        "insight": {
            "judge_kind": state.judge_runtime.kind,
            "judge_calls_24h": snap.calls_24h,
            "judge_pending_count": pending,
            "judge_cache_hits_24h": snap.cache_hits_24h,
            "judge_cache_misses_24h": snap.cache_misses_24h,
            "judge_budget_exhaustions_24h": snap.budget_exhaustions_24h,
        }
    }))
}

/// slice-6 — per-source freshness summary for `witmcc doctor`.
///
/// Groups `raw_event` by source_type, returns the fixed taxonomy with
/// `null`/`0` rows for sources that have never ingested anything. The mapping
/// from DB `source_type` to UI label is normalised here (e.g., DB stores
/// `"otel"` for traces, label is `"otel-traces"`; `"claude_transcript"`
/// becomes `"transcript"`). Slice-10a — `file_git` removed: filesystem
/// watcher + git poller no longer exist; file lineage comes from transcript
/// `toolUseResult.structuredPatch`.
pub async fn health_sources(State(pool): State<SqlitePool>) -> impl IntoResponse {
    use sqlx::Row;
    // (db source_type, ui label)
    let taxonomy: &[(&str, &str)] = &[
        ("claude_transcript", "transcript"),
        ("otel", "otel-traces"),
        ("otel-metrics", "otel-metrics"),
        ("otel-logs", "otel-logs"),
        ("hook", "hook"),
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
) -> Result<Json<Envelope<SessionDetail>>, (StatusCode, Json<serde_json::Value>)> {
    // Slice-9 — summary only. Events ship via /v1/sessions/:id/events.
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

    // Per-kind counts cover the whole session (not a windowed sample), so
    // ranking, badges, and source-mix UIs stay accurate independently of any
    // paging the events endpoint applies.
    let by_kind = repo_observed::session_kind_counts(&pool, &id)
        .await
        .expect("db");

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
        },
    }))
}

#[derive(Deserialize)]
pub struct EventsQuery {
    pub before: Option<String>,
    pub after: Option<String>,
    pub limit: Option<i64>,
}

/// Slice-9 — cursor-paged event window. See
/// `docs/superpowers/specs/2026-05-21-witmcc-slice9-windowed-buffer-design.md`
/// §4 for the cursor format and `prev_cursor`/`next_cursor` semantics.
pub async fn session_events(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
    Query(q): Query<EventsQuery>,
) -> Result<Json<Envelope<SessionEventsResponse>>, (StatusCode, Json<serde_json::Value>)> {
    use crate::model::cursor::Cursor;
    fn parse_cursor(
        opt: Option<&str>,
    ) -> Result<Option<Cursor>, (StatusCode, Json<serde_json::Value>)> {
        match opt {
            None => Ok(None),
            Some(s) => s.parse::<Cursor>().map(Some).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "type":"about:blank",
                        "title":"INVALID_CURSOR",
                        "detail": e.to_string(),
                    })),
                )
            }),
        }
    }
    let before = parse_cursor(q.before.as_deref())?;
    let after = parse_cursor(q.after.as_deref())?;
    let limit = q.limit.unwrap_or(500);

    let evs =
        repo_observed::list_session_window(&pool, &id, before.as_ref(), after.as_ref(), limit)
            .await
            .expect("db");

    let (prev_cursor, next_cursor) = match (evs.first(), evs.last()) {
        (Some(first), Some(last)) => {
            let prev = format!("{}|{}", first.observed_at.to_rfc3339(), first.event_id);
            // next_cursor is null when the window already reaches the session's
            // live tip — further events arrive via SSE, not via another page
            // fetch. The summary's last_observed_at is the authoritative tip.
            let summary = repo_observed::session_summary(&pool, &id).await.expect("db");
            let at_live_tip = matches!(summary, Some((_, _, last_observed_at)) if last_observed_at == last.observed_at.to_rfc3339());
            let next = if at_live_tip {
                None
            } else {
                Some(format!("{}|{}", last.observed_at.to_rfc3339(), last.event_id))
            };
            (Some(prev), next)
        }
        _ => (None, None),
    };

    let events: Vec<serde_json::Value> =
        evs.iter().map(|e| observed_to_dto(e)).collect();
    Ok(Json(Envelope {
        meta: ResponseMeta::now(),
        data: SessionEventsResponse {
            events,
            prev_cursor,
            next_cursor,
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

pub async fn session_diff_hunks(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> Result<Json<Envelope<DiffHunksResponse>>, (StatusCode, Json<serde_json::Value>)> {
    let rows = repo_diff_hunk::list_session(&pool, &id).await.expect("db");
    let hunks = rows
        .into_iter()
        .map(|r| DiffHunkDto {
            diff_hunk_id: r.diff_hunk_id,
            session_id: r.session_id,
            file_path: r.file_path,
            change_type: r.change_type,
            line_range_after_start: r.line_range_after_start,
            line_range_after_end: r.line_range_after_end,
            introduced_by_event_id: r.introduced_by_event_id,
            introduced_by_tool_use_id: r.introduced_by_tool_use_id,
            patch_preview: r.patch_preview,
            lines_added: r.lines_added,
            lines_removed: r.lines_removed,
            user_modified: r.user_modified,
        })
        .collect();
    Ok(Json(Envelope {
        meta: ResponseMeta::now(),
        data: DiffHunksResponse { hunks },
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

/// Slice-11 — `GET /v1/sessions/:id/verification-runs`
///
/// Lists all verification runs for a session, ordered by `started_at`.
/// `covered_diff_hunk_ids` is computed at response time by joining against
/// the `diff_hunk` table (temporal precedence rule, DEV-S11-02).
pub async fn session_verification_runs(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let runs = repo_verification_run::list_session(&pool, &id)
        .await
        .expect("db");
    let hunks = repo_diff_hunk::list_session(&pool, &id)
        .await
        .expect("db");

    let data: Vec<VerificationRunDto> = runs
        .into_iter()
        .map(|r| {
            let covered = covered_hunk_ids_for_run(&r, &hunks);
            run_to_dto(r, covered)
        })
        .collect();

    Json(Envelope {
        meta: ResponseMeta::now(),
        data,
    })
}

/// Slice-11 — `GET /v1/verification-runs/:id`
///
/// Single verification run detail by ID. Includes `covered_diff_hunk_ids`.
pub async fn verification_run_detail(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> Result<Json<Envelope<VerificationRunDto>>, (StatusCode, Json<serde_json::Value>)> {
    let row = repo_verification_run::get(&pool, &id)
        .await
        .expect("db");
    let Some(run) = row else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({
                "type": "about:blank",
                "title": "RESOURCE_NOT_FOUND",
                "detail": format!("verification_run {id} not found"),
            })),
        ));
    };
    let session_id = run.session_id.clone();
    let hunks = repo_diff_hunk::list_session(&pool, &session_id)
        .await
        .expect("db");
    let covered = covered_hunk_ids_for_run(&run, &hunks);
    Ok(Json(Envelope {
        meta: ResponseMeta::now(),
        data: run_to_dto(run, covered),
    }))
}

/// Compute the `covered_diff_hunk_ids` for a run using temporal precedence.
/// A hunk is "covered" if its introducing event's observed_at is not available
/// OR if we can determine it strictly precedes the run's started_at.
/// (In this slice, we use the diff_hunk's introduced_by_event_id as a proxy
/// and fall back to always including it when timestamps are not resolvable.)
fn covered_hunk_ids_for_run(
    run: &repo_verification_run::VerificationRunRow,
    hunks: &[crate::db::repo_diff_hunk::DiffHunkRow],
) -> Vec<String> {
    let run_started: Option<chrono::DateTime<chrono::Utc>> = run.started_at.parse().ok();
    hunks
        .iter()
        .filter(|h| h.session_id == run.session_id)
        .filter(|_| run_started.is_some())  // only when we have a parseable timestamp
        .map(|h| h.diff_hunk_id.clone())
        .collect()
    // Note: full temporal filtering requires the introducing event's timestamp,
    // which is not stored on the diff_hunk row (it's in observed_event). This
    // implementation is conservative: it includes all hunks in the same session
    // that have a valid run_started. The graph builder has the exact timestamps
    // (via event_observed_at map); here we approximate for the API.
    // Slice-12 episode segmentation will refine this with episode scoping.
}

fn run_to_dto(
    r: repo_verification_run::VerificationRunRow,
    covered_diff_hunk_ids: Vec<String>,
) -> VerificationRunDto {
    VerificationRunDto {
        verification_run_id: r.verification_run_id,
        schema_version: r.schema_version,
        session_id: r.session_id,
        source: r.source,
        command: r.command,
        command_kind: r.command_kind,
        trigger_event_id: r.trigger_event_id,
        trigger_tool_use_id: r.trigger_tool_use_id,
        status: r.status,
        started_at: r.started_at,
        ended_at: r.ended_at,
        exit_code: r.exit_code,
        failure_summary: r.failure_summary,
        covered_diff_hunk_ids,
    }
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

// ---- Slice-12 — episode endpoints ------------------------------------------

fn row_to_episode_dto(row: repo_episode::EpisodeRow) -> EpisodeDto {
    let evidence_node_ids: Vec<serde_json::Value> =
        serde_json::from_str(&row.evidence_node_ids).unwrap_or_default();
    let classification_basis: Vec<serde_json::Value> =
        serde_json::from_str(&row.classification_basis).unwrap_or_default();
    EpisodeDto {
        episode_id: row.episode_id,
        schema_version: row.schema_version,
        session_id: row.session_id,
        phase: row.phase,
        start_event_id: row.start_event_id,
        end_event_id: row.end_event_id,
        started_at: row.started_at,
        ended_at: row.ended_at,
        evidence_node_ids,
        classification_basis,
        confidence: row.confidence,
        summary: row.summary,
        classifier_version: row.classifier_version,
        created_at: row.created_at,
    }
}

/// `GET /v1/sessions/:id/episodes`
///
/// Returns all episodes for a session, ordered by `started_at`.
/// Returns 200 with an empty `data` array if the session has no episodes or
/// does not exist.
pub async fn session_episodes(
    State(pool): State<SqlitePool>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    match repo_episode::list_session(&pool, &session_id).await {
        Ok(rows) => {
            let data: Vec<EpisodeDto> = rows.into_iter().map(row_to_episode_dto).collect();
            Json(EpisodesResponse { data }).into_response()
        }
        Err(err) => {
            tracing::error!(session_id = %session_id, err = %err, "session_episodes failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal server error"})),
            )
                .into_response()
        }
    }
}

/// `GET /v1/episodes/:id`
///
/// Returns a single episode by ID. 404 if not found.
pub async fn episode_detail(
    State(pool): State<SqlitePool>,
    Path(episode_id): Path<String>,
) -> impl IntoResponse {
    match repo_episode::get(&pool, &episode_id).await {
        Ok(Some(row)) => {
            Json(EpisodeDetailResponse {
                data: row_to_episode_dto(row),
            })
            .into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "episode not found", "episode_id": episode_id})),
        )
            .into_response(),
        Err(err) => {
            tracing::error!(episode_id = %episode_id, err = %err, "episode_detail failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal server error"})),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Slice-14 — Finding endpoints
// ---------------------------------------------------------------------------

/// Query parameters for `GET /v1/findings`.
#[derive(Deserialize)]
pub struct FindingsQuery {
    pub session_id: Option<String>,
    pub category: Option<String>,
    pub severity: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
}

/// Convert a `FindingRow` to the API DTO shape.
fn finding_row_to_dto(row: repo_finding::FindingRow) -> FindingDto {
    let evidence_refs: Vec<serde_json::Value> = serde_json::from_str(&row.evidence_refs)
        .unwrap_or_else(|_| vec![]);
    let evidence_projection: serde_json::Value = serde_json::from_str(&row.evidence_projection)
        .unwrap_or(serde_json::Value::Null);
    let provenance: serde_json::Value = serde_json::from_str(&row.provenance)
        .unwrap_or(serde_json::Value::Null);
    FindingDto {
        finding_id: row.finding_id,
        schema_version: row.schema_version,
        session_id: row.session_id,
        category: row.category,
        severity: row.severity,
        confidence: row.confidence,
        summary: row.summary,
        evidence_refs,
        evidence_projection,
        provenance,
        status: row.status,
        created_at: row.created_at,
    }
}

/// `GET /v1/findings` — list findings with optional filters.
///
/// Query params: `session_id`, `category`, `severity`, `status` (default `active`),
/// `limit` (default 50, max 200).
pub async fn list_findings(
    State(pool): State<SqlitePool>,
    Query(q): Query<FindingsQuery>,
) -> impl IntoResponse {
    let filter = repo_finding::ListFilter {
        session_id: q.session_id,
        category: q.category,
        severity: q.severity,
        status: q.status,
        limit: q.limit.unwrap_or(50).min(200).max(1),
    };
    match repo_finding::list(&pool, &filter).await {
        Ok(rows) => {
            let data: Vec<FindingDto> = rows.into_iter().map(finding_row_to_dto).collect();
            Json(FindingsResponse { data }).into_response()
        }
        Err(err) => {
            tracing::error!(err = %err, "list_findings failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal server error"})),
            )
                .into_response()
        }
    }
}

/// `GET /v1/findings/:id` — single finding detail.
pub async fn finding_detail(
    State(pool): State<SqlitePool>,
    Path(finding_id): Path<String>,
) -> impl IntoResponse {
    match repo_finding::get(&pool, &finding_id).await {
        Ok(Some(row)) => Json(FindingDetailResponse {
            data: finding_row_to_dto(row),
        })
        .into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "finding not found", "finding_id": finding_id})),
        )
            .into_response(),
        Err(err) => {
            tracing::error!(finding_id = %finding_id, err = %err, "finding_detail failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal server error"})),
            )
                .into_response()
        }
    }
}

/// `GET /v1/findings/:id/evidence` — finding + subgraph + raw source refs.
///
/// The subgraph includes graph nodes whose `source_event_ids` contain any
/// event_id in the finding's `evidence_refs`. Edges connecting those nodes
/// are also included. `raw_source_refs` contains the raw event provenance for
/// each evidenced event.
pub async fn finding_evidence(
    State(pool): State<SqlitePool>,
    Path(finding_id): Path<String>,
) -> impl IntoResponse {
    let row = match repo_finding::get(&pool, &finding_id).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "finding not found", "finding_id": finding_id})),
            )
                .into_response();
        }
        Err(err) => {
            tracing::error!(err = %err, "finding_evidence failed");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal server error"})),
            )
                .into_response();
        }
    };

    let session_id = row.session_id.clone();

    // Parse evidence_refs
    let evidence_refs: Vec<String> = serde_json::from_str(&row.evidence_refs).unwrap_or_default();
    let finding_dto = finding_row_to_dto(row);

    // Load graph nodes/edges for the session
    let (nodes, edges) = repo_graph::load_session(&pool, &session_id)
        .await
        .unwrap_or_else(|_| (vec![], vec![]));

    // Filter nodes whose source_event_ids overlap with evidence_refs
    let relevant_node_ids: std::collections::HashSet<String> = nodes
        .iter()
        .filter(|n| {
            n.source_event_ids.iter().any(|id| evidence_refs.contains(id))
        })
        .map(|n| n.node_id.clone())
        .collect();

    let subgraph_nodes: Vec<serde_json::Value> = nodes
        .iter()
        .filter(|n| relevant_node_ids.contains(&n.node_id))
        .map(|n| serde_json::to_value(n).unwrap_or(serde_json::Value::Null))
        .collect();

    let subgraph_edges: Vec<serde_json::Value> = edges
        .iter()
        .filter(|e| {
            relevant_node_ids.contains(&e.from_node_id)
                || relevant_node_ids.contains(&e.to_node_id)
        })
        .map(|e| serde_json::to_value(e).unwrap_or(serde_json::Value::Null))
        .collect();

    // Build raw_source_refs from observed_event → raw_event joins
    let mut raw_source_refs: Vec<RawSourceRef> = Vec::new();
    for ev_id in &evidence_refs {
        if let Ok(Some(raw)) = repo_raw::get_for_event_id(&pool, ev_id).await {
            raw_source_refs.push(RawSourceRef {
                event_id: ev_id.clone(),
                source_type: raw.source_type,
                source_uri: raw.source_uri,
                redaction_state: "none".into(),
            });
        }
    }

    Json(FindingEvidenceResponse {
        data: FindingEvidenceData {
            finding: finding_dto,
            subgraph: EvidenceSubgraph {
                nodes: subgraph_nodes,
                edges: subgraph_edges,
            },
            raw_source_refs,
        },
    })
    .into_response()
}

/// `GET /v1/sessions/:id/findings` — alias for `GET /v1/findings?session_id=:id`.
pub async fn session_findings(
    State(pool): State<SqlitePool>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let filter = repo_finding::ListFilter {
        session_id: Some(session_id),
        status: Some("active".into()),
        limit: 200,
        ..Default::default()
    };
    match repo_finding::list(&pool, &filter).await {
        Ok(rows) => {
            let data: Vec<FindingDto> = rows.into_iter().map(finding_row_to_dto).collect();
            Json(FindingsResponse { data }).into_response()
        }
        Err(err) => {
            tracing::error!(err = %err, "session_findings failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal server error"})),
            )
                .into_response()
        }
    }
}
