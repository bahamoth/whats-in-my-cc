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
    repo_audit, repo_diff_hunk, repo_observed, repo_raw, repo_retention, repo_signal,
    repo_usage_facet, repo_verification_run,
};
use crate::insight::event_tags::classify_tool_call;
use crate::insight::task_summary::{
    build_task_summaries, DiffSample, TagSample, TaskOp, TaskOpKind, UsageSample, VerifSample,
};
use crate::model::meta::{Envelope, ResponseMeta, SCHEMA_VERSION};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    pub limit: Option<i64>,
    /// Dogfood 2026-06-12 (§3-3) — only sessions with ≥1 event whose `cwd`
    /// equals this project root (trailing slash normalised). Lets the
    /// session-retrospect skill resolve "the sessions of this project"
    /// without hand-copied session IDs.
    pub project: Option<String>,
    /// perf-2026-06-29 — comma-separated opt-in expansions. The only token
    /// today is `redaction`, which re-enables `meta.redaction_summary` on
    /// `/v1/sessions`. It is omitted by default because computing it scans
    /// raw_event manifests across every listed session (≈1.9s on the dogfood
    /// DB) and no current consumer reads it from the list response.
    pub include: Option<String>,
}

impl ListQuery {
    /// True when `include=` contains the `redaction` token.
    fn wants_redaction(&self) -> bool {
        self.include
            .as_deref()
            .map(|s| s.split(',').any(|t| t.trim() == "redaction"))
            .unwrap_or(false)
    }
}

/// Full-retention (2026-06-11) — 410 gate shared by every handler whose
/// resource class the sweep tombstones (session, raw payload,
/// verification_run; signal_detail keeps its inline check). Fail-open on a
/// tombstone-table error, mirroring signal_detail: a broken gate must not
/// take down read paths.
async fn tombstone_gate(
    pool: &SqlitePool,
    resource_id: &str,
    kind: &str,
    what: &str,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    match repo_retention::is_tombstoned(pool, resource_id, kind).await {
        Ok(true) => Err((
            StatusCode::GONE,
            Json(json!({
                "type": "about:blank",
                "title": "RESOURCE_GONE",
                "detail": format!("{what} {resource_id} was deleted by retention sweep"),
                "resource_id": resource_id,
            })),
        )),
        Ok(false) => Ok(()),
        Err(err) => {
            tracing::warn!(resource_id = %resource_id, err = %err, "tombstone check failed");
            Ok(())
        }
    }
}

