//! Slice 2 (telemetry fold) — real-fixture backbone invariant.
//!
//! After building the graph for a real session, `graph_node` must contain ZERO
//! nodes of the orphan telemetry kinds {metric_sample, hook_event, otel_span,
//! log_record}, while the conversation/action backbone survives.
//!
//! Two real anchors:
//!   - `transcripts/real/verification_v01.jsonl` — a real Claude Code transcript
//!     carrying the backbone (user/assistant messages). Asserts backbone present
//!     and no telemetry node kinds.
//!   - `otel/real/logs_v01.json` — real OTLP logs carrying *orphan* log records
//!     (hook_execution_complete + mcp_server_connection — none of which is a
//!     foldable tool_result/tool_decision/api_request). Asserts the log records
//!     land in observed_event (SSOT) yet produce ZERO log_record graph nodes
//!     after the Slice-2 drop.
//!
//! Real-data anchoring: the orphan log records (hook/mcp) are frozen real
//! payloads in logs_v01.json; the backbone messages are a frozen real transcript.

use axum_test::TestServer;
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::{migrate, repo_observed};
use wimcc::graph::build;
use wimcc::ingest::store;
use wimcc::live::NoopSink;

const ORPHAN_TELEMETRY_KINDS: &[&str] = &["metric_sample", "hook_event", "otel_span", "log_record"];
const BACKBONE_KINDS: &[&str] = &[
    "user_message",
    "assistant_message",
    "tool_call",
    "verification_run",
    "diff_hunk",
];

async fn make_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

async fn node_kinds(pool: &sqlx::SqlitePool, session_id: &str) -> Vec<String> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT node_kind FROM graph_node WHERE session_id = ?")
            .bind(session_id)
            .fetch_all(pool)
            .await
            .unwrap();
    rows.into_iter().map(|r| r.0).collect()
}

#[tokio::test]
async fn real_transcript_graph_has_backbone_and_no_telemetry_nodes() {
    let pool = make_pool().await;
    let path = "tests/fixtures/transcripts/real/verification_v01.jsonl";
    store::ingest_file(&pool, std::path::Path::new(path), &NoopSink)
        .await
        .unwrap();

    let sessions = repo_observed::list_sessions(&pool, 10).await.unwrap();
    assert!(!sessions.is_empty(), "fixture produced no sessions");
    let sid = sessions[0].session_id.clone();

    // Rebuild graph from observed_event/side-tables.
    build::rebuild_session(&pool, &sid).await.unwrap();

    let kinds = node_kinds(&pool, &sid).await;
    assert!(!kinds.is_empty(), "real transcript must yield backbone nodes");

    for telem in ORPHAN_TELEMETRY_KINDS {
        assert!(
            !kinds.iter().any(|k| k == telem),
            "graph_node must contain NO {telem} after Slice-2 drop; got {kinds:?}"
        );
    }
    // Backbone present: every surviving node is a backbone kind, and at least
    // one backbone node exists. (verification_v01 surfaces tool_call +
    // verification_run; messages carrying only tool blocks fold into those.)
    assert!(
        kinds.iter().any(|k| BACKBONE_KINDS.contains(&k.as_str())),
        "at least one backbone node must remain; got {kinds:?}"
    );
    assert!(
        kinds.iter().all(|k| BACKBONE_KINDS.contains(&k.as_str())),
        "every surviving node must be a backbone kind; got {kinds:?}"
    );
}

#[tokio::test]
async fn real_orphan_logs_stay_in_observed_event_but_drop_from_graph() {
    // Ingest the real OTLP logs fixture (orphan hook/mcp log records) via the
    // HTTP receiver, which also rebuilds the touched session's graph.
    let pool = make_pool().await;
    let app = wimcc::api::router(wimcc::api::AppState::new_for_tests(pool.clone()));
    let server = TestServer::new(app).unwrap();

    let body = std::fs::read("tests/fixtures/otel/real/logs_v01.json").unwrap();
    let resp = server
        .post("/otel/v1/logs")
        .add_header("content-type", "application/json")
        .bytes(body.into())
        .await;
    resp.assert_status_ok();
    let v: serde_json::Value = resp.json();
    let accepted = v["data"]["accepted_log_records"].as_u64().unwrap_or(0);
    assert!(accepted >= 1, "real logs fixture has >=1 orphan log record");
    let session_id = v["data"]["sessions_touched"][0].as_str().unwrap().to_string();

    // SSOT preserved: every accepted log record is an observed_event row.
    let observed: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM observed_event WHERE kind = 'log_record'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        observed.0, accepted as i64,
        "orphan log records must remain in observed_event (SSOT)"
    );

    // Graph: ZERO orphan telemetry nodes after the Slice-2 drop.
    let kinds = node_kinds(&pool, &session_id).await;
    for telem in ORPHAN_TELEMETRY_KINDS {
        assert!(
            !kinds.iter().any(|k| k == telem),
            "graph_node must contain NO {telem} after Slice-2 drop; got {kinds:?}"
        );
    }
}
