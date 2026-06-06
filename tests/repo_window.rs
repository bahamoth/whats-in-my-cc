//! Slice-9 L1 tests — `repo_observed::list_session_window` cursor-paged
//! reads. Seed 1500 rows so we can exercise limit clamp (>1000) and three
//! cursor combinations (no-cursor, before, after) with strict ordering.

use chrono::{DateTime, TimeZone, Utc};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

use wimcc::db::{migrate, repo_observed, repo_raw, repo_runs};
use wimcc::model::cursor::Cursor;
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

const SESS: &str = "sess-window";
const SEED_N: usize = 1500;

async fn seed_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();

    let run_id = repo_runs::start(&pool).await.unwrap();
    // One observed_at per row, monotonic +1 second so ordering is unambiguous.
    let base: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();
    for i in 0..SEED_N {
        let event_id = format!("01J{:023}", i); // ULID-like, 26 chars, monotonic
        let raw_id = format!("raw_{i:06}");
        repo_raw::insert_dedup(
            &pool,
            &repo_raw::NewRaw {
                raw_event_id: raw_id.clone(),
                ingest_run_id: run_id.clone(),
                source_type: "test".into(),
                source_uri: format!("test://{i}"),
                source_line_no: i as i64,
                source_byte_offset: 0,
                payload_sha256: format!("sha_{i:06}"),
                payload: b"{}".to_vec(),
                parse_error: None,
                captured_at: chrono::Utc::now(),
                redaction_state: "not_applicable".into(),
                redaction_manifest: None,
            },
        )
        .await
        .unwrap();
        let ev = ObservedEvent {
            event_id,
            raw_event_id: raw_id,
            schema_version: "0.5.0".into(),
            session_id: SESS.into(),
            observed_at: base + chrono::Duration::seconds(i as i64),
            actor: Actor::User,
            kind: EventKind::UserMessage,
            parser_version: "test".into(),
            ..Default::default()
        };
        repo_observed::insert(&pool, &ev).await.unwrap();
    }
    pool
}

fn cursor_at(i: usize) -> Cursor {
    let base: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();
    Cursor {
        observed_at: base + chrono::Duration::seconds(i as i64),
        event_id: format!("01J{:023}", i),
    }
}

#[tokio::test]
async fn empty_session_returns_empty() {
    let pool = seed_pool().await;
    let rows = repo_observed::list_session_window(&pool, "no-such-session", None, None, 500)
        .await
        .unwrap();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn no_cursor_returns_newest_limit_asc() {
    let pool = seed_pool().await;
    let rows = repo_observed::list_session_window(&pool, SESS, None, None, 500)
        .await
        .unwrap();
    assert_eq!(rows.len(), 500);
    // Newest 500 means rows[1000..1500] from the seed, ordered ASC on the wire.
    assert_eq!(rows.first().unwrap().event_id, format!("01J{:023}", 1000));
    assert_eq!(rows.last().unwrap().event_id, format!("01J{:023}", 1499));
}

#[tokio::test]
async fn before_only_returns_older_window_asc() {
    let pool = seed_pool().await;
    let cur = cursor_at(500);
    let rows = repo_observed::list_session_window(&pool, SESS, Some(&cur), None, 500)
        .await
        .unwrap();
    // Strictly older than row[500]. Newest 500 of those = rows[0..500].
    assert_eq!(rows.len(), 500);
    assert_eq!(rows.first().unwrap().event_id, format!("01J{:023}", 0));
    assert_eq!(rows.last().unwrap().event_id, format!("01J{:023}", 499));
}

#[tokio::test]
async fn after_only_returns_newer_window_asc() {
    let pool = seed_pool().await;
    let cur = cursor_at(500);
    let rows = repo_observed::list_session_window(&pool, SESS, None, Some(&cur), 500)
        .await
        .unwrap();
    // Strictly newer than row[500]. Oldest 500 of those = rows[501..1001].
    assert_eq!(rows.len(), 500);
    assert_eq!(rows.first().unwrap().event_id, format!("01J{:023}", 501));
    assert_eq!(rows.last().unwrap().event_id, format!("01J{:023}", 1000));
}

#[tokio::test]
async fn both_cursors_returns_slice() {
    let pool = seed_pool().await;
    let after_cur = cursor_at(100);
    let before_cur = cursor_at(200);
    let rows = repo_observed::list_session_window(
        &pool,
        SESS,
        Some(&before_cur),
        Some(&after_cur),
        500,
    )
    .await
    .unwrap();
    // Strictly between row[100] and row[200]. rows[101..200].
    assert_eq!(rows.len(), 99);
    assert_eq!(rows.first().unwrap().event_id, format!("01J{:023}", 101));
    assert_eq!(rows.last().unwrap().event_id, format!("01J{:023}", 199));
}

#[tokio::test]
async fn limit_clamps_to_1000() {
    let pool = seed_pool().await;
    let rows = repo_observed::list_session_window(&pool, SESS, None, None, 5000)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1000);
}