/// Slice-19: adds a `security` block with `auth_required` and `retention_profile`.
/// `/v1/health` is auth-gated (DEV-S19-02).
/// 2026-07-17 §4: adds a `version` block (`current`/`latest`/`update_available`)
/// sourced from the background update-check loop (see `update_check`).
pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let v = state.update_status.read().await;
    Json(json!({
        "status": "ok",
        "build_sha": option_env!("GIT_SHA").unwrap_or("dev"),
        "version": {
            "current": env!("CARGO_PKG_VERSION"),
            "latest": v.latest,
            "update_available": v.update_available,
        },
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

/// `GET /v1/plugins` — marketplace-installed plugins + provenance, resolved from
/// the `claude` CLI (cached once per process). Read-only; no DB state needed.
pub async fn list_plugins() -> impl IntoResponse {
    let data: Vec<PluginDto> = crate::plugins::cached_registry()
        .iter()
        .map(|e| PluginDto {
            id: e.id.clone(),
            plugin: e.plugin.clone(),
            marketplace: e.marketplace.clone(),
            provenance: e.provenance.as_str().to_string(),
            scope: e.scope.clone(),
            enabled: e.enabled,
            mcp_servers: e.mcp_servers.clone(),
            description: e.description.clone(),
        })
        .collect();
    Json(Envelope {
        meta: ResponseMeta::now(),
        data,
    })
}

fn parse_ms(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp_millis())
}

/// "Task #N created…" → "N". The TaskCreate taskId only appears in its result.
fn parse_task_num(content: &str) -> Option<String> {
    let i = content.find('#')?;
    let n: String = content[i + 1..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    (!n.is_empty()).then_some(n)
}

/// `GET /v1/sessions/:id/tasks` — per-task summary: TaskCreate/TaskUpdate
/// correlated (taskId resolved from each create's result) + work-span window
/// aggregations (tag histogram · diff · verification · tokens). Aggregation in
/// `crate::insight::task_summary`; this maps storage rows → its samples.
pub async fn session_tasks(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let events = repo_observed::list_session(&pool, &id, 100_000)
        .await
        .expect("db");
    let diffs = repo_diff_hunk::list_session(&pool, &id).await.expect("db");
    let verifs = repo_verification_run::list_session(&pool, &id)
        .await
        .expect("db");
    let usage = repo_usage_facet::list_session(&pool, &id)
        .await
        .expect("db");

    // event_id → at_ms (for diff timestamping); tool_use_id → result content
    // (for TaskCreate taskId resolution).
    let mut at_by_event: std::collections::HashMap<&str, i64> = std::collections::HashMap::new();
    let mut result_by_use: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    for e in &events {
        at_by_event.insert(&e.event_id, e.observed_at.timestamp_millis());
        if e.kind.as_str() == "tool_result" {
            if let (Some(uid), Some(c)) = (
                e.tool_use_id.as_deref(),
                e.payload
                    .get("tool_result")
                    .and_then(|t| t.get("content"))
                    .and_then(|c| c.as_str()),
            ) {
                result_by_use.insert(uid, c);
            }
        }
    }

    let mut ops: Vec<TaskOp> = Vec::new();
    let mut tags: Vec<TagSample> = Vec::new();
    for e in &events {
        if e.kind.as_str() != "tool_call" {
            continue;
        }
        let at_ms = e.observed_at.timestamp_millis();
        let name = e
            .tool_name
            .clone()
            .or_else(|| {
                e.payload
                    .get("tool_name")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
            .unwrap_or_default();
        // every tool_call contributes a tag sample (for the work histogram)
        let outcome = classify_tool_call(Some(&name), &e.payload);
        tags.push(TagSample {
            at_ms,
            tag: outcome.value.map(String::from),
        });
        let input = e.payload.get("input");
        let s = |k: &str| {
            input
                .and_then(|i| i.get(k))
                .and_then(|v| v.as_str())
                .map(String::from)
        };
        match name.as_str() {
            "TaskCreate" => {
                let task_id = e
                    .tool_use_id
                    .as_deref()
                    .and_then(|u| result_by_use.get(u))
                    .and_then(|c| parse_task_num(c));
                if let Some(task_id) = task_id {
                    ops.push(TaskOp {
                        event_id: e.event_id.clone(),
                        task_id,
                        at_ms,
                        kind: TaskOpKind::Create,
                        subject: s("subject"),
                        description: s("description"),
                        active_form: s("activeForm"),
                        status: None,
                    });
                }
            }
            "TaskUpdate" => {
                if let (Some(task_id), Some(status)) = (s("taskId"), s("status")) {
                    ops.push(TaskOp {
                        event_id: e.event_id.clone(),
                        task_id,
                        at_ms,
                        kind: TaskOpKind::Update,
                        subject: None,
                        description: None,
                        active_form: None,
                        status: Some(status),
                    });
                }
            }
            _ => {}
        }
    }

    let diff_samples: Vec<DiffSample> = diffs
        .iter()
        .filter_map(|d| {
            at_by_event
                .get(d.introduced_by_event_id.as_str())
                .map(|at| DiffSample {
                    at_ms: *at,
                    added: d.lines_added,
                    removed: d.lines_removed,
                })
        })
        .collect();
    let verif_samples: Vec<VerifSample> = verifs
        .iter()
        .filter_map(|v| {
            parse_ms(&v.started_at).map(|at| VerifSample {
                at_ms: at,
                status: v.status.clone(),
            })
        })
        .collect();
    let usage_samples: Vec<UsageSample> = usage
        .iter()
        .filter_map(|u| {
            parse_ms(&u.observed_at).map(|at| UsageSample {
                at_ms: at,
                input: u.input_tokens,
                output: u.output_tokens,
                cache_creation: u.cache_creation_input_tokens,
                cache_read: u.cache_read_input_tokens,
            })
        })
        .collect();

    let data: Vec<TaskDto> =
        build_task_summaries(&ops, &tags, &diff_samples, &verif_samples, &usage_samples)
            .into_iter()
            .map(|t| TaskDto {
                task_id: t.task_id,
                subject: t.subject,
                description: t.description,
                active_form: t.active_form,
                event_id: t.create_event_id,
                created_at_ms: t.created_at_ms,
                status: t.status,
                transitions: t
                    .transitions
                    .into_iter()
                    .map(|tr| TaskTransitionDto {
                        status: tr.status,
                        at_ms: tr.at_ms,
                        event_id: tr.event_id,
                    })
                    .collect(),
                duration_ms: t.duration_ms,
                work_duration_ms: t.work_duration_ms,
                saw_in_progress: t.saw_in_progress,
                activity_count: t.activity_count,
                tag_histogram: t
                    .tag_histogram
                    .into_iter()
                    .map(|(tag, count)| TaskHistEntryDto { tag, count })
                    .collect(),
                lines_added: t.lines_added,
                lines_removed: t.lines_removed,
                verification: t.verification.map(|v| TaskVerifDto {
                    passed: v.passed,
                    failed: v.failed,
                    unknown: v.unknown,
                    not_executed: v.not_executed,
                }),
                tokens: t.tokens.map(|k| TaskTokensDto {
                    input: k.input,
                    output: k.output,
                    cache_creation: k.cache_creation,
                    cache_read: k.cache_read,
                }),
            })
            .collect();

    Json(Envelope {
        meta: ResponseMeta::now(),
        data,
    })
}

pub async fn list_sessions(
    State(pool): State<SqlitePool>,
    Query(q): Query<ListQuery>,
) -> impl IntoResponse {
    let limit = clamp_limit(q.limit);
    let project = q
        .project
        .as_deref()
        .map(|p| p.trim_end_matches('/'))
        .filter(|p| !p.is_empty());
    let rows = repo_observed::list_sessions_filtered(&pool, limit, project)
        .await
        .expect("db");
    // Slice-18 + doc-audit-2026-06-10: meta.redaction_summary aggregates the
    // redaction manifests of *every* session in this response (per design §6,
    // the summary covers the response's raw events), not just the most
    // recent session.
    //
    // perf-2026-06-29: this aggregate scans raw_event manifests across every
    // listed session (≈1.9s on the dogfood DB) and the WebUI never reads it,
    // so it is now OPT-IN via `?include=redaction`. redaction_policy below is
    // static/cheap and stays on every response.
    let summary = if q.wants_redaction() {
        let session_ids: Vec<String> = rows.iter().map(|r| r.session_id.clone()).collect();
        repo_raw::aggregate_sessions_summary(&pool, &session_ids)
            .await
            .ok()
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
            project: r.project,
            model: r.model,
            slug: r.slug,
            first_user_message_preview: r.first_user_message_preview,
            agent_name: r.agent_name,
            team_name: r.team_name,
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
    tombstone_gate(&pool, &id, "session", "session").await?;
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

    let agent_setting = repo_observed::session_agent_setting(&pool, &id)
        .await
        .expect("db");

    Ok(Json(Envelope {
        meta: ResponseMeta::now(),
        data: SessionDetail {
            session_id: id,
            agent_setting,
            summary: SessionSummary {
                event_count,
                by_kind,
                first_observed_at: first_obs,
                last_observed_at: last_obs,
            },
        },
    }))
}

/// Dogfood 2026-06-12 — `deny_unknown_fields`: unknown query params are now a
/// 400 instead of being silently dropped. A silently-ignored `?kind=` filter
/// cost a dogfooding analysis a full-session download; misspelled params must
/// fail loudly.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventsQuery {
    pub before: Option<String>,
    pub after: Option<String>,
    /// Deep-link window: return the window containing this event (half the
    /// limit before it, half after). 404 when the event is not in the session.
    /// Takes precedence over `before`/`after` (same as the correlated params).
    pub around: Option<String>,
    pub limit: Option<i64>,
    /// Dogfood 2026-06-12 — kind filter (CSV of EventKind snake_case names).
    /// Cursor-paged like the unfiltered window; cannot be combined with
    /// `around` / `tool_use_id` / `request_id`.
    pub kind: Option<String>,
    /// §1.2 role axis — CSV of `user` / `assistant` / `system` (mapped to
    /// `user_message` / `assistant_message` / `system_summary` kinds).
    pub role: Option<String>,
    /// §1.2 origin axis — CSV of `origin_of` classifications (user_message only).
    pub origin: Option<String>,
    /// §1.2 error axis — `true` restricts to tool_result events with
    /// `tool_result.is_error == true`.
    pub error: Option<String>,
    /// §1.2 signal axis — `true` restricts to events referenced by this
    /// session's signal evidence_refs.
    pub signal: Option<String>,
    /// §1.2 verification axis — CSV of `passed` / `failed` / `unknown`,
    /// matched against each event's latest verification_run status (by
    /// trigger_event_id).
    pub verification: Option<String>,
    /// §1.2 tool axis — CSV of tool_name values (tool_call events).
    pub tool: Option<String>,
    /// §1.2 model axis — CSV of exact model name matches (assistant_message).
    pub model: Option<String>,
    /// tag axis (2026-07-05 사후 확장) — CSV of deterministic `verb.object`
    /// tool-call tags (`classify_tool_call`, same source as turn tag_histogram).
    pub tag: Option<String>,
    /// §1.2 free-text search — case-insensitive substring over message/tool
    /// input/result text.
    pub q: Option<String>,
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
    tombstone_gate(&pool, &id, "session", "session").await?;
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
    // §1.2 — 8축 필터 파싱. kind는 기존 dogfood 2026-06-12 검증을
    // `EventFilter::from_params`로 이관(계약 보존: 미지 kind → 400
    // INVALID_KIND, 그 외 축 오류 → INVALID_FILTER).
    use crate::insight::event_filter::{EventFilter, FilterCtx, RawFilterParams};
    let filter = EventFilter::from_params(&RawFilterParams {
        kind: q.kind.as_deref(),
        role: q.role.as_deref(),
        origin: q.origin.as_deref(),
        error: q.error.as_deref(),
        signal: q.signal.as_deref(),
        verification: q.verification.as_deref(),
        tool: q.tool.as_deref(),
        model: q.model.as_deref(),
        tag: q.tag.as_deref(),
        q: q.q.as_deref(),
    })
    .map_err(|detail| {
        let title = if detail.starts_with("unknown event kind:") {
            "INVALID_KIND"
        } else {
            "INVALID_FILTER"
        };
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"type":"about:blank","title": title,"detail": detail})),
        )
    })?;
    if filter.is_some() && (q.around.is_some() || q.tool_use_id.is_some() || q.request_id.is_some())
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "type": "about:blank",
                "title": "INVALID_PARAMS",
                "detail": "filter params cannot be combined with around / tool_use_id / request_id",
            })),
        ));
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
        let events: Vec<serde_json::Value> = evs.iter().map(observed_to_dto).collect();
        return Ok(Json(Envelope {
            meta: ResponseMeta::now(),
            data: SessionEventsResponse {
                events,
                prev_cursor: None,
                next_cursor: None,
                matched_count: None,
            },
        }));
    }

    let limit = q.limit.unwrap_or(500);

    let mut matched_count: Option<i64> = None;
    let evs = if let Some(around_id) = q.around.as_deref() {
        // Deep-link window centered on an event the client only knows by id
        // (it cannot build the `<observed_at>|<event_id>` cursor itself).
        match repo_observed::list_session_around(&pool, &id, around_id, limit)
            .await
            .expect("db")
        {
            Some(evs) => evs,
            None => {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(json!({
                        "type": "about:blank",
                        "title": "RESOURCE_NOT_FOUND",
                        "detail": format!("event {around_id} not found in session {id}"),
                    })),
                ))
            }
        }
    } else {
        let before = parse_cursor(q.before.as_deref())?;
        let after = parse_cursor(q.after.as_deref())?;
        if let Some(f) = &filter {
            let ctx = if f.needs_ctx() {
                FilterCtx {
                    signal_evidence: repo_signal::evidence_event_ids(&pool, &id)
                        .await
                        .expect("db"),
                    verification_by_trigger: repo_verification_run::status_by_trigger(&pool, &id)
                        .await
                        .expect("db"),
                }
            } else {
                FilterCtx::default()
            };
            let pred = |e: &crate::model::observed::ObservedEvent| f.matches(e, &ctx);
            // verification/signal 축은 매칭 가능한 이벤트가 ctx 집합의 키로
            // 한정된다 — event_id IN 푸시다운으로 풀스캔(50k 세션 수 초, 행별
            // payload 파싱)을 후보 수백 행 조회로 줄인다(2026-07-05 실사고).
            // 둘 다 활성이면 교집합. matches()에도 남아 있어 이중 안전.
            let candidate_ids: Option<Vec<String>> = if f.verifications.is_some() || f.signal {
                let mut ids: Vec<String> = match (f.verifications.is_some(), f.signal) {
                    (true, true) => ctx
                        .verification_by_trigger
                        .keys()
                        .filter(|k| ctx.signal_evidence.contains(*k))
                        .cloned()
                        .collect(),
                    (true, false) => ctx.verification_by_trigger.keys().cloned().collect(),
                    (false, true) => ctx.signal_evidence.iter().cloned().collect(),
                    (false, false) => unreachable!(),
                };
                ids.sort();
                Some(ids)
            } else {
                None
            };
            // kind·tool은 SQL 푸시다운, matches()에도 남아 있어 이중 안전.
            matched_count = Some(
                repo_observed::count_session_scan(
                    &pool,
                    &id,
                    f.kinds.as_deref(),
                    f.tools.as_deref(),
                    candidate_ids.as_deref(),
                    &pred,
                )
                .await
                .expect("db"),
            );
            repo_observed::list_session_window_scan(
                &pool,
                &id,
                f.kinds.as_deref(),
                f.tools.as_deref(),
                candidate_ids.as_deref(),
                &pred,
                before.as_ref(),
                after.as_ref(),
                limit,
            )
            .await
            .expect("db")
        } else {
            repo_observed::list_session_window(&pool, &id, before.as_ref(), after.as_ref(), limit)
                .await
                .expect("db")
        }
    };

    let (prev_cursor, next_cursor) = match (evs.first(), evs.last()) {
        (Some(first), Some(last)) => {
            let prev = format!("{}|{}", first.observed_at.to_rfc3339(), first.event_id);
            let next_of_last = || {
                Some(format!(
                    "{}|{}",
                    last.observed_at.to_rfc3339(),
                    last.event_id
                ))
            };
            let next = if filter.is_some() {
                // Filtered window: the session's overall last_observed_at is
                // the wrong tip (it is usually bulk telemetry). The tip rule
                // for the filtered stream: a newest-anchored window (no
                // cursors) is at the tip by construction; forward paging hit
                // the tip when it returned fewer rows than asked.
                let eff_limit = limit.clamp(1, 1000);
                let at_filtered_tip = (q.before.is_none() && q.after.is_none())
                    || (q.after.is_some() && (evs.len() as i64) < eff_limit);
                if at_filtered_tip {
                    None
                } else {
                    next_of_last()
                }
            } else {
                // next_cursor is null when the window already reaches the
                // session's live tip — further events arrive via SSE, not via
                // another page fetch. The summary's last_observed_at is the
                // authoritative tip.
                let summary = repo_observed::session_summary(&pool, &id)
                    .await
                    .expect("db");
                let at_live_tip = matches!(summary, Some((_, _, last_observed_at)) if last_observed_at == last.observed_at.to_rfc3339());
                if at_live_tip {
                    None
                } else {
                    next_of_last()
                }
            };
            (Some(prev), next)
        }
        _ => (None, None),
    };

    let events: Vec<serde_json::Value> = evs.iter().map(observed_to_dto).collect();
    // Slice-18: aggregate redaction summary for this session's raw events.
    let summary = repo_raw::aggregate_session_summary(&pool, &id).await.ok();
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
            matched_count,
        },
    }))
}

