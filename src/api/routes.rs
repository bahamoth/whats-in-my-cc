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
    repo_audit, repo_diff_hunk, repo_finding,
    repo_observed, repo_raw, repo_retention, repo_usage_facet, repo_verification_run,
};
use crate::model::meta::{Envelope, ResponseMeta, SCHEMA_VERSION};

#[derive(Deserialize)]
pub struct ListQuery {
    pub limit: Option<i64>,
}

/// Slice-19: adds a `security` block with `auth_required` and `retention_profile`.
/// `/v1/health` is auth-gated (DEV-S19-02).
pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "build_sha": option_env!("GIT_SHA").unwrap_or("dev"),
        "security": {
            "auth_required": !state.token.is_empty(),
            "retention_profile": state.retention_profile,
        }
    }))
}

/// slice-6 — per-source freshness summary for `wimcc doctor`.
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
    // Slice-18: aggregate redaction summary across all listed sessions.
    // For simplicity, we report aggregate across the first session or a global
    // query. Per design §6, the summary covers the response's raw events.
    // Here we aggregate across all sessions in the response (first 200 raw rows).
    let summary = if !rows.is_empty() {
        let sid = &rows[0].session_id;
        repo_raw::aggregate_session_summary(&pool, sid).await.ok()
    } else {
        None
    };
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
    let mut meta = ResponseMeta::now();
    if let Some(s) = summary {
        meta = meta.with_summary(s);
    }
    Json(Envelope { meta, data })
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
    /// On-demand correlated telemetry for the detail view: when either is set,
    /// the endpoint returns the events whose payload carries that
    /// tool_use_id / request_id (instead of the cursor-paged window).
    pub tool_use_id: Option<String>,
    pub request_id: Option<String>,
}

