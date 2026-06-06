//! Phase A — TDD red test for L1-only deterministic promotion.
//!
//! On CURRENT code: `risky_action` uses `PromotionPolicy::IfAbove(1.0)`, so
//! confidence_l1 (0.7) never exceeds 1.0, the candidate always routes to the
//! noop judge, and ends up in `findings_pending_judge` forever — 0 active findings.
//!
//! After A2+A3 (PromotionPolicy removed, pipeline rewritten to L1-only):
//! `risky_action` promotes directly; this test passes.

use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::migrate;

/// Build a migrated in-memory pool with FK enforcement off, seeded with a
/// single `rm -rf /tmp/foo` Bash tool_call that triggers `risky_action`.
async fn pool_with_destructive_bash() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();

    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&pool)
        .await
        .unwrap();

    migrate(&pool).await.unwrap();

    let sess = "sess_l1";
    let ts = "2026-01-01T00:00:00Z";

    // Minimal raw_event row (FK check disabled).
    sqlx::query(
        "INSERT OR IGNORE INTO raw_event \
         (raw_event_id, ingest_run_id, source_type, source_uri, source_line_no, \
          source_byte_offset, payload_sha256, payload, captured_at) \
         VALUES (?,?,?,?,?,?,?,?,?)",
    )
    .bind("raw_000")
    .bind("run_0")
    .bind("claude_transcript")
    .bind("test.jsonl")
    .bind(0i64)
    .bind(0i64)
    .bind("sha256_000")
    .bind(b"{}" as &[u8])
    .bind(ts)
    .execute(&pool)
    .await
    .unwrap();

    // observed_event: assistant tool_call for Bash with `rm -rf /tmp/foo`
    // Payload shape mirrors what `risky_action` extractor reads via
    // `payload.pointer("/tool_use/input/command")`.
    let call_payload = serde_json::to_string(&serde_json::json!({
        "tool_use": {
            "name": "Bash",
            "input": { "command": "rm -rf /tmp/foo" }
        }
    }))
    .unwrap();

    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id, raw_event_id, schema_version, session_id, observed_at, \
          actor, kind, tool_name, tool_use_id, is_sidechain, is_meta, payload, parser_version) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("ev_000")
    .bind("raw_000")
    .bind("observed_event.v1")
    .bind(sess)
    .bind(ts)
    .bind("assistant")
    .bind("tool_call")
    .bind("Bash")
    .bind("tu_000")
    .bind(0i64)
    .bind(0i64)
    .bind(&call_payload)
    .bind("v1")
    .execute(&pool)
    .await
    .unwrap();

    pool
}

#[tokio::test]
async fn risky_action_promotes_without_judge() {
    let pool = pool_with_destructive_bash().await;

    let rows = wimcc::insight::pipeline::run_extractors(&pool, "sess_l1")
        .await
        .unwrap();

    let risky: Vec<_> = rows.iter().filter(|r| r.category == "risky_action").collect();
    assert!(
        !risky.is_empty(),
        "risky_action must promote at L1 without a judge; got 0 active findings (rows={:?})",
        rows.iter().map(|r| &r.category).collect::<Vec<_>>()
    );
    assert_eq!(
        risky[0].status, "active",
        "risky_action finding must have status=active"
    );
    assert!(
        risky[0].provenance.contains("\"layer\":\"L1\""),
        "risky_action finding must have provenance.layer=L1, got: {}",
        risky[0].provenance
    );
}