/// Dogfood 2026-06-12 (§3-2) — turn-level deterministic rollup. Counts and
/// redacted excerpts only; the "is this a rework loop?" judgment belongs to
/// the LLM consumer (deterministic-measurement / LLM-judgment split).
pub async fn session_turns(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> Result<
    Json<Envelope<crate::insight::turn_rollup::TurnRollupResponse>>,
    (StatusCode, Json<serde_json::Value>),
> {
    tombstone_gate(&pool, &id, "session", "session").await?;
    let evs = repo_observed::list_session_conversation(&pool, &id)
        .await
        .expect("db");
    let mut rollup = crate::insight::turn_rollup::rollup(&id, &evs);
    // S8 — attach per-turn token sums (usage_facet ⋈ observed_event on turn_id)
    // so the KPI strip can draw intra-session sparklines.
    let by_turn = repo_usage_facet::tokens_by_turn(&pool, &id)
        .await
        .unwrap_or_default();
    for t in rollup.turns.iter_mut() {
        if let Some(&(input, cc, cr, output)) = by_turn.get(&t.turn_id) {
            t.tokens = Some(crate::insight::turn_rollup::TurnTokens {
                input_tokens: input,
                cache_creation_input_tokens: cc,
                cache_read_input_tokens: cr,
                output_tokens: output,
            });
        }
    }
    Ok(Json(Envelope {
        meta: ResponseMeta::now(),
        data: rollup,
    }))
}

pub async fn session_diff_hunks(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> Result<Json<Envelope<DiffHunksResponse>>, (StatusCode, Json<serde_json::Value>)> {
    tombstone_gate(&pool, &id, "session", "session").await?;
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

    // The retention sweep scrubs the payload but keeps the skeleton row, so
    // the join above still resolves — the tombstone says the content expired.
    tombstone_gate(
        &pool,
        &row.raw_event_id,
        "raw_event",
        "raw payload of event",
    )
    .await?;

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
            redaction_state: row.redaction_state,
            telemetry,
        },
    }))
}