#[tokio::test]
async fn limit_min_one() {
    let pool = seed_pool().await;
    let rows = repo_observed::list_session_window(&pool, SESS, None, None, 0)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
}

/// Regression for DEV-S8-10: cross-source event_ids must order by observed_at
/// primary key, not lexicographic event_id (where `metric:...` > `01J...`).
#[tokio::test]
async fn cross_source_event_ids_order_by_observed_at() {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let run_id = repo_runs::start(&pool).await.unwrap();
    let base: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();
    // Three rows, observed_at strictly increasing, event_ids deliberately
    // lexicographically OUT of order:
    //   t=0  : "metric:foo:bar:0"   ('m' = 0x6d)
    //   t=1  : "01J0000000000000000000000A"  ('0' = 0x30)
    //   t=2  : "zzzz_just_for_chaos"
    for (i, eid) in [
        "metric:foo:bar:0",
        "01J0000000000000000000000A",
        "zzzz_just_for_chaos",
    ]
    .iter()
    .enumerate()
    {
        let raw_id = format!("raw_x_{i}");
        repo_raw::insert_dedup(
            &pool,
            &repo_raw::NewRaw {
                raw_event_id: raw_id.clone(),
                ingest_run_id: run_id.clone(),
                source_type: "test".into(),
                source_uri: format!("test://x{i}"),
                source_line_no: i as i64,
                source_byte_offset: 0,
                payload_sha256: format!("sha_x{i}"),
                payload: b"{}".to_vec(),
                parse_error: None,
                captured_at: chrono::Utc::now(),
                redaction_state: "not_applicable".into(),
                redaction_manifest: None,
            },
        )
        .await
        .unwrap();
        let ev = ObservedEvent {
            event_id: eid.to_string(),
            raw_event_id: raw_id,
            schema_version: "0.5.0".into(),
            session_id: "sess-mixed".into(),
            observed_at: base + chrono::Duration::seconds(i as i64),
            actor: Actor::User,
            kind: EventKind::UserMessage,
            parser_version: "test".into(),
            ..Default::default()
        };
        repo_observed::insert(&pool, &ev).await.unwrap();
    }
    let rows = repo_observed::list_session_window(&pool, "sess-mixed", None, None, 500)
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);
    // ASC by observed_at:
    assert_eq!(rows[0].event_id, "metric:foo:bar:0");
    assert_eq!(rows[1].event_id, "01J0000000000000000000000A");
    assert_eq!(rows[2].event_id, "zzzz_just_for_chaos");
}

// --- conversation-anchored initial window (no cursor) -------------------
// A session ending in a telemetry burst must not load a window full of rows
// the message view drops. The no-cursor window anchors at the last
// non-telemetry (conversation) event, excluding the trailing telemetry tail.

