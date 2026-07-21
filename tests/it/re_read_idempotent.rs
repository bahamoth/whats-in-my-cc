//! Regression test for the re_read signal non-idempotency bug found during
//! OTel-session dogfooding (2026-06-11).
//!
//! `signal_id` was derived from the full `evidence_refs` list. For aggregating
//! detectors like `re_read`, the evidence set GROWS as more reads accumulate
//! across (re-)ingests, so each growth produced a NEW `signal_id` and the older
//! signal rows were never replaced — they piled up (observed: 19 rows for a
//! single file, 154 rows collapsing to 53 distinct files).
//!
//! Fix: re_read carries a stable `dedup_key` (file_path) so the derived
//! `signal_id` is stable across read-count growth (INSERT OR REPLACE updates the
//! same row), and `run_detectors` reconciles each (session, detector) by deleting
//! stored signals absent from the current pass (self-heals existing duplicates).
//!
//! FK enforcement is disabled for the test pool (synthetic rows), mirroring
//! tests/insight_pipeline.rs.

use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::migrate;

const SESS: &str = "sess_reread";

async fn pool() -> sqlx::SqlitePool {
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
    pool
}

/// Insert one Read tool_call observed_event for `file_path` with the given index.
async fn insert_read(pool: &sqlx::SqlitePool, i: usize, file_path: &str) {
    let raw_id = format!("raw_rr_{i:03}");
    let ev_id = format!("ev_rr_{i:03}");
    let ts = format!("2026-06-11T00:00:{i:02}Z");
    sqlx::query(
        "INSERT OR IGNORE INTO raw_event \
         (raw_event_id, ingest_run_id, source_type, source_uri, source_line_no, \
          source_byte_offset, payload_sha256, payload, captured_at) \
         VALUES (?,?,?,?,?,?,?,?,?)",
    )
    .bind(&raw_id)
    .bind("run_0")
    .bind("claude_transcript")
    .bind("test.jsonl")
    .bind(i as i64)
    .bind(0i64)
    .bind(format!("sha_{i}"))
    .bind(b"{}" as &[u8])
    .bind(&ts)
    .execute(pool)
    .await
    .unwrap();

    let payload = format!(r#"{{"tool_name":"Read","input":{{"file_path":"{file_path}"}}}}"#);
    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id, raw_event_id, schema_version, session_id, observed_at, \
          actor, kind, tool_name, parser_version, payload) \
         VALUES (?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&ev_id)
    .bind(&raw_id)
    .bind("observed_event.v1")
    .bind(SESS)
    .bind(&ts)
    .bind("assistant")
    .bind("tool_call")
    .bind("Read")
    .bind("test")
    .bind(&payload)
    .execute(pool)
    .await
    .unwrap();
}

async fn re_read_rows(pool: &sqlx::SqlitePool) -> Vec<(String, i64)> {
    sqlx::query_as::<_, (String, i64)>(
        "SELECT signal_id, json_extract(facts, '$.read_count') \
         FROM signal WHERE detector='re_read' AND session_id=?",
    )
    .bind(SESS)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn re_read_signal_stable_across_growing_reads() {
    let pool = pool().await;

    // First pass: file read twice → one re_read signal (read_count=2).
    insert_read(&pool, 0, "src/a.rs").await;
    insert_read(&pool, 1, "src/a.rs").await;
    wimcc::insight::pipeline::run_detectors(&pool, SESS)
        .await
        .unwrap();

    let after_first = re_read_rows(&pool).await;
    assert_eq!(
        after_first.len(),
        1,
        "first pass must produce exactly one re_read signal for src/a.rs"
    );

    // Second pass simulating re-ingest: a 3rd read of the SAME file arrives.
    // The evidence set grows (r0,r1 -> r0,r1,r2); the signal must REMAIN a single
    // row with the updated read_count, not spawn a second row.
    insert_read(&pool, 2, "src/a.rs").await;
    wimcc::insight::pipeline::run_detectors(&pool, SESS)
        .await
        .unwrap();

    let after_second = re_read_rows(&pool).await;
    assert_eq!(
        after_second.len(),
        1,
        "growing reads of the same file must NOT create duplicate re_read signals \
         (got {} rows: {:?})",
        after_second.len(),
        after_second
    );
    assert_eq!(
        after_second[0].1, 3,
        "the single re_read signal must reflect the latest read_count (3)"
    );
}

#[tokio::test]
async fn run_detectors_reconciles_stale_signals() {
    let pool = pool().await;

    // Pre-seed a stale re_read signal that the current pass will NOT produce
    // (no events back it). Reconciliation must delete it.
    sqlx::query(
        "INSERT INTO signal \
         (signal_id, schema_version, session_id, detector, subkind, summary, \
          evidence_refs, facts, provenance, created_at) \
         VALUES (?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("sig_stale_phantom")
    .bind("signal.v1")
    .bind(SESS)
    .bind("re_read")
    .bind(Option::<String>::None)
    .bind("phantom stale signal")
    .bind(r#"["ev_ghost"]"#)
    .bind(r#"{"file_path":"src/ghost.rs","read_count":9}"#)
    .bind(r#"{"detector":"re_read@v1","version":"L1","rule_pack":null}"#)
    .bind("2026-06-11T00:00:00Z")
    .execute(&pool)
    .await
    .unwrap();

    // A real re_read situation for a different file.
    insert_read(&pool, 0, "src/real.rs").await;
    insert_read(&pool, 1, "src/real.rs").await;

    wimcc::insight::pipeline::run_detectors(&pool, SESS)
        .await
        .unwrap();

    let stale_survives: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM signal WHERE signal_id='sig_stale_phantom'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        stale_survives, 0,
        "run_detectors must reconcile away stale re_read signals not in the current pass"
    );

    let rows = re_read_rows(&pool).await;
    assert_eq!(
        rows.len(),
        1,
        "only the current-pass re_read signal (src/real.rs) must remain"
    );
}