/// insight-redesign #1 — `GET /v1/sessions/:id/usage`
///
/// Returns the session token-usage aggregate: assistant_events (usage_facet row
/// count), user_turns (distinct turn_id), raw token counts, billed_tokens
/// (input + cache_creation + output; cache_read is NOT billed), and a
/// per-model breakdown. F1: `turns` renamed to `assistant_events`, `user_turns`
/// added, `cache_hit_ratio` removed (consumers compute from token components).
pub async fn session_usage(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(gone) = tombstone_gate(&pool, &id, "session", "session").await {
        return gone.into_response();
    }
    let agg = repo_usage_facet::session_aggregate(&pool, &id)
        .await
        .expect("db");
    let user_turns = repo_observed::count_distinct_turns(&pool, &id)
        .await
        .expect("db");
    let billed = agg.input_tokens + agg.cache_creation_input_tokens + agg.output_tokens;
    let cost = crate::insight::pricing::estimate_session_cost(&agg.by_model);
    let priced: std::collections::HashMap<&str, f64> = cost
        .per_model
        .iter()
        .map(|c| (c.model.as_str(), c.estimated_cost_usd))
        .collect();
    let data = SessionUsageDto {
        session_id: id,
        assistant_events: agg.assistant_events,
        user_turns,
        input_tokens: agg.input_tokens,
        cache_creation_input_tokens: agg.cache_creation_input_tokens,
        cache_read_input_tokens: agg.cache_read_input_tokens,
        output_tokens: agg.output_tokens,
        billed_tokens: billed,
        estimated_cost_usd: cost.total_usd,
        cost_basis: crate::insight::pricing::COST_BASIS_ESTIMATE.to_string(),
        pricing_version: crate::insight::pricing::pricing_version().to_string(),
        models_without_pricing: cost.models_without_pricing.clone(),
        by_model: agg
            .by_model
            .into_iter()
            .map(|m| {
                let est = priced.get(m.model.as_str()).copied().unwrap_or(0.0);
                let rates = crate::insight::pricing::rates_for(&m.model).map(|r| ModelRatesDto {
                    input_per_mtok: r.input_per_mtok,
                    cache_creation_per_mtok: r.cache_creation_per_mtok,
                    cache_read_per_mtok: r.cache_read_per_mtok,
                    output_per_mtok: r.output_per_mtok,
                });
                let is_priced = rates.is_some();
                ModelUsageDto {
                    model: m.model,
                    assistant_events: m.assistant_events,
                    input_tokens: m.input_tokens,
                    cache_creation_input_tokens: m.cache_creation_input_tokens,
                    cache_read_input_tokens: m.cache_read_input_tokens,
                    output_tokens: m.output_tokens,
                    estimated_cost_usd: est,
                    priced: is_priced,
                    rates,
                }
            })
            .collect(),
    };
    Json(Envelope {
        meta: ResponseMeta::now(),
        data,
    })
    .into_response()
}