async fn seed_one(
    pool: &SqlitePool,
    run_id: &str,
    sess: &str,
    i: usize,
    kind: EventKind,
    eid: &str,
) {
    let raw_id = format!("raw_{sess}_{i}");
    repo_raw::insert_dedup(
        pool,
        &repo_raw::NewRaw {
            raw_event_id: raw_id.clone(),
            ingest_run_id: run_id.to_string(),
            source_type: "test".into(),
            source_uri: format!("test://{sess}/{i}"),
            source_line_no: i as i64,
            source_byte_offset: 0,
            payload_sha256: format!("sha_{sess}_{i}"),
            payload: b"{}".to_vec(),
            parse_error: None,
            captured_at: chrono::Utc::now(),
            redaction_state: "not_applicable".into(),
            redaction_manifest: None,
        },
    )
    .await
    .unwrap();
    let base: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();
    let ev = ObservedEvent {
        event_id: eid.to_string(),
        raw_event_id: raw_id,
        schema_version: "0.5.0".into(),
        session_id: sess.into(),
        observed_at: base + chrono::Duration::seconds(i as i64),
        actor: Actor::System,
        kind,
        parser_version: "test".into(),
        ..Default::default()
    };
    repo_observed::insert(pool, &ev).await.unwrap();
}

async fn mem_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn no_cursor_window_anchors_at_last_conversation_skipping_telemetry_tail() {
    let pool = mem_pool().await;
    let run_id = repo_runs::start(&pool).await.unwrap();
    let sess = "sess-anchor";
    // 3 conversation events, then a 5-event telemetry burst AFTER them.
    for i in 0..3 {
        seed_one(&pool, &run_id, sess, i, EventKind::UserMessage, &format!("c{i}")).await;
    }
    for i in 3..8 {
        seed_one(&pool, &run_id, sess, i, EventKind::MetricSample, &format!("m{i}")).await;
    }
    let rows = repo_observed::list_session_window(&pool, sess, None, None, 100)
        .await
        .unwrap();
    // window anchored at the last conversation event (ASC → newest is last)
    assert_eq!(rows.last().unwrap().kind, EventKind::UserMessage);
    assert_eq!(rows.last().unwrap().event_id, "c2");
    // the telemetry tail must NOT fill the window
    assert!(
        rows.iter().all(|e| e.kind != EventKind::MetricSample),
        "trailing telemetry burst must be excluded from the no-cursor window",
    );
    assert_eq!(rows.len(), 3);
}

#[tokio::test]
async fn no_cursor_window_falls_back_to_newest_when_telemetry_only() {
    let pool = mem_pool().await;
    let run_id = repo_runs::start(&pool).await.unwrap();
    let sess = "sess-telem-only";
    for i in 0..4 {
        seed_one(&pool, &run_id, sess, i, EventKind::MetricSample, &format!("m{i}")).await;
    }
    let rows = repo_observed::list_session_window(&pool, sess, None, None, 100)
        .await
        .unwrap();
    // no conversation event to anchor on → return the newest telemetry (no empty)
    assert_eq!(rows.len(), 4);
}

#[tokio::test]
async fn no_cursor_anchor_ignores_trailing_non_rendered_kinds() {
    // The trailing tail can include non-telemetry-but-non-rendered kinds
    // (attachment_meta, session_state). The anchor must be the last RENDERED
    // event (here a UserMessage), not a trailing attachment_meta/session_state.
    let pool = mem_pool().await;
    let run_id = repo_runs::start(&pool).await.unwrap();
    let sess = "sess-nonrendered-tail";
    seed_one(&pool, &run_id, sess, 0, EventKind::UserMessage, "c0").await;
    seed_one(&pool, &run_id, sess, 1, EventKind::UserMessage, "c1").await;
    seed_one(&pool, &run_id, sess, 2, EventKind::AttachmentMeta, "a2").await;
    seed_one(&pool, &run_id, sess, 3, EventKind::SessionState, "s3").await;
    let rows = repo_observed::list_session_window(&pool, sess, None, None, 100)
        .await
        .unwrap();
    assert_eq!(rows.last().unwrap().kind, EventKind::UserMessage);
    assert_eq!(rows.last().unwrap().event_id, "c1");
    assert!(
        rows.iter()
            .all(|e| e.kind != EventKind::AttachmentMeta && e.kind != EventKind::SessionState),
        "trailing non-rendered kinds must not anchor/fill the window",
    );
}

// --- on-demand correlated telemetry (detail view, window-外) ------------

