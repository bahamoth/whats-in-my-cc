use sqlx::{Row, SqlitePool};

use crate::error::Result;
use crate::model::cursor::Cursor;
use crate::model::observed::{Actor, EventKind, ObservedEvent, TelemetryFacet};

pub async fn insert(pool: &SqlitePool, e: &ObservedEvent) -> Result<()> {
    insert_inner(pool, e, false).await.map(|_| ())
}

/// slice-6 — insert that skips on PK conflict. Returns true when a new row was
/// added, false when `event_id` already existed. Used by reparse-friendly
/// ingesters (otel metrics / logs) where the same data point may be normalised
/// more than once across Stage 1 → Stage 2 transitions.
pub async fn insert_or_ignore(pool: &SqlitePool, e: &ObservedEvent) -> Result<bool> {
    insert_inner(pool, e, true).await
}

async fn insert_inner(pool: &SqlitePool, e: &ObservedEvent, ignore: bool) -> Result<bool> {
    let sql = if ignore {
        "INSERT OR IGNORE INTO observed_event(
            event_id, raw_event_id, schema_version, session_id, event_uuid, parent_uuid,
            observed_at, actor, kind, subkind, tool_use_id, tool_name, request_id,
            message_id, turn_id, source_tool_assistant_uuid, source_tool_use_id,
            is_sidechain, agent_id, workflow_run_id, agent_name, team_name, is_meta, cwd, git_branch, user_type, entrypoint, cc_version,
            trace_id, span_id, parent_span_id, latency_ms,
            payload, parser_version)
         VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
    } else {
        "INSERT INTO observed_event(
            event_id, raw_event_id, schema_version, session_id, event_uuid, parent_uuid,
            observed_at, actor, kind, subkind, tool_use_id, tool_name, request_id,
            message_id, turn_id, source_tool_assistant_uuid, source_tool_use_id,
            is_sidechain, agent_id, workflow_run_id, agent_name, team_name, is_meta, cwd, git_branch, user_type, entrypoint, cc_version,
            trace_id, span_id, parent_span_id, latency_ms,
            payload, parser_version)
         VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
    };
    let res = sqlx::query(sql)
        .bind(&e.event_id)
        .bind(&e.raw_event_id)
        .bind(&e.schema_version)
        .bind(&e.session_id)
        .bind(&e.event_uuid)
        .bind(&e.parent_uuid)
        .bind(e.observed_at.to_rfc3339())
        .bind(e.actor.as_str())
        .bind(e.kind.as_str())
        .bind(&e.subkind)
        .bind(&e.tool_use_id)
        .bind(&e.tool_name)
        .bind(&e.request_id)
        .bind(&e.message_id)
        .bind(&e.turn_id)
        .bind(&e.source_tool_assistant_uuid)
        .bind(&e.source_tool_use_id)
        .bind(e.is_sidechain as i64)
        .bind(&e.agent_id)
        .bind(&e.workflow_run_id)
        .bind(&e.agent_name)
        .bind(&e.team_name)
        .bind(e.is_meta as i64)
        .bind(&e.cwd)
        .bind(&e.git_branch)
        .bind(&e.user_type)
        .bind(&e.entrypoint)
        .bind(&e.cc_version)
        .bind(&e.trace_id)
        .bind(&e.span_id)
        .bind(&e.parent_span_id)
        .bind(e.latency_ms)
        .bind(merge_payload_with_telemetry(&e.payload, e.telemetry.as_ref()).to_string())
        .bind(&e.parser_version)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