#[derive(Deserialize)]
pub struct BaselineQuery {
    pub session_id: Option<String>,
}

/// PR-3 §3a — 세션 횡단 baseline은 series와 같은 후보 cap을 쓴다(§G-3 준용).
const BASELINE_CANDIDATE_CAP: i64 = 5000;

/// insight-redesign #6 + PR-3 §3a — `GET /v1/usage/baseline`
///
/// Cross-session baseline: median (+ p25/p75 + sample n) of each key metric.
/// `?session_id=`가 오면 그 세션의 `session_summary.project`로 분포를 스코프하고
/// scope="project"·project를 실어 프론트가 라벨을 정직하게 붙이게 한다. project
/// 미상이면 store 전체로 폴백(scope="store", 기존 동작 보존). SQLite has no
/// MEDIAN(), so the per-session metric values are pulled and quantiles computed
/// in Rust. usage 4지표(cache_hit·billed·assistant_events·output)는 usage_facet
/// 파생이고, 신규 3지표(pass_rate·tool_failure·cost)는 `compute_session_metrics`
/// 파생이다 — 각 지표의 게이트(분모)가 달라 `n`(표본 수)도 지표별로 다르다.
pub async fn usage_baseline(
    State(pool): State<SqlitePool>,
    Query(q): Query<BaselineQuery>,
) -> impl IntoResponse {
    // 1) 스코프 해석: session_id → session_summary.project. 미상이면 store.
    let project = match q.session_id.as_deref() {
        Some(sid) => repo_observed::session_project(&pool, sid)
            .await
            .expect("db"),
        None => None,
    };
    let rows =
        repo_observed::list_sessions_filtered(&pool, BASELINE_CANDIDATE_CAP, project.as_deref())
            .await
            .expect("db");
    let scope_ids: std::collections::HashSet<String> =
        rows.iter().map(|r| r.session_id.clone()).collect();

    // 2) usage 4지표 — 기존 per_session_metrics를 스코프로 필터(계산식 불변).
    //    store 스코프(project=None)에서는 필터를 적용하지 않아 기존 동작을
    //    보존한다(usage-only 세션은 관측 이벤트가 없어 scope_ids에 없을 수 있음).
    let metrics: Vec<_> = repo_usage_facet::per_session_metrics(&pool)
        .await
        .expect("db")
        .into_iter()
        .filter(|m| project.is_none() || scope_ids.contains(&m.session_id))
        .collect();
    let session_count = metrics.len() as i64;
    let cache_hit_vals: Vec<f64> = metrics.iter().filter_map(|m| m.cache_hit_ratio).collect();
    let billed_vals: Vec<f64> = metrics.iter().map(|m| m.billed_tokens as f64).collect();
    let assistant_events_vals: Vec<f64> =
        metrics.iter().map(|m| m.assistant_events as f64).collect();
    let output_vals: Vec<f64> = metrics.iter().map(|m| m.output_tokens as f64).collect();

    // 3) 신규 3지표 — 스코프 세션별 compute_session_metrics(인메모리 캐시 편승,
    //    series /v1/metrics와 같은 인터랙티브 경로 선례). 스코프의 관측-이벤트
    //    세션(rows)과 usage 세션(metrics)의 합집합을 돈다 — usage-only 세션도
    //    billed>0이면 비용 표본에 기여하기 때문(검증/도구는 게이트로 걸러진다).
    let loop_ids: std::collections::BTreeSet<String> = rows
        .iter()
        .map(|r| r.session_id.clone())
        .chain(metrics.iter().map(|m| m.session_id.clone()))
        .collect();
    let mut pass_rate_vals: Vec<f64> = Vec::new();
    let mut tool_failure_vals: Vec<f64> = Vec::new();
    let mut cost_vals: Vec<f64> = Vec::new();
    for sid in &loop_ids {
        let m = crate::insight::metrics::compute_session_metrics(&pool, sid)
            .await
            .expect("db");
        let measured = m.verification_passed + m.verification_failed;
        if measured > 0 {
            pass_rate_vals.push(m.verification_passed as f64 / measured as f64);
        }
        if m.tool_call_total > 0 {
            tool_failure_vals.push(m.tool_failure_count as f64);
        }
        let billed = m.input_tokens + m.cache_creation_input_tokens + m.output_tokens;
        if billed > 0 {
            cost_vals.push(m.estimated_cost_usd);
        }
    }

    fn stat(values: &[f64]) -> BaselineStat {
        match repo_usage_facet::median_p25_p75(values) {
            Some(q) => BaselineStat {
                p25: Some(q.p25),
                median: Some(q.median),
                p75: Some(q.p75),
                n: values.len() as i64,
            },
            None => BaselineStat {
                p25: None,
                median: None,
                p75: None,
                n: 0,
            },
        }
    }

    let data = UsageBaselineDto {
        session_count,
        scope: if project.is_some() {
            "project".into()
        } else {
            "store".into()
        },
        project,
        cache_hit_ratio: stat(&cache_hit_vals),
        billed_tokens: stat(&billed_vals),
        assistant_events: stat(&assistant_events_vals),
        output_tokens: stat(&output_vals),
        verification_pass_rate: stat(&pass_rate_vals),
        tool_failure_count: stat(&tool_failure_vals),
        estimated_cost_usd: stat(&cost_vals),
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
    if let Err(gone) = tombstone_gate(&pool, &id, "session", "session").await {
        return gone.into_response();
    }
    let runs = repo_verification_run::list_session(&pool, &id)
        .await
        .expect("db");
    let hunks = repo_diff_hunk::list_session(&pool, &id).await.expect("db");

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
    .into_response()
}

/// Slice-11 — `GET /v1/verification-runs/:id`
///
/// Single verification run detail by ID. Includes `covered_diff_hunk_ids`.
pub async fn verification_run_detail(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> Result<Json<Envelope<VerificationRunDto>>, (StatusCode, Json<serde_json::Value>)> {
    tombstone_gate(&pool, &id, "verification_run", "verification_run").await?;
    let row = repo_verification_run::get(&pool, &id).await.expect("db");
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
        .filter(|_| run_started.is_some()) // only when we have a parseable timestamp
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
        status_provenance: r.status_provenance,
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
pub(crate) fn observed_to_dto(e: &crate::model::observed::ObservedEvent) -> serde_json::Value {
    let telemetry = e
        .telemetry
        .as_ref()
        .map(|t| serde_json::to_value(t).unwrap_or(serde_json::Value::Null));
    // tool_call만 태그 분류(loop-foundations 2026-06-12) — UI와 MCP 소비자가
    // 같은 어휘를 본다. 그 외 kind는 null.
    let tag = if e.kind == crate::model::observed::EventKind::ToolCall {
        serde_json::to_value(crate::insight::event_tags::classify_tool_call(
            e.tool_name.as_deref(),
            &e.payload,
        ))
        .unwrap_or(serde_json::Value::Null)
    } else {
        serde_json::Value::Null
    };
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
        "agent_id": e.agent_id,
        "workflow_run_id": e.workflow_run_id,
        "agent_name": e.agent_name,
        "team_name": e.team_name,
        "is_meta": e.is_meta,
        "trace_id": e.trace_id,
        "span_id": e.span_id,
        "parent_span_id": e.parent_span_id,
        "latency_ms": e.latency_ms,
        "telemetry": telemetry,
        "tag": tag,
        "payload": e.payload,
    })
}

// ---------------------------------------------------------------------------
// Plan 3a — Behavioral metrics endpoint
// ---------------------------------------------------------------------------

/// `GET /v1/sessions/:id/metrics` — on-demand deterministic behavioral metrics.
///
/// Aggregates events, signals, verification_runs and usage_facet for the
/// session into counts/ratios. No severity or judgment fields (spec §6.3).
pub async fn session_metrics(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(gone) = tombstone_gate(&pool, &id, "session", "session").await {
        return gone.into_response();
    }
    match crate::insight::metrics::compute_session_metrics(&pool, &id).await {
        Ok(m) => Json(SessionMetricsResponse { data: m }).into_response(),
        Err(err) => {
            tracing::error!(session_id = %id, err = %err, "session_metrics failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal server error"})),
            )
                .into_response()
        }
    }
}