/// Seeds an observed_event with the given payload AND the indexed `tool_use_id`
/// / `request_id` columns — mirrors what C2 ingest does for OTel events and
/// what `mapping.rs` does for transcript events.
async fn seed_payload_with_cols(
    pool: &SqlitePool,
    run_id: &str,
    sess: &str,
    i: usize,
    kind: EventKind,
    eid: &str,
    payload: serde_json::Value,
    tool_use_id: Option<&str>,
    request_id: Option<&str>,
) {
    let raw_id = format!("rawp_{sess}_{i}");
    repo_raw::insert_dedup(
        pool,
        &repo_raw::NewRaw {
            raw_event_id: raw_id.clone(),
            ingest_run_id: run_id.to_string(),
            source_type: "test".into(),
            source_uri: format!("test://{sess}/{i}"),
            source_line_no: i as i64,
            source_byte_offset: 0,
            payload_sha256: format!("shap_{sess}_{i}"),
            payload: b"{}".to_vec(),
            parse_error: None,
            captured_at: chrono::Utc::now(),
            redaction_state: "not_applicable".into(),
            redaction_manifest: None,
        },
    )
    .await
    .unwrap();
    let base: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();
    let ev = ObservedEvent {
        event_id: eid.to_string(),
        raw_event_id: raw_id,
        schema_version: "0.5.0".into(),
        session_id: sess.into(),
        observed_at: base + chrono::Duration::seconds(i as i64),
        actor: Actor::System,
        kind,
        payload,
        tool_use_id: tool_use_id.map(String::from),
        request_id: request_id.map(String::from),
        parser_version: "test".into(),
        ..Default::default()
    };
    repo_observed::insert(pool, &ev).await.unwrap();
}

#[tokio::test]
async fn events_correlated_by_tool_use_id_matches_payload_attributes() {
    use serde_json::json;
    let pool = mem_pool().await;
    let run_id = repo_runs::start(&pool).await.unwrap();
    let sess = "sess-corr-tool";
    // tool_use_id column populated (C2 ingest now sets it for OTel log_records).
    seed_payload_with_cols(&pool, &run_id, sess, 0, EventKind::LogRecord, "tr",
        json!({"event_name":"tool_result","attributes":{"tool_use_id":"u1","duration_ms":"57"}}),
        Some("u1"), None).await;
    seed_payload_with_cols(&pool, &run_id, sess, 1, EventKind::LogRecord, "other",
        json!({"event_name":"tool_result","attributes":{"tool_use_id":"OTHER"}}),
        Some("OTHER"), None).await;
    let rows = repo_observed::events_correlated(&pool, sess, Some("u1"), None)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].event_id, "tr");
}

// A tool call rejected at input validation (e.g. Edit "File has not been read
// yet") emits NO OTel tool_result/tool_decision log_record, so its only result
// is the transcript tool_result event, whose tool_use_id lives at
// `payload.tool_result.tool_use_id` — a DIFFERENT path than the OTel log's
// `payload.attributes.tool_use_id`. The on-demand correlated fetch exists to
// populate detail metrics when telemetry falls outside the loaded window, so it
// must reach this transcript result too; otherwise a tool_call selected far from
// its result still shows "지표 미수집". DB-verified shape: payload =
// { content_ordinal, tool_result:{ type, content, is_error?, tool_use_id } }.
#[tokio::test]
async fn events_correlated_by_tool_use_id_matches_transcript_tool_result() {
    use serde_json::json;
    let pool = mem_pool().await;
    let run_id = repo_runs::start(&pool).await.unwrap();
    let sess = "sess-corr-transcript";
    // mapping.rs sets tool_use_id column for transcript ToolResult (line 75-78).
    seed_payload_with_cols(&pool, &run_id, sess, 0, EventKind::ToolResult, "tres",
        json!({"content_ordinal":0,"tool_result":{"type":"tool_result","is_error":true,
            "tool_use_id":"u1","content":"<tool_use_error>File has not been read yet.</tool_use_error>"}}),
        Some("u1"), None).await;
    seed_payload_with_cols(&pool, &run_id, sess, 1, EventKind::ToolResult, "other",
        json!({"content_ordinal":0,"tool_result":{"type":"tool_result","tool_use_id":"OTHER","content":"ok"}}),
        Some("OTHER"), None).await;
    let rows = repo_observed::events_correlated(&pool, sess, Some("u1"), None)
        .await
        .unwrap();
    let ids: Vec<&str> = rows.iter().map(|r| r.event_id.as_str()).collect();
    assert!(ids.contains(&"tres"), "transcript tool_result matched by tool_use_id column");
    assert!(!ids.contains(&"other"), "non-matching transcript tool_result excluded");
}