/// Backfill `agent_id` on existing rows from the raw transcript payload's
/// top-level `agentId` (present on subagent jsonl records). Idempotent — only
/// touches rows where `agent_id IS NULL`. Lets an already-ingested DB gain
/// subagent attribution on serve startup without a full init-db / re-ingest.
/// agentId is a hex id untouched by redaction, so the masked payload still has it.
pub async fn backfill_agent_id(pool: &SqlitePool) -> Result<u64> {
    let res = sqlx::query(
        "UPDATE observed_event
         SET agent_id = json_extract(
             CAST(
                 (SELECT r.payload FROM raw_event r WHERE r.raw_event_id = observed_event.raw_event_id)
                 AS TEXT
             ),
             '$.agentId'
         )
         WHERE agent_id IS NULL
           AND (
             SELECT CASE
                 WHEN json_valid(CAST(r.payload AS TEXT))
                 THEN json_extract(CAST(r.payload AS TEXT), '$.agentId') IS NOT NULL
                 ELSE 0
             END
             FROM raw_event r WHERE r.raw_event_id = observed_event.raw_event_id
           ) = 1",
    )
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

/// Backfill `workflow_run_id` on existing rows from the raw transcript file path.
/// Workflow-tool subagents live under `…/subagents/workflows/<runId>/agent-*`, and
/// the runId is the deterministic workflow group key — present only in the file
/// path (`raw_event.source_uri`), never in the record. Idempotent (only NULL rows).
/// Lets an already-ingested DB gain workflow grouping on serve startup without a
/// full init-db / re-ingest.
pub async fn backfill_workflow_run_id(pool: &SqlitePool) -> Result<u64> {
    let res = sqlx::query(
        "UPDATE observed_event
         SET workflow_run_id = (
             SELECT substr(tail, 1, instr(tail, '/') - 1) FROM (
                 SELECT substr(
                     r.source_uri,
                     instr(r.source_uri, '/subagents/workflows/') + length('/subagents/workflows/')
                 ) AS tail
                 FROM raw_event r WHERE r.raw_event_id = observed_event.raw_event_id
             )
         )
         WHERE workflow_run_id IS NULL
           AND raw_event_id IN (
             SELECT raw_event_id FROM raw_event WHERE source_uri LIKE '%/subagents/workflows/%'
           )",
    )
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

fn merge_payload_with_telemetry(
    payload: &serde_json::Value,
    telemetry: Option<&TelemetryFacet>,
) -> serde_json::Value {
    let mut out = if payload.is_object() {
        payload.clone()
    } else {
        serde_json::json!({ "value": payload })
    };
    if let Some(t) = telemetry {
        if let serde_json::Value::Object(map) = &mut out {
            map.insert(
                "telemetry".into(),
                serde_json::to_value(t).unwrap_or(serde_json::Value::Null),
            );
        }
    }
    out
}

pub struct SessionRow {
    pub session_id: String,
    pub first_observed_at: String,
    pub last_observed_at: String,
    pub event_count: i64,
    /// slice-7 — per-source row counts so the WebUI can show transcript-only
    /// vs OTel-only sessions at a glance without a second round trip.
    pub by_kind: std::collections::BTreeMap<String, i64>,
    /// S6 (UX 재설계) — identifiability facets. All optional: a session may
    /// lack a hook-collected `slug`, a text assistant turn (→ no `model`), a
    /// `cwd`, or a non-empty first user message.
    /// `project` = the session's most recent non-empty `cwd`.
    pub project: Option<String>,
    /// Dominant assistant model (`message.model`); ties broken by name.
    pub model: Option<String>,
    /// Stable per-session slug from the transcript's `system` summary payload.
    pub slug: Option<String>,
    /// First non-empty user-message content, truncated server-side.
    pub first_user_message_preview: Option<String>,
    /// Teammate 세션 식별 (2026-07-03) — named Agent 스폰의 envelope 필드.
    /// 팀메이트가 아닌 세션은 None.
    pub agent_name: Option<String>,
    pub team_name: Option<String>,
}

pub async fn list_sessions(pool: &SqlitePool, limit: i64) -> Result<Vec<SessionRow>> {
    list_sessions_filtered(pool, limit, None).await
}

/// Dogfood 2026-06-12 (§3-3) — optional project filter: only sessions having
/// ≥1 event whose `cwd` equals the given (already slash-normalised) path.
/// `cwd` comes from the transcript's per-event `cwd` field, so sessions that
/// `cd` around still match on the project root they ran any event in.
pub async fn list_sessions_filtered(
    pool: &SqlitePool,
    limit: i64,
    project: Option<&str>,
) -> Result<Vec<SessionRow>> {
    use sqlx::Row as _Row;
    // First pass: per-session totals + ordering. Limit applies here.
    let totals = match project {
        None => {
            sqlx::query(
                "SELECT session_id,
                        MIN(observed_at) AS first_observed_at,
                        MAX(observed_at) AS last_observed_at,
                        COUNT(*)         AS event_count
                 FROM observed_event WHERE session_id != ''
                 GROUP BY session_id ORDER BY last_observed_at DESC LIMIT ?",
            )
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        Some(p) => {
            sqlx::query(
                "SELECT session_id,
                        MIN(observed_at) AS first_observed_at,
                        MAX(observed_at) AS last_observed_at,
                        COUNT(*)         AS event_count
                 FROM observed_event
                 WHERE session_id != ''
                   AND session_id IN (
                       SELECT DISTINCT session_id FROM observed_event
                        WHERE cwd = ? OR cwd = ? || '/'
                   )
                 GROUP BY session_id ORDER BY last_observed_at DESC LIMIT ?",
            )
            .bind(p)
            .bind(p)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
    };
    if totals.is_empty() {
        return Ok(Vec::new());
    }
    // Second pass: by_kind for just the session_ids we returned. Inlining the
    // IN(?) list with a dynamic placeholder list keeps it sqlx-friendly.
    let ids: Vec<String> = totals
        .iter()
        .map(|r| r.get::<String, _>("session_id"))
        .collect();
    let placeholders = (0..ids.len()).map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT session_id, kind, COUNT(*) AS n
           FROM observed_event
          WHERE session_id IN ({placeholders})
          GROUP BY session_id, kind"
    );
    let mut q = sqlx::query(&sql);
    for id in &ids {
        q = q.bind(id);
    }
    let kind_rows = q.fetch_all(pool).await?;
    let mut by_kind_map: std::collections::HashMap<
        String,
        std::collections::BTreeMap<String, i64>,
    > = std::collections::HashMap::new();
    for r in kind_rows {
        let sid: String = r.get("session_id");
        let kind: String = r.get("kind");
        let n: i64 = r.get("n");
        by_kind_map.entry(sid).or_default().insert(kind, n);
    }

    // perf-2026-06-29 — facets are materialized in session_summary by
    // recompute_session (transcript ingest path) and read back here, instead of
    // re-scanning + json_extract over observed_event on every list request.
    let mut facets = read_session_summaries(pool, &ids, &placeholders).await?;

    Ok(totals
        .into_iter()
        .map(|r| {
            let sid: String = r.get("session_id");
            let by_kind = by_kind_map.remove(&sid).unwrap_or_default();
            let f = facets.remove(&sid).unwrap_or_default();
            SessionRow {
                session_id: sid,
                first_observed_at: r.get("first_observed_at"),
                last_observed_at: r.get("last_observed_at"),
                event_count: r.get("event_count"),
                by_kind,
                project: f.project,
                model: f.model,
                slug: f.slug,
                first_user_message_preview: f.preview,
                agent_name: f.agent_name,
                team_name: f.team_name,
            }
        })
        .collect())
}

/// S6 — per-session identity facets, collected for an already-resolved id set.
#[derive(Default)]
struct SessionFacets {
    project: Option<String>,
    model: Option<String>,
    slug: Option<String>,
    preview: Option<String>,
    /// Teammate 세션 식별 (2026-07-03) — observed_event의 세션 상수 컬럼에서
    /// MIN 집계. 팀메이트가 아닌 세션은 None.
    agent_name: Option<String>,
    team_name: Option<String>,
}

/// First user-message preview is truncated to this many characters
/// (char-safe, server-side) so the list payload stays small. The WebUI
/// renders a single ellipsised line on top of this.
const PREVIEW_MAX_CHARS: usize = 140;

async fn session_facets(
    pool: &SqlitePool,
    ids: &[String],
    placeholders: &str,
) -> Result<std::collections::HashMap<String, SessionFacets>> {
    use sqlx::Row as _Row;
    let mut out: std::collections::HashMap<String, SessionFacets> =
        std::collections::HashMap::new();

    // project = most recent non-empty cwd. SQLite's bare-column rule returns
    // `cwd` from the row holding MAX(observed_at) within each group.
    let cwd_sql = format!(
        "SELECT session_id, cwd, MAX(observed_at) AS _t
           FROM observed_event
          WHERE session_id IN ({placeholders}) AND cwd IS NOT NULL AND cwd != ''
          GROUP BY session_id"
    );
    let mut q = sqlx::query(&cwd_sql);
    for id in ids {
        q = q.bind(id);
    }
    for r in q.fetch_all(pool).await? {
        let sid: String = r.get("session_id");
        out.entry(sid).or_default().project = r.try_get::<Option<String>, _>("cwd").ok().flatten();
    }

    // slug = stable per-session slug from the `system_summary` payload.
    let slug_sql = format!(
        "SELECT session_id, MAX(json_extract(payload,'$.slug')) AS slug
           FROM observed_event
          WHERE session_id IN ({placeholders}) AND kind = 'system_summary'
            AND json_extract(payload,'$.slug') IS NOT NULL
          GROUP BY session_id"
    );
    let mut q = sqlx::query(&slug_sql);
    for id in ids {
        q = q.bind(id);
    }
    for r in q.fetch_all(pool).await? {
        let sid: String = r.get("session_id");
        out.entry(sid).or_default().slug = r.try_get::<Option<String>, _>("slug").ok().flatten();
    }

    // model = dominant assistant model; tie-break by name DESC for determinism.
    let model_sql = format!(
        "SELECT session_id, json_extract(payload,'$.model') AS model, COUNT(*) AS n
           FROM observed_event
          WHERE session_id IN ({placeholders}) AND kind = 'assistant_message'
            AND json_extract(payload,'$.model') IS NOT NULL
          GROUP BY session_id, model"
    );
    let mut q = sqlx::query(&model_sql);
    for id in ids {
        q = q.bind(id);
    }
    // (count, model) per session — pick max count, tie-break by model name DESC.
    let mut best: std::collections::HashMap<String, (i64, String)> =
        std::collections::HashMap::new();
    for r in q.fetch_all(pool).await? {
        let sid: String = r.get("session_id");
        let model: String = r.get("model");
        let n: i64 = r.get("n");
        let take = match best.get(&sid) {
            None => true,
            Some((bn, bm)) => n > *bn || (n == *bn && model > *bm),
        };
        if take {
            best.insert(sid, (n, model));
        }
    }
    for (sid, (_, model)) in best {
        out.entry(sid).or_default().model = Some(model);
    }

    // preview = earliest non-empty, non-meta user message content, skipping
    // slash-command stdin wrappers (`<command-name>…`, `<local-command-…>`)
    // which are noise for identification, not real prompts.
    let preview_sql = format!(
        "SELECT session_id, json_extract(payload,'$.content') AS content, MIN(observed_at) AS _t
           FROM observed_event
          WHERE session_id IN ({placeholders}) AND kind = 'user_message' AND is_meta = 0
            AND trim(coalesce(json_extract(payload,'$.content'),'')) != ''
            AND trim(coalesce(json_extract(payload,'$.content'),'')) NOT LIKE '<command-%'
            AND trim(coalesce(json_extract(payload,'$.content'),'')) NOT LIKE '<local-command-%'
          GROUP BY session_id"
    );
    let mut q = sqlx::query(&preview_sql);
    for id in ids {
        q = q.bind(id);
    }
    for r in q.fetch_all(pool).await? {
        let sid: String = r.get("session_id");
        let content: Option<String> = r.try_get::<Option<String>, _>("content").ok().flatten();
        out.entry(sid).or_default().preview = content.map(|c| truncate_preview(&c));
    }

    // teammate 식별 (2026-07-03) — agent_name/team_name은 세션 내 상수로
    // 관측됐다(teammate_v01 fixture, 표본 1 세션). MIN은 상수 집계일 뿐
    // 선택 로직이 아니다.
    let team_sql = format!(
        "SELECT session_id, MIN(agent_name) AS agent_name, MIN(team_name) AS team_name
           FROM observed_event
          WHERE session_id IN ({placeholders}) AND team_name IS NOT NULL
          GROUP BY session_id"
    );
    let mut q = sqlx::query(&team_sql);
    for id in ids {
        q = q.bind(id);
    }
    for r in q.fetch_all(pool).await? {
        let sid: String = r.get("session_id");
        let f = out.entry(sid).or_default();
        f.agent_name = r.try_get::<Option<String>, _>("agent_name").ok().flatten();
        f.team_name = r.try_get::<Option<String>, _>("team_name").ok().flatten();
    }

    Ok(out)
}

/// perf-2026-06-29 — read materialized facets for an already-resolved id set
/// from `session_summary` (populated by `recompute_session` / backfill). This
/// replaces the four per-request grouped `json_extract` scans of
/// `session_facets`. Sessions absent from the table (OTLP-only, or not yet
/// backfilled) simply yield no entry → default (all-None) facets, which is
/// correct: they have no transcript-derived model/slug/preview.
async fn read_session_summaries(
    pool: &SqlitePool,
    ids: &[String],
    placeholders: &str,
) -> Result<std::collections::HashMap<String, SessionFacets>> {
    use sqlx::Row as _Row;
    let sql = format!(
        "SELECT session_id, project, model, slug, first_user_message_preview, agent_name, team_name
           FROM session_summary
          WHERE session_id IN ({placeholders})"
    );
    let mut q = sqlx::query(&sql);
    for id in ids {
        q = q.bind(id);
    }
    let mut out = std::collections::HashMap::new();
    for r in q.fetch_all(pool).await? {
        let sid: String = r.get("session_id");
        out.insert(
            sid,
            SessionFacets {
                project: r.try_get::<Option<String>, _>("project").ok().flatten(),
                model: r.try_get::<Option<String>, _>("model").ok().flatten(),
                slug: r.try_get::<Option<String>, _>("slug").ok().flatten(),
                preview: r
                    .try_get::<Option<String>, _>("first_user_message_preview")
                    .ok()
                    .flatten(),
                agent_name: r.try_get::<Option<String>, _>("agent_name").ok().flatten(),
                team_name: r.try_get::<Option<String>, _>("team_name").ok().flatten(),
            },
        );
    }
    Ok(out)
}

/// perf-2026-06-29 — compute one session's transcript facets and UPSERT them
/// into `session_summary`. Called by `recompute_session` after a transcript
/// batch (and by backfill). Idempotent. Empty `session_id` is a no-op.
pub async fn upsert_session_summary(pool: &SqlitePool, session_id: &str) -> Result<()> {
    if session_id.is_empty() {
        return Ok(());
    }
    let mut facets = session_facets(pool, &[session_id.to_string()], "?").await?;
    let f = facets.remove(session_id).unwrap_or_default();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO session_summary
            (session_id, project, model, slug, first_user_message_preview, agent_name, team_name, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(session_id) DO UPDATE SET
            project = excluded.project,
            model = excluded.model,
            slug = excluded.slug,
            first_user_message_preview = excluded.first_user_message_preview,
            agent_name = excluded.agent_name,
            team_name = excluded.team_name,
            updated_at = excluded.updated_at",
    )
    .bind(session_id)
    .bind(f.project)
    .bind(f.model)
    .bind(f.slug)
    .bind(f.preview)
    .bind(f.agent_name)
    .bind(f.team_name)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// perf-2026-06-29 — fill `session_summary` for every session present in
/// `observed_event` but missing from the table (rows ingested before migration
/// 0025). Called on serve/ingest startup; non-fatal. Returns the number of
/// sessions filled. Idempotent: once filled, subsequent calls find nothing
/// missing and return 0.
pub async fn backfill_session_summary(pool: &SqlitePool) -> Result<u64> {
    let missing: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT session_id FROM observed_event
          WHERE session_id != ''
            AND session_id NOT IN (SELECT session_id FROM session_summary)",
    )
    .fetch_all(pool)
    .await?;
    let mut n = 0u64;
    for sid in &missing {
        upsert_session_summary(pool, sid).await?;
        n += 1;
    }
    Ok(n)
}

/// Char-safe single-line preview: collapse whitespace runs, trim, and cap at
/// `PREVIEW_MAX_CHARS` characters (never split a UTF-8 char).
fn truncate_preview(s: &str) -> String {
    let collapsed = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= PREVIEW_MAX_CHARS {
        collapsed
    } else {
        collapsed
            .chars()
            .take(PREVIEW_MAX_CHARS)
            .collect::<String>()
            + "…"
    }
}

/// Distinct `turn_id` count for a session = number of user turns (prompts).
/// `turn_id` is the prompt_id carried by user + assistant events; counting
/// distinct non-null values yields user-turn count (NOT assistant-event count).
pub async fn count_distinct_turns(pool: &SqlitePool, session_id: &str) -> Result<i64> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(DISTINCT turn_id) FROM observed_event \
         WHERE session_id = ? AND turn_id IS NOT NULL",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

pub async fn list_session(
    pool: &SqlitePool,
    session_id: &str,
    limit: i64,
) -> Result<Vec<ObservedEvent>> {
    let rows = sqlx::query(
        "SELECT * FROM observed_event WHERE session_id = ? ORDER BY observed_at ASC LIMIT ?",
    )
    .bind(session_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(row_to_observed).collect())
}

/// Slice-8 — newest `limit` events of a session, ordered DESC so we don't have
/// to scan from the start. Used by the WebUI live timeline: sessions with more
/// than `limit` rows would otherwise show only the oldest window with the
/// ASC variant above, and the live tail's newest envelopes would never appear
/// in the rendered page. The handler reverses before serialising so the wire
/// order remains ASC (consumers expect a chronological timeline).
///
/// Long-term, this is replaced by windowed range queries (slice-9 follow-up):
/// `?from=&to=&limit=` with a client-side LRU chunk cache + virtualisation,
/// matching the video-streaming buffer pattern.
pub async fn list_session_latest(
    pool: &SqlitePool,
    session_id: &str,
    limit: i64,
) -> Result<Vec<ObservedEvent>> {
    let rows = sqlx::query(
        "SELECT * FROM observed_event WHERE session_id = ? ORDER BY observed_at DESC LIMIT ?",
    )
    .bind(session_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(row_to_observed).collect())
}

/// Slice-9 — windowed range query for paged event reads. Supersedes
/// `list_session_latest` in handler usage; `list_session_latest` remains as
/// the no-cursor convenience for SSE backfill's `last_event_id` path.
///
/// Ordering: `(observed_at, event_id)` ASC on the wire (DEV-S8-10 lock).
/// SQL: when `before` is set or no cursors → DESC LIMIT then reverse in
/// memory so we always return the most relevant window without a full scan;
/// when only `after` is set → ASC LIMIT directly.
///
/// Limits: clamped to `[1, 1000]`. Returning 5000+ rows is the slice-8
/// anti-pattern this slice replaces — see DEV-S8-14.
pub async fn list_session_window(
    pool: &SqlitePool,
    session_id: &str,
    before: Option<&Cursor>,
    after: Option<&Cursor>,
    limit: i64,
) -> Result<Vec<ObservedEvent>> {
    let limit = limit.clamp(1, 1000);
    let rows = match (before, after) {
        (None, None) => {
            // Conversation-anchored initial window. The message view renders
            // only conversation/activity kinds and DROPS bulk telemetry
            // (metric_sample / otel_span / most log_record) and non-rendered
            // envelopes (attachment_meta / session_state). A session that ends
            // in a telemetry burst would otherwise load a newest-N raw window
            // full of dropped rows → empty stream though conversation exists.
            //
            // So window by RENDERED events, not raw: bound the window to
            //   [ limit-th newest rendered event ,  newest rendered event ]
            // and return every row in that range (interleaved telemetry stays
            // loaded for detail metrics; the trailing burst above the newest
            // rendered event is excluded). Guarantees up to `limit` rendered
            // events, or the whole session when it has fewer. Falls back to
            // plain newest-N when the session has no rendered events at all.
            const RENDERED: &str = "kind IN ('user_message','assistant_message',\
                'tool_call','tool_result','thinking','hook_event','system_summary','diff_hunk')";
            // newest rendered event = the upper bound (anchor).
            let upper = sqlx::query(&format!(
                "SELECT observed_at, event_id FROM observed_event \
                 WHERE session_id = ? AND {RENDERED} \
                 ORDER BY observed_at DESC, event_id DESC LIMIT 1"
            ))
            .bind(session_id)
            .fetch_optional(pool)
            .await?;
            match upper {
                None => {
                    // No rendered events (telemetry-only session) → newest-N.
                    sqlx::query(
                        "SELECT * FROM observed_event WHERE session_id = ? \
                         ORDER BY observed_at DESC, event_id DESC LIMIT ?",
                    )
                    .bind(session_id)
                    .bind(limit)
                    .fetch_all(pool)
                    .await?
                }
                Some(u) => {
                    let uts: String = u.get("observed_at");
                    let ueid: String = u.get("event_id");
                    // `limit`-th newest rendered event = the lower bound (None
                    // when the session has fewer than `limit` rendered events,
                    // in which case the window runs back to the session start).
                    let lower = sqlx::query(&format!(
                        "SELECT observed_at, event_id FROM observed_event \
                         WHERE session_id = ? AND {RENDERED} \
                         ORDER BY observed_at DESC, event_id DESC LIMIT 1 OFFSET ?"
                    ))
                    .bind(session_id)
                    .bind(limit - 1)
                    .fetch_optional(pool)
                    .await?;
                    // Raw cap so a pathologically telemetry-dense range can't
                    // return an unbounded page (mirrors the client's max window).
                    const RAW_CAP: i64 = 5000;
                    match lower {
                        Some(l) => {
                            let lts: String = l.get("observed_at");
                            let leid: String = l.get("event_id");
                            sqlx::query(
                                "SELECT * FROM observed_event WHERE session_id = ? \
                                 AND (observed_at < ? OR (observed_at = ? AND event_id <= ?)) \
                                 AND (observed_at > ? OR (observed_at = ? AND event_id >= ?)) \
                                 ORDER BY observed_at DESC, event_id DESC LIMIT ?",
                            )
                            .bind(session_id)
                            .bind(&uts)
                            .bind(&uts)
                            .bind(&ueid)
                            .bind(&lts)
                            .bind(&lts)
                            .bind(&leid)
                            .bind(RAW_CAP)
                            .fetch_all(pool)
                            .await?
                        }
                        None => {
                            sqlx::query(
                                "SELECT * FROM observed_event WHERE session_id = ? \
                             AND (observed_at < ? OR (observed_at = ? AND event_id <= ?)) \
                             ORDER BY observed_at DESC, event_id DESC LIMIT ?",
                            )
                            .bind(session_id)
                            .bind(&uts)
                            .bind(&uts)
                            .bind(&ueid)
                            .bind(RAW_CAP)
                            .fetch_all(pool)
                            .await?
                        }
                    }
                }
            }
        }
        (Some(b), None) => {
            let ts = b.observed_at.to_rfc3339();
            sqlx::query(
                "SELECT * FROM observed_event WHERE session_id = ? \
                 AND (observed_at < ? OR (observed_at = ? AND event_id < ?)) \
                 ORDER BY observed_at DESC, event_id DESC LIMIT ?",
            )
            .bind(session_id)
            .bind(&ts)
            .bind(&ts)
            .bind(&b.event_id)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        (None, Some(a)) => {
            let ts = a.observed_at.to_rfc3339();
            sqlx::query(
                "SELECT * FROM observed_event WHERE session_id = ? \
                 AND (observed_at > ? OR (observed_at = ? AND event_id > ?)) \
                 ORDER BY observed_at ASC, event_id ASC LIMIT ?",
            )
            .bind(session_id)
            .bind(&ts)
            .bind(&ts)
            .bind(&a.event_id)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        (Some(b), Some(a)) => {
            let ats = a.observed_at.to_rfc3339();
            let bts = b.observed_at.to_rfc3339();
            sqlx::query(
                "SELECT * FROM observed_event WHERE session_id = ? \
                 AND (observed_at > ? OR (observed_at = ? AND event_id > ?)) \
                 AND (observed_at < ? OR (observed_at = ? AND event_id < ?)) \
                 ORDER BY observed_at ASC, event_id ASC LIMIT ?",
            )
            .bind(session_id)
            .bind(&ats)
            .bind(&ats)
            .bind(&a.event_id)
            .bind(&bts)
            .bind(&bts)
            .bind(&b.event_id)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
    };
    let mut events: Vec<ObservedEvent> = rows.into_iter().map(row_to_observed).collect();
    // before-only and no-cursor used DESC SQL — flip to chronological ASC.
    let needs_reverse = matches!((before, after), (Some(_), None) | (None, None));
    if needs_reverse {
        events.reverse();
    }
    Ok(events)
}

/// Dogfood 2026-06-12 (§3-2) — all conversation-kind events of a session,
/// chronological, unwindowed. Feeds the turn rollup: turn aggregation needs
/// every user_message/tool_call of the session, and conversation kinds are
/// bounded in practice (the heaviest dogfood session: ~700 of 6,533 rows) —
/// the bulk telemetry kinds stay excluded.
pub async fn list_session_conversation(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Vec<ObservedEvent>> {
    let rows = sqlx::query(
        "SELECT * FROM observed_event WHERE session_id = ? \
         AND kind IN ('user_message','assistant_message','tool_call','tool_result') \
         ORDER BY observed_at ASC, event_id ASC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(row_to_observed).collect())
}

/// Dogfood 2026-06-12 — kind-filtered cursor window (`?kind=` on the events
/// endpoint). Unlike the unfiltered window there is no rendered-kind anchor
/// special case: the caller already names the kinds it wants, so every branch
/// filters by `kind IN (...)` directly. Ordering/cursor contract is identical
/// to `list_session_window` (`(observed_at, event_id)` ASC on the wire).
pub async fn list_session_window_kinds(
    pool: &SqlitePool,
    session_id: &str,
    kinds: &[String],
    before: Option<&Cursor>,
    after: Option<&Cursor>,
    limit: i64,
) -> Result<Vec<ObservedEvent>> {
    let limit = limit.clamp(1, 1000);
    let placeholders = (0..kinds.len()).map(|_| "?").collect::<Vec<_>>().join(",");
    let mut sql =
        format!("SELECT * FROM observed_event WHERE session_id = ? AND kind IN ({placeholders})");
    match (before, after) {
        (None, None) => {
            sql.push_str(" ORDER BY observed_at DESC, event_id DESC LIMIT ?");
        }
        (Some(_), None) => {
            sql.push_str(
                " AND (observed_at < ? OR (observed_at = ? AND event_id < ?)) \
                 ORDER BY observed_at DESC, event_id DESC LIMIT ?",
            );
        }
        (None, Some(_)) => {
            sql.push_str(
                " AND (observed_at > ? OR (observed_at = ? AND event_id > ?)) \
                 ORDER BY observed_at ASC, event_id ASC LIMIT ?",
            );
        }
        (Some(_), Some(_)) => {
            sql.push_str(
                " AND (observed_at > ? OR (observed_at = ? AND event_id > ?)) \
                 AND (observed_at < ? OR (observed_at = ? AND event_id < ?)) \
                 ORDER BY observed_at ASC, event_id ASC LIMIT ?",
            );
        }
    }
    let mut q = sqlx::query(&sql).bind(session_id);
    for k in kinds {
        q = q.bind(k);
    }
    match (before, after) {
        (None, None) => {}
        (Some(b), None) => {
            let ts = b.observed_at.to_rfc3339();
            q = q.bind(ts.clone()).bind(ts).bind(&b.event_id);
        }
        (None, Some(a)) => {
            let ts = a.observed_at.to_rfc3339();
            q = q.bind(ts.clone()).bind(ts).bind(&a.event_id);
        }
        (Some(b), Some(a)) => {
            let ats = a.observed_at.to_rfc3339();
            let bts = b.observed_at.to_rfc3339();
            q = q
                .bind(ats.clone())
                .bind(ats)
                .bind(&a.event_id)
                .bind(bts.clone())
                .bind(bts)
                .bind(&b.event_id);
        }
    }
    let rows = q.bind(limit).fetch_all(pool).await?;
    let mut events: Vec<ObservedEvent> = rows.into_iter().map(row_to_observed).collect();
    // DESC branches flip to chronological ASC for the wire.
    if matches!((before, after), (Some(_), None) | (None, None)) {
        events.reverse();
    }
    Ok(events)
}

/// Deep-link window — the events around (and including) `event_id`, ordered
/// ASC like every window. Used by `?around=<event_id>`: a replay deep link
/// (`/sessions/:id?selected=<event_id>`) carries only the event_id, so the
/// client cannot build a `<observed_at>|<event_id>` cursor for it — and the
/// cursor bounds are exclusive, so even a known cursor could not include the
/// target itself. Returns `None` when the event does not exist in the session.
///
/// Window shape: `(limit-1)/2` events strictly before the target, the target,
/// then the remainder strictly after — both halves clamp at the session
/// edges (no rebalancing; a target near the start/end yields a smaller
/// window). Reuses `list_session_window`'s before/after arms so the ordering
/// key `(observed_at, event_id)` stays the single SQL contract.
pub async fn list_session_around(
    pool: &SqlitePool,
    session_id: &str,
    event_id: &str,
    limit: i64,
) -> Result<Option<Vec<ObservedEvent>>> {
    let limit = limit.clamp(1, 1000);
    let row = sqlx::query("SELECT * FROM observed_event WHERE session_id = ? AND event_id = ?")
        .bind(session_id)
        .bind(event_id)
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else { return Ok(None) };
    let target = row_to_observed(row);
    let cursor = Cursor {
        observed_at: target.observed_at,
        event_id: target.event_id.clone(),
    };
    let before_n = (limit - 1) / 2;
    let after_n = limit - 1 - before_n;
    let mut events = Vec::with_capacity(limit as usize);
    if before_n > 0 {
        events.extend(list_session_window(pool, session_id, Some(&cursor), None, before_n).await?);
    }
    events.push(target);
    if after_n > 0 {
        events.extend(list_session_window(pool, session_id, None, Some(&cursor), after_n).await?);
    }
    Ok(Some(events))
}

/// On-demand correlated telemetry for the detail view: the events whose indexed
/// `tool_use_id` / `request_id` columns match the given keys, used when an
/// entity's correlated telemetry falls outside the loaded message window.
///
/// Correlation uses the INDEXED columns (not payload JSON) introduced in C2:
///
///   **tool_use_id arm** — `kind != 'tool_call'` guard is intentional:
///     - OTel `log_record` / `metric_sample` events: column set by C2 ingest
///     - transcript `tool_result`: column set by `mapping.rs` (line 75-78)
///     - transcript `tool_call`: column set, but deliberately EXCLUDED — the
///       caller already holds the tool_call; returning it again would duplicate
///       it in the detail view.
///
///   **request_id arm** — scoped to OTel kinds only (`log_record`, `otel_span`,
///     `metric_sample`) to preserve the semantics of the original query, which
///     matched only `attributes.request_id` (OTel logs) and `raw_span.attributes`
///     (OTel spans — that `payload.raw_span` re-embed was since removed in
///     Tier 3-1; request_id now lives in the indexed column). Transcript
///     `assistant_message` / `thinking` / `tool_call` also carry `request_id` in
///     the column (set by `mapping.rs`), but they were NOT matched by the old
///     payload-path query and must remain excluded.
pub async fn events_correlated(
    pool: &SqlitePool,
    session_id: &str,
    tool_use_id: Option<&str>,
    request_id: Option<&str>,
) -> Result<Vec<ObservedEvent>> {
    let rows = sqlx::query(
        "SELECT * FROM observed_event WHERE session_id = ? AND ( \
           (? IS NOT NULL AND tool_use_id = ? AND kind != 'tool_call') \
           OR (? IS NOT NULL AND request_id = ? AND kind IN ('log_record','otel_span','metric_sample')) \
         ) ORDER BY observed_at ASC, event_id ASC LIMIT 500",
    )
    .bind(session_id)
    .bind(tool_use_id)
    .bind(tool_use_id)
    .bind(request_id)
    .bind(request_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(row_to_observed).collect())
}

/// Slice-9 — per-kind row counts for a single session. Replaces the
/// summary's `by_kind` that slice-8 derived from the (windowed) events array;
/// once `session_detail` stopped returning events, by_kind needed its own
/// query so the WebUI's source-mix badges stay accurate on 5000+ event
/// sessions.
pub async fn session_kind_counts(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<std::collections::BTreeMap<String, i64>> {
    let rows = sqlx::query(
        "SELECT kind, COUNT(*) AS n FROM observed_event \
         WHERE session_id = ? GROUP BY kind",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    let mut out = std::collections::BTreeMap::new();
    for r in rows {
        let k: String = r.get("kind");
        let n: i64 = r.get("n");
        out.insert(k, n);
    }
    Ok(out)
}

/// Slice-8 — accurate per-session summary (count + first/last observed_at)
/// independent of the `list_session_latest` window. Without this the WebUI
/// MetaStrip would show the first/last of the 5000-event window instead of
/// the true session boundaries.
pub async fn session_summary(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Option<(i64, String, String)>> {
    let row: Option<(i64, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT COUNT(*), MIN(observed_at), MAX(observed_at) \
         FROM observed_event WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|(c, f, l)| match (f, l) {
        (Some(f), Some(l)) if c > 0 => Some((c, f, l)),
        _ => None,
    }))
}

fn row_to_observed(r: sqlx::sqlite::SqliteRow) -> ObservedEvent {
    let actor: String = r.get("actor");
    let kind: String = r.get("kind");
    let payload_str: String = r.get("payload");
    let mut payload: serde_json::Value =
        serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);
    let telemetry = if let serde_json::Value::Object(map) = &mut payload {
        map.remove("telemetry")
            .and_then(|v| serde_json::from_value(v).ok())
    } else {
        None
    };
    ObservedEvent {
        event_id: r.get("event_id"),
        raw_event_id: r.get("raw_event_id"),
        schema_version: r.get("schema_version"),
        parser_version: r.get("parser_version"),
        session_id: r.get("session_id"),
        event_uuid: r.try_get("event_uuid").ok(),
        parent_uuid: r.try_get("parent_uuid").ok(),
        observed_at: chrono::DateTime::parse_from_rfc3339(&r.get::<String, _>("observed_at"))
            .unwrap()
            .with_timezone(&chrono::Utc),
        actor: match actor.as_str() {
            "user" => Actor::User,
            "assistant" => Actor::Assistant,
            "hook" => Actor::Hook,
            "tool" => Actor::Tool,
            _ => Actor::System,
        },
        kind: match kind.as_str() {
            "user_message" => EventKind::UserMessage,
            "assistant_message" => EventKind::AssistantMessage,
            "thinking" => EventKind::Thinking,
            "tool_call" => EventKind::ToolCall,
            "tool_result" => EventKind::ToolResult,
            "hook_event" => EventKind::HookEvent,
            "system_summary" => EventKind::SystemSummary,
            "session_state" => EventKind::SessionState,
            "attachment_meta" => EventKind::AttachmentMeta,
            "otel_span" => EventKind::OtelSpan,
            "metric_sample" => EventKind::MetricSample,
            "log_record" => EventKind::LogRecord,
            _ => EventKind::Unknown,
        },
        subkind: r.try_get("subkind").ok(),
        tool_use_id: r.try_get("tool_use_id").ok(),
        tool_name: r.try_get("tool_name").ok(),
        request_id: r.try_get("request_id").ok(),
        message_id: r.try_get("message_id").ok(),
        turn_id: r.try_get("turn_id").ok(),
        source_tool_assistant_uuid: r.try_get("source_tool_assistant_uuid").ok(),
        source_tool_use_id: r.try_get("source_tool_use_id").ok(),
        is_sidechain: r.get::<i64, _>("is_sidechain") != 0,
        agent_id: r.try_get("agent_id").ok(),
        workflow_run_id: r.try_get("workflow_run_id").ok(),
        agent_name: r.try_get("agent_name").ok(),
        team_name: r.try_get("team_name").ok(),
        is_meta: r.get::<i64, _>("is_meta") != 0,
        cwd: r.try_get("cwd").ok(),
        git_branch: r.try_get("git_branch").ok(),
        user_type: r.try_get("user_type").ok(),
        entrypoint: r.try_get("entrypoint").ok(),
        cc_version: r.try_get("cc_version").ok(),
        trace_id: r.try_get("trace_id").ok(),
        span_id: r.try_get("span_id").ok(),
        parent_span_id: r.try_get("parent_span_id").ok(),
        latency_ms: r.try_get("latency_ms").ok(),
        telemetry,
        payload,
    }
}