/// `GET /v1/metrics` — 세션 횡단 metrics+fingerprint series (on-demand).
///
/// 개입(하네스/프롬프트 변경) 전후 비교의 측정면. 미지원 쿼리 파라미터는
/// 400(deny_unknown_fields — dogfood 2026-06-12 계약과 동일), `from`/`to`는
/// RFC3339가 아니면 400 INVALID_TIME.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricsSeriesQuery {
    pub project: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<i64>,
}

pub async fn metrics_series(
    State(pool): State<SqlitePool>,
    Query(q): Query<MetricsSeriesQuery>,
) -> impl IntoResponse {
    fn parse_time(
        s: Option<&str>,
        name: &str,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>, (StatusCode, Json<serde_json::Value>)> {
        match s {
            None => Ok(None),
            Some(v) => chrono::DateTime::parse_from_rfc3339(v)
                .map(|d| Some(d.with_timezone(&chrono::Utc)))
                .map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(json!({
                            "type": "about:blank",
                            "title": "INVALID_TIME",
                            "detail": format!("{name} must be RFC3339"),
                        })),
                    )
                }),
        }
    }
    let from = match parse_time(q.from.as_deref(), "from") {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    let to = match parse_time(q.to.as_deref(), "to") {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    let project_norm = q
        .project
        .as_deref()
        .map(|p| p.trim_end_matches('/'))
        .filter(|p| !p.is_empty())
        .map(String::from);
    let limit = q.limit.unwrap_or(crate::insight::series::DEFAULT_LIMIT);
    match crate::insight::series::collect(&pool, project_norm.as_deref(), from, to, limit).await {
        Ok(series) => Json(Envelope {
            meta: ResponseMeta::now(),
            data: series,
        })
        .into_response(),
        Err(err) => {
            tracing::error!(err = %err, "metrics_series failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal server error"})),
            )
                .into_response()
        }
    }
}

#[derive(serde::Deserialize)]
pub struct VerificationSummaryQuery {
    pub project: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    /// §3c — 단일 세션 스코프. project/from/to와 결합 불가(400).
    pub session_id: Option<String>,
}

/// 2026-07-04 대시보드 검증 탭 — `GET /v1/verification/summary`.
/// kind×결과·측정률(판정불가 원인 분해)·실패 복구/방치·실행 리듬·변경
/// 커버리지의 창 단위 집계. 파생 정의의 SSOT는 insight::verification_summary.
pub async fn verification_summary(
    State(pool): State<SqlitePool>,
    Query(q): Query<VerificationSummaryQuery>,
) -> impl IntoResponse {
    if let Some(sid) = q.session_id.as_deref() {
        if q.project.is_some() || q.from.is_some() || q.to.is_some() {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "type": "about:blank",
                    "title": "INVALID_QUERY",
                    "detail": "session_id cannot be combined with project/from/to",
                })),
            )
                .into_response();
        }
        return match crate::insight::verification_summary::collect_session(&pool, sid).await {
            Ok(summary) => Json(Envelope {
                meta: ResponseMeta::now(),
                data: summary,
            })
            .into_response(),
            Err(err) => {
                tracing::error!(err = %err, "verification_summary failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "internal server error"})),
                )
                    .into_response()
            }
        };
    }
    fn parse_time(
        s: Option<&str>,
        name: &str,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>, (StatusCode, Json<serde_json::Value>)> {
        match s {
            None => Ok(None),
            Some(v) => chrono::DateTime::parse_from_rfc3339(v)
                .map(|d| Some(d.with_timezone(&chrono::Utc)))
                .map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(json!({
                            "type": "about:blank",
                            "title": "INVALID_TIME",
                            "detail": format!("{name} must be RFC3339"),
                        })),
                    )
                }),
        }
    }
    let from = match parse_time(q.from.as_deref(), "from") {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    let to = match parse_time(q.to.as_deref(), "to") {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    let project_norm = q
        .project
        .as_deref()
        .map(|p| p.trim_end_matches('/'))
        .filter(|p| !p.is_empty())
        .map(String::from);
    match crate::insight::verification_summary::collect(&pool, project_norm.as_deref(), from, to)
        .await
    {
        Ok(summary) => Json(Envelope {
            meta: ResponseMeta::now(),
            data: summary,
        })
        .into_response(),
        Err(err) => {
            tracing::error!(err = %err, "verification_summary failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal server error"})),
            )
                .into_response()
        }
    }
}