/// Column-based contract (Tier 2-2 perf refactor lock):
/// `events_correlated` must use the INDEXED `tool_use_id` / `request_id` columns
/// introduced by C2.  This test exercises all three participants:
///   - OTel `log_record` with tool_use_id column → INCLUDED
///   - transcript `tool_result` with tool_use_id column → INCLUDED
///   - transcript `tool_call` with tool_use_id column → EXCLUDED (deliberate;
///     the caller already holds the tool_call; returning it again would duplicate)
///   - transcript `assistant_message` with request_id column → EXCLUDED from the
///     request_id arm (old query never matched transcript kinds via request_id)
#[tokio::test]
async fn events_correlated_column_based_contract() {
    use serde_json::json;
    let pool = mem_pool().await;
    let run_id = repo_runs::start(&pool).await.unwrap();
    let sess = "sess-corr-col";

    // OTel log_record — tool_use_id column set by C2 ingest
    seed_payload_with_cols(&pool, &run_id, sess, 0, EventKind::LogRecord, "otel_log",
        json!({"event_name":"tool_result","attributes":{"tool_use_id":"tuid1","duration_ms":"42"}}),
        Some("tuid1"), None).await;
    // transcript tool_result — tool_use_id column set by mapping.rs
    seed_payload_with_cols(&pool, &run_id, sess, 1, EventKind::ToolResult, "t_result",
        json!({"content_ordinal":0,"tool_result":{"type":"tool_result","tool_use_id":"tuid1","content":"ok"}}),
        Some("tuid1"), None).await;
    // transcript tool_call — tool_use_id column set by mapping.rs, but MUST be excluded
    seed_payload_with_cols(&pool, &run_id, sess, 2, EventKind::ToolCall, "t_call",
        json!({"content_ordinal":0,"tool_name":"Bash","input":{}}),
        Some("tuid1"), None).await;
    // transcript assistant_message — request_id column set, but NOT matched by request_id arm
    seed_payload_with_cols(&pool, &run_id, sess, 3, EventKind::AssistantMessage, "a_msg",
        json!({"content_ordinal":0,"text":"hello","model":"claude-opus-4-5"}),
        None, Some("rid1")).await;
    // OTel log_record — request_id column set
    seed_payload_with_cols(&pool, &run_id, sess, 4, EventKind::LogRecord, "otel_req_log",
        json!({"event_name":"api_request","attributes":{"request_id":"rid1"}}),
        None, Some("rid1")).await;

    // tool_use_id arm: OTel log + transcript tool_result included; tool_call excluded
    let by_tuid = repo_observed::events_correlated(&pool, sess, Some("tuid1"), None)
        .await
        .unwrap();
    let tuid_ids: Vec<&str> = by_tuid.iter().map(|r| r.event_id.as_str()).collect();
    assert!(tuid_ids.contains(&"otel_log"), "OTel log_record must be included by tool_use_id column");
    assert!(tuid_ids.contains(&"t_result"), "transcript tool_result must be included by tool_use_id column");
    assert!(!tuid_ids.contains(&"t_call"), "transcript tool_call must be EXCLUDED (kind guard)");

    // request_id arm: OTel log_record included; transcript assistant_message excluded
    let by_rid = repo_observed::events_correlated(&pool, sess, None, Some("rid1"))
        .await
        .unwrap();
    let rid_ids: Vec<&str> = by_rid.iter().map(|r| r.event_id.as_str()).collect();
    assert!(rid_ids.contains(&"otel_req_log"), "OTel log_record must be included by request_id column");
    assert!(!rid_ids.contains(&"a_msg"), "transcript assistant_message must be EXCLUDED from request_id arm");
}