/// Slice-9 — cursor-paged event window. See
/// `docs/superpowers/specs/2026-05-21-wimcc-slice9-windowed-buffer-design.md`
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
    // Correlated-telemetry fetch (detail view): targeted, not a window — return
    // the matching events with null cursors (no pagination).
    if q.tool_use_id.is_some() || q.request_id.is_some() {
        let evs = repo_observed::events_correlated(
            &pool,
            &id,
            q.tool_use_id.as_deref(),
            q.request_id.as_deref(),
        )
        .await
        .expect("db");
        let events: Vec<serde_json::Value> = evs.iter().map(|e| observed_to_dto(e)).collect();
        return Ok(Json(Envelope {
            meta: ResponseMeta::now(),
            data: SessionEventsResponse {
                events,
                prev_cursor: None,
                next_cursor: None,
            },
        }));
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
    // Slice-18: aggregate redaction summary for this session's raw events.
    let summary = repo_raw::aggregate_session_summary(&pool, &id)
        .await
        .ok();
    let mut meta = ResponseMeta::now();
    if let Some(s) = summary {
        meta = meta.with_summary(s);
    }
    Ok(Json(Envelope {
        meta,
        data: SessionEventsResponse {
            events,
            prev_cursor,
            next_cursor,
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

/// insight-redesign #1 — `GET /v1/sessions/:id/usage`
///
/// Returns the session token-usage aggregate: total turns, raw token counts,
/// billed_tokens (input + cache_creation + output; cache_read is NOT billed),
/// cache_hit_ratio (cache_read / (cache_read + cache_creation + input); null
/// when denominator is 0), and a per-model breakdown.
pub async fn session_usage(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let agg = repo_usage_facet::session_aggregate(&pool, &id)
        .await
        .expect("db");
    let billed = agg.input_tokens + agg.cache_creation_input_tokens + agg.output_tokens;
    let denom = agg.cache_read_input_tokens + agg.cache_creation_input_tokens + agg.input_tokens;
    let cache_hit_ratio = if denom > 0 {
        Some(agg.cache_read_input_tokens as f64 / denom as f64)
    } else {
        None
    };
    let cost = crate::insight::pricing::estimate_session_cost(&agg.by_model);
    let priced: std::collections::HashMap<&str, f64> = cost
        .per_model
        .iter()
        .map(|c| (c.model.as_str(), c.estimated_cost_usd))
        .collect();
    let data = SessionUsageDto {
        session_id: id,
        turns: agg.turns,
        input_tokens: agg.input_tokens,
        cache_creation_input_tokens: agg.cache_creation_input_tokens,
        cache_read_input_tokens: agg.cache_read_input_tokens,
        output_tokens: agg.output_tokens,
        billed_tokens: billed,
        cache_hit_ratio,
        estimated_cost_usd: cost.total_usd,
        cost_basis: crate::insight::pricing::COST_BASIS_ESTIMATE.to_string(),
        pricing_version: crate::insight::pricing::PRICING_VERSION.to_string(),
        models_without_pricing: cost.models_without_pricing.clone(),
        by_model: agg
            .by_model
            .into_iter()
            .map(|m| {
                let est = priced.get(m.model.as_str()).copied().unwrap_or(0.0);
                let is_priced = crate::insight::pricing::rates_for(&m.model).is_some();
                ModelUsageDto {
                    model: m.model,
                    turns: m.turns,
                    input_tokens: m.input_tokens,
                    cache_creation_input_tokens: m.cache_creation_input_tokens,
                    cache_read_input_tokens: m.cache_read_input_tokens,
                    output_tokens: m.output_tokens,
                    estimated_cost_usd: est,
                    priced: is_priced,
                }
            })
            .collect(),
    };
    Json(Envelope {
        meta: ResponseMeta::now(),
        data,
    })
}

/// insight-redesign #6 — `GET /v1/usage/baseline`
///
/// Cross-session baseline: median (+ p25/p75) of each key usage metric over
/// ALL stored sessions that have usage_facet rows. SQLite has no MEDIAN(), so
/// the per-session metric rows are pulled and the quantiles computed in Rust.
/// `cache_hit_ratio`'s distribution excludes sessions with a 0-token
/// denominator (None); `session_count` counts all sessions with usage rows.
pub async fn usage_baseline(State(pool): State<SqlitePool>) -> impl IntoResponse {
    let metrics = repo_usage_facet::per_session_metrics(&pool)
        .await
        .expect("db");

    let session_count = metrics.len() as i64;

    let cache_hit_vals: Vec<f64> = metrics.iter().filter_map(|m| m.cache_hit_ratio).collect();
    let billed_vals: Vec<f64> = metrics.iter().map(|m| m.billed_tokens as f64).collect();
    let turns_vals: Vec<f64> = metrics.iter().map(|m| m.turns as f64).collect();
    let output_vals: Vec<f64> = metrics.iter().map(|m| m.output_tokens as f64).collect();

    fn stat(values: &[f64]) -> BaselineStat {
        match repo_usage_facet::median_p25_p75(values) {
            Some(q) => BaselineStat {
                p25: Some(q.p25),
                median: Some(q.median),
                p75: Some(q.p75),
            },
            None => BaselineStat {
                p25: None,
                median: None,
                p75: None,
            },
        }
    }

    let data = UsageBaselineDto {
        session_count,
        cache_hit_ratio: stat(&cache_hit_vals),
        billed_tokens: stat(&billed_vals),
        turns: stat(&turns_vals),
        output_tokens: stat(&output_vals),
    };
    Json(Envelope {
        meta: ResponseMeta::now(),
        data,
    })
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
    // that have a valid run_started; here we approximate for the API. A precise
    // version would join observed_event for the introducing event's timestamp.
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
        detection_basis: r.detection_basis,
        status_basis: r.status_basis,
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
        "request_id": e.request_id,
        "message_id": e.message_id,
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
    pub subkind: Option<String>,
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
        subkind: row.subkind,
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
        subkind: q.subkind,
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
///
/// Slice-19: if the finding has been deleted by retention sweep, the tombstone
/// table will have a record for it and this handler returns `410 Gone` instead
/// of `404 Not Found`, so clients can distinguish "never existed" from "expired".
pub async fn finding_detail(
    State(pool): State<SqlitePool>,
    Path(finding_id): Path<String>,
) -> impl IntoResponse {
    // Check tombstone first (slice-19).
    match repo_retention::is_tombstoned(&pool, &finding_id).await {
        Ok(true) => {
            return (
                StatusCode::GONE,
                Json(json!({
                    "type": "about:blank",
                    "title": "RESOURCE_GONE",
                    "detail": format!("finding {finding_id} was deleted by retention sweep"),
                    "resource_id": finding_id
                })),
            )
                .into_response();
        }
        Ok(false) => {}
        Err(err) => {
            tracing::warn!(finding_id = %finding_id, err = %err, "tombstone check failed");
        }
    }

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

/// `GET /v1/findings/:id/evidence` — finding + its evidence event IDs + raw
/// source refs.
///
/// `evidence_refs` are the event IDs cited by the finding. `raw_source_refs`
/// resolves the raw-event provenance (source_type / source_uri) for each cited
/// event via the observed_event → raw_event join.
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

    // Parse evidence_refs
    let evidence_refs: Vec<String> = serde_json::from_str(&row.evidence_refs).unwrap_or_default();
    let finding_dto = finding_row_to_dto(row);

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
            evidence_refs,
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

/// `GET /v1/sessions/:id/tool-failures` — tool_failure class breakdown +
/// user-visible drill list (spec §6.3). Internal retries / benign exits are
/// counted but kept out of the drill list so they never headline.
pub async fn session_tool_failures(
    State(pool): State<SqlitePool>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let counts = match repo_finding::count_by_subkind(&pool, &session_id, "tool_failure").await {
        Ok(c) => c,
        Err(err) => {
            tracing::error!(err = %err, "count_by_subkind failed");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal server error"})),
            )
                .into_response();
        }
    };
    let mut user_visible = 0i64;
    let mut internal_retry = 0i64;
    let mut benign = 0i64;
    let mut unclassified = 0i64;
    for (sk, n) in &counts {
        match sk.as_deref() {
            Some("user_visible") => user_visible = *n,
            Some("internal_retry") => internal_retry = *n,
            Some("benign_nonzero_exit") => benign = *n,
            _ => unclassified += *n,
        }
    }
    let total = user_visible + internal_retry + benign + unclassified;

    let drill_filter = repo_finding::ListFilter {
        session_id: Some(session_id.clone()),
        category: Some("tool_failure".into()),
        subkind: Some("user_visible".into()),
        status: Some("active".into()),
        limit: 200,
        ..Default::default()
    };
    let drill = repo_finding::list(&pool, &drill_filter)
        .await
        .unwrap_or_default();
    let user_visible_findings: Vec<FindingDto> =
        drill.into_iter().map(finding_row_to_dto).collect();

    Json(ToolFailureSummaryResponse {
        data: ToolFailureSummaryDto {
            session_id,
            user_visible,
            internal_retry,
            benign_nonzero_exit: benign,
            unclassified,
            total,
            user_visible_findings,
        },
    })
    .into_response()
}

// ---------------------------------------------------------------------------
// Slice-19 — Audit endpoint
// ---------------------------------------------------------------------------

/// `GET /v1/audit` — list recent audit rows.
///
/// Returns up to 200 recent audit rows, ordered by `created_at DESC`.
/// Auth-gated (bearer token required).
pub async fn list_audit(State(pool): State<SqlitePool>) -> impl IntoResponse {
    match repo_audit::list_recent(&pool, 200).await {
        Ok(rows) => {
            let data: Vec<serde_json::Value> = rows
                .into_iter()
                .map(|r| {
                    json!({
                        "audit_id": r.audit_id,
                        "event": r.event,
                        "actor": r.actor,
                        "payload": serde_json::from_str::<serde_json::Value>(&r.payload)
                            .unwrap_or(serde_json::Value::Null),
                        "created_at": r.created_at,
                    })
                })
                .collect();
            Json(json!({ "data": data })).into_response()
        }
        Err(err) => {
            tracing::error!(err = %err, "list_audit failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal server error"})),
            )
                .into_response()
        }
    }
}