/// 4차 개정 — `GET /v1/instructions/:sha` 스냅샷 내용(경계 diff의 원료).
pub async fn instruction_snapshot(
    State(pool): State<SqlitePool>,
    Path(sha): Path<String>,
) -> impl IntoResponse {
    let row = sqlx::query(
        "SELECT content, first_observed_at FROM instruction_snapshot WHERE content_sha256 = ?",
    )
    .bind(&sha)
    .fetch_optional(&pool)
    .await
    .expect("db");
    match row {
        Some(r) => {
            use sqlx::Row as _;
            Json(Envelope {
                meta: ResponseMeta::now(),
                data: json!({
                    "content_sha256": sha,
                    "content": r.get::<String, _>("content"),
                    "first_observed_at": r.get::<String, _>("first_observed_at"),
                }),
            })
            .into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "type": "about:blank",
                "title": "RESOURCE_NOT_FOUND",
                "detail": format!("instruction snapshot {sha} not found"),
            })),
        )
            .into_response(),
    }
}

/// 4차 개정 — `GET /v1/sessions/:id/instructions` 세션의 지시문 관측 목록
/// (시간순 — 세션 중 변경의 서사가 그대로 드러난다).
pub async fn session_instructions(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    use sqlx::Row as _;
    let rows = sqlx::query(
        "SELECT source, path, content_sha256, observed_at FROM instruction_observation
         WHERE session_id = ? ORDER BY observed_at, source",
    )
    .bind(&id)
    .fetch_all(&pool)
    .await
    .expect("db");
    let data: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "source": r.get::<String, _>("source"),
                "path": r.get::<String, _>("path"),
                "content_sha256": r.get::<String, _>("content_sha256"),
                "observed_at": r.get::<String, _>("observed_at"),
            })
        })
        .collect();
    Json(Envelope {
        meta: ResponseMeta::now(),
        data,
    })
    .into_response()
}

/// `GET /v1/sessions/:id/fingerprint` — 세션 환경 fingerprint (on-demand).
///
/// 자기개선 루프의 독립변수 표면: 이 세션이 어떤 모델·CC 버전·branch·
/// instruction(CLAUDE.md 해시) 아래에서 돌았는가. 관측 값만 — 판단 필드 없음.
pub async fn session_fingerprint(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(gone) = tombstone_gate(&pool, &id, "session", "session").await {
        return gone.into_response();
    }
    match crate::insight::fingerprint::compute_session_fingerprint(&pool, &id).await {
        Ok(f) => Json(Envelope {
            meta: ResponseMeta::now(),
            data: f,
        })
        .into_response(),
        Err(err) => {
            tracing::error!(session_id = %id, err = %err, "session_fingerprint failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal server error"})),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Plan 1 — Signal endpoints (replaces the slice-14 finding endpoints)
// ---------------------------------------------------------------------------

/// Convert a `SignalRow` to the API DTO shape. JSON columns (`evidence_refs`,
/// `facts`, `provenance`) are parsed so callers receive typed values.
/// `pub(crate)`: MCP `get_session_signals`가 같은 변환을 공유한다 (parity 2026-07-03).
pub(crate) fn signal_row_to_dto(row: repo_signal::SignalRow) -> SignalDto {
    let evidence_refs: Vec<serde_json::Value> =
        serde_json::from_str(&row.evidence_refs).unwrap_or_default();
    let facts: serde_json::Value =
        serde_json::from_str(&row.facts).unwrap_or(serde_json::Value::Null);
    let provenance: serde_json::Value =
        serde_json::from_str(&row.provenance).unwrap_or(serde_json::Value::Null);
    SignalDto {
        signal_id: row.signal_id,
        schema_version: row.schema_version,
        session_id: row.session_id,
        detector: row.detector,
        subkind: row.subkind,
        summary: row.summary,
        evidence_refs,
        facts,
        provenance,
        created_at: row.created_at,
    }
}

/// `GET /v1/sessions/:id/signals` — list all signals for a session.
pub async fn session_signals(
    State(pool): State<SqlitePool>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    if let Err(gone) = tombstone_gate(&pool, &session_id, "session", "session").await {
        return gone.into_response();
    }
    match repo_signal::list_by_session(&pool, &session_id).await {
        Ok(rows) => {
            let data: Vec<SignalDto> = rows.into_iter().map(signal_row_to_dto).collect();
            Json(SignalsResponse { data }).into_response()
        }
        Err(err) => {
            tracing::error!(err = %err, "session_signals failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal server error"})),
            )
                .into_response()
        }
    }
}

/// `GET /v1/signals/:id` — single signal detail.
///
/// If the signal has been deleted by retention sweep, the tombstone table will
/// have a record for it and this handler returns `410 Gone` instead of `404 Not
/// Found`, so clients can distinguish "never existed" from "expired".
pub async fn signal_detail(
    State(pool): State<SqlitePool>,
    Path(signal_id): Path<String>,
) -> impl IntoResponse {
    // Check tombstone first.
    match repo_retention::is_tombstoned(&pool, &signal_id, "signal").await {
        Ok(true) => {
            return (
                StatusCode::GONE,
                Json(json!({
                    "type": "about:blank",
                    "title": "RESOURCE_GONE",
                    "detail": format!("signal {signal_id} was deleted by retention sweep"),
                    "resource_id": signal_id
                })),
            )
                .into_response();
        }
        Ok(false) => {}
        Err(err) => {
            tracing::warn!(signal_id = %signal_id, err = %err, "tombstone check failed");
        }
    }

    match repo_signal::get(&pool, &signal_id).await {
        Ok(Some(row)) => Json(json!({ "data": signal_row_to_dto(row) })).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "signal not found", "signal_id": signal_id})),
        )
            .into_response(),
        Err(err) => {
            tracing::error!(signal_id = %signal_id, err = %err, "signal_detail failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal server error"})),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Plan 4 — Detector catalog endpoint
// ---------------------------------------------------------------------------