#[tokio::test]
async fn events_correlated_by_request_id_matches_api_log_and_span() {
    use serde_json::json;
    let pool = mem_pool().await;
    let run_id = repo_runs::start(&pool).await.unwrap();
    let sess = "sess-corr-req";
    // request_id column populated (C2 ingest now sets it for OTel log_records and otel_spans).
    seed_payload_with_cols(&pool, &run_id, sess, 0, EventKind::LogRecord, "apilog",
        json!({"event_name":"api_request","attributes":{"request_id":"r1","output_tokens":"2300"}}),
        None, Some("r1")).await;
    // Tier 3-1: otel_span events no longer re-embed `payload.raw_span`; the
    // span data lives in the telemetry facet and request_id is matched by the
    // INDEXED column (set here), not by payload JSON — so the stored payload is
    // the empty object the ingest path now produces.
    seed_payload_with_cols(&pool, &run_id, sess, 1, EventKind::OtelSpan, "span",
        json!({}), None, Some("r1")).await;
    seed_payload_with_cols(&pool, &run_id, sess, 2, EventKind::OtelSpan, "otherspan",
        json!({}), None, Some("r2")).await;
    let rows = repo_observed::events_correlated(&pool, sess, None, Some("r1"))
        .await
        .unwrap();
    let ids: Vec<&str> = rows.iter().map(|r| r.event_id.as_str()).collect();
    assert!(ids.contains(&"apilog"), "api_request log matched by request_id column");
    assert!(ids.contains(&"span"), "llm_request span matched by request_id column");
    assert!(!ids.contains(&"otherspan"), "non-matching span excluded");
}

// REGRESSION GUARD: the no-cursor window's `limit` bounds RENDERED events (not
// raw rows), AND telemetry INTERLEAVED between rendered events inside the window
// range must stay loaded — detail-panel metrics (buildToolMetricsFromEvents /
// buildLlmMetricsFromEvents) correlate that telemetry by tool_use_id /
// request_id out of exactly this window. Two failure modes this locks:
//   (a) an OFFSET off-by-one would over/under-count rendered events;
//   (b) a regression that narrowed the window to only rendered rows would
//       silently drop the interleaved telemetry → empty detail metrics.
#[tokio::test]
async fn no_cursor_window_bounds_rendered_count_and_keeps_interleaved_telemetry() {
    let pool = mem_pool().await;
    let run_id = repo_runs::start(&pool).await.unwrap();
    let sess = "sess-interleaved";
    // 3 rendered conversation events with telemetry interleaved between them:
    //   c0 , m1 , c1 , m2 , c2     (ASC by observed_at)
    seed_one(&pool, &run_id, sess, 0, EventKind::UserMessage, "c0").await;
    seed_one(&pool, &run_id, sess, 1, EventKind::MetricSample, "m1").await;
    seed_one(&pool, &run_id, sess, 2, EventKind::UserMessage, "c1").await;
    seed_one(&pool, &run_id, sess, 3, EventKind::MetricSample, "m2").await;
    seed_one(&pool, &run_id, sess, 4, EventKind::UserMessage, "c2").await;

    // limit = 2 rendered events → window = [ c1 .. c2 ], excluding the oldest
    // rendered (c0) and the telemetry (m1) that precedes the lower bound.
    let rows = repo_observed::list_session_window(&pool, sess, None, None, 2)
        .await
        .unwrap();
    let ids: Vec<&str> = rows.iter().map(|e| e.event_id.as_str()).collect();

    // exactly `limit` RENDERED events, not raw rows.
    let rendered = rows
        .iter()
        .filter(|e| e.kind == EventKind::UserMessage)
        .count();
    assert_eq!(rendered, 2, "limit must bound RENDERED events, got {ids:?}");
    assert!(ids.contains(&"c1") && ids.contains(&"c2"), "newest 2 rendered, got {ids:?}");
    assert!(!ids.contains(&"c0"), "the limit+1-th oldest rendered must be excluded (off-by-one)");

    // telemetry interleaved INSIDE the range stays loaded; telemetry before the
    // lower bound does not.
    assert!(ids.contains(&"m2"), "interleaved telemetry inside the window must be retained");
    assert!(!ids.contains(&"m1"), "telemetry before the window's lower bound must be excluded");

    // returned ASC → newest rendered event is last.
    assert_eq!(rows.last().unwrap().event_id, "c2");
}