/// `GET /v1/detectors` — LLM-readable detector manifest catalog.
///
/// Returns the manifest for every registered detector: id, intent, inputs,
/// rule, output, config_keys, rationale. Read-only; no DB access.
/// Spec §6.4: manifest=declaration, config=rule pack, predicate=code.
pub async fn list_detectors() -> impl IntoResponse {
    let catalog: Vec<_> = crate::insight::pipeline::all_detectors()
        .iter()
        .map(|d| d.manifest())
        .collect();
    Json(json!({ "data": catalog }))
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

// ── B-4 (2026-07-04): POST /v1/export-bundles — owner-only local export ────

/// read-only 원칙의 유일한 write 예외(PRD·05 §06). 데이터가 로컬을 떠나는
/// 경로가 아니라 owner가 반출을 선택하는 행위다: 번들은
/// `$WIMCC_CONFIG_DIR/exports/`(token 컨벤션 재사용)에 로컬 파일로 쓰이고
/// sha256이 응답에 실린다. 계약(§09-3, 2026-07-04 사용자 결정): 기본은
/// redacted normalized evidence(observed — ingest 시점 redaction 게이트
/// 통과분)만, raw record는 explicit opt-in. 05 §06 "must not include"
/// (patch·hook 스크립트·설정 변경 류)는 내용이 데이터 전용이라 구조적으로
/// 만족한다. kind는 우선 "session"만 — signal/lineage 반출은 세션 번들이
/// 상위집합이라 필요가 관측되면 추가한다.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportBundleRequest {
    pub kind: String,
    pub id: String,
    #[serde(default)]
    pub include_raw: bool,
}

fn exports_dir() -> std::path::PathBuf {
    let base = if let Ok(v) = std::env::var("WIMCC_CONFIG_DIR") {
        std::path::PathBuf::from(v)
    } else {
        dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("wimcc")
    };
    base.join("exports")
}

pub async fn create_export_bundle(
    State(pool): State<SqlitePool>,
    Json(req): Json<ExportBundleRequest>,
) -> Result<Json<Envelope<serde_json::Value>>, (StatusCode, Json<serde_json::Value>)> {
    if req.kind != "session" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "type": "about:blank",
                "title": "INVALID_KIND",
                "detail": format!("unsupported bundle kind: {} (only \"session\")", req.kind),
            })),
        ));
    }
    let session_id = req.id.as_str();
    let Some((event_count, first_observed_at, last_observed_at)) =
        repo_observed::session_summary(&pool, session_id)
            .await
            .expect("db")
    else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({
                "type": "about:blank",
                "title": "RESOURCE_NOT_FOUND",
                "detail": format!("session {session_id} not found"),
            })),
        ));
    };

    let by_kind = repo_observed::session_kind_counts(&pool, session_id)
        .await
        .expect("db");
    let fingerprint = crate::insight::fingerprint::compute_session_fingerprint(&pool, session_id)
        .await
        .expect("fingerprint");
    let metrics = crate::insight::metrics::compute_session_metrics(&pool, session_id)
        .await
        .expect("metrics");
    let signals: Vec<crate::api::dto::SignalDto> =
        crate::db::repo_signal::list_by_session(&pool, session_id)
            .await
            .expect("db")
            .into_iter()
            .map(signal_row_to_dto)
            .collect();
    let runs = repo_verification_run::list_session(&pool, session_id)
        .await
        .expect("db");
    let hunks = repo_diff_hunk::list_session(&pool, session_id)
        .await
        .expect("db");
    let verification_runs: Vec<VerificationRunDto> = runs
        .into_iter()
        .map(|r| {
            let covered = covered_hunk_ids_for_run(&r, &hunks);
            run_to_dto(r, covered)
        })
        .collect();
    let events: Vec<serde_json::Value> = repo_observed::list_session(&pool, session_id, 100_000)
        .await
        .expect("db")
        .iter()
        .map(observed_to_dto)
        .collect();

    let mut bundle = json!({
        "meta": {
            "schema_version": SCHEMA_VERSION,
            "collection_profile": crate::model::meta::COLLECTION_PROFILE,
            "redaction_policy": crate::model::meta::RedactionPolicy::standard(),
            "include_raw": req.include_raw,
            "generated_at": chrono::Utc::now().to_rfc3339(),
        },
        "session": {
            "session_id": session_id,
            "summary": {
                "event_count": event_count,
                "first_observed_at": first_observed_at,
                "last_observed_at": last_observed_at,
                "by_kind": by_kind,
            },
            "fingerprint": fingerprint,
            "metrics": metrics,
        },
        "signals": signals,
        "verification_runs": verification_runs,
        "events": events,
    });
    if req.include_raw {
        let raw = crate::db::repo_raw::list_session_records(&pool, session_id)
            .await
            .expect("db");
        bundle["raw_events"] = json!(raw);
    }

    let dir = exports_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("cannot create exports dir: {e}")})),
        ));
    }
    let short: String = session_id.chars().take(8).collect();
    let fname = format!(
        "wimcc-bundle-{short}-{}.json",
        chrono::Utc::now().format("%Y%m%dT%H%M%S%fZ")
    );
    let path = dir.join(fname);
    let bytes = serde_json::to_vec_pretty(&bundle).unwrap_or_default();
    if let Err(e) = std::fs::write(&path, &bytes) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("cannot write bundle: {e}")})),
        ));
    }
    let sha256 = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(&bytes);
        format!("{:x}", h.finalize())
    };

    // owner action 가시성 — audit 원장에 남긴다(§06 traceability).
    let audit_payload = json!({
        "session_id": session_id,
        "include_raw": req.include_raw,
        "bundle_path": path.to_string_lossy(),
        "sha256": sha256,
        "byte_size": bytes.len(),
    });
    let _ = crate::db::repo_audit::insert(
        &pool,
        &ulid::Ulid::new().to_string(),
        "export_bundle_created",
        Some("owner"),
        &audit_payload.to_string(),
    )
    .await;

    Ok(Json(Envelope {
        meta: ResponseMeta::now(),
        data: json!({
            "bundle_path": path.to_string_lossy(),
            "sha256": sha256,
            "byte_size": bytes.len(),
            "resource": {"kind": "session", "id": session_id},
            "include_raw": req.include_raw,
            "event_count": events.len(),
            "signal_count": bundle["signals"].as_array().map(|a| a.len()).unwrap_or(0),
            "created_at": chrono::Utc::now().to_rfc3339(),
        }),
    }))
}
