//! Locks the signal provenance shape (Plan 1: finding → signal): a
//! `<detector>@v1` stamp, `version="L1"`, and NO judge fields. The judge
//! subsystem was deleted, so `judge` / `judge_template_version` are never
//! emitted. `.get(k).is_none()` distinguishes an absent key from a present-null
//! one — `value[k].is_null()` would pass for both and so would not lock removal.

use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::migrate;

#[tokio::test]
async fn signal_provenance_shape_has_no_judge_fields() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();

    // Insert a signal directly with the canonical provenance shape.
    let provenance = serde_json::json!({
        "detector": "tool_failure@v1",
        "version": "L1",
        "rule_pack": null
    });
    sqlx::query(
        "INSERT INTO signal \
         (signal_id, session_id, detector, summary, evidence_refs, facts, provenance) \
         VALUES (?,?,?,?,?,?,?)",
    )
    .bind("sig_demo_001")
    .bind("sess_x")
    .bind("tool_failure")
    .bind("test")
    .bind(r#"["ev_001"]"#)
    .bind("{}")
    .bind(provenance.to_string())
    .execute(&pool)
    .await
    .unwrap();

    let row: (String,) =
        sqlx::query_as("SELECT provenance FROM signal WHERE signal_id='sig_demo_001'")
            .fetch_one(&pool)
            .await
            .unwrap();

    let prov: serde_json::Value = serde_json::from_str(&row.0).unwrap();
    assert_eq!(prov["version"].as_str().unwrap(), "L1");
    assert!(
        prov.get("judge").is_none(),
        "judge field must be absent (judge subsystem removed), got: {prov}"
    );
    assert!(
        prov.get("judge_template_version").is_none(),
        "judge_template_version field must be absent, got: {prov}"
    );
    assert_eq!(prov["detector"].as_str().unwrap(), "tool_failure@v1");
}

/// Pipeline-generated signals must carry the right provenance: version="L1",
/// NO judge fields, and a `<detector>@v1` stamp matching the firing detector.
///
/// We drive the deterministic `tool_failure` detector: a Bash tool_call whose
/// paired tool_result resolves to Failed (Plan 6: via the explicit "exit code: 1"
/// in content → Tier-3 structural parse, Measured). Judge never consulted.
#[tokio::test]
async fn pipeline_signals_omit_judge_fields() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();

    // Disable FK enforcement for synthetic test data.
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&pool)
        .await
        .unwrap();

    migrate(&pool).await.unwrap();

    let sess = "sess_prov";
    let ts = |i: usize| format!("2026-01-01T00:00:{:02}Z", i);

    // Seed minimal raw rows for the two events below.
    for i in 0..2usize {
        sqlx::query(
            "INSERT OR IGNORE INTO raw_event \
             (raw_event_id, ingest_run_id, source_type, source_uri, source_line_no, \
              source_byte_offset, payload_sha256, payload, captured_at) \
             VALUES (?,?,?,?,?,?,?,?,?)"
        )
        .bind(format!("raw_p{i}"))
        .bind("run_0")
        .bind("claude_transcript")
        .bind("test.jsonl")
        .bind(i as i64)
        .bind(0i64)
        .bind(format!("sha_{i}"))
        .bind(b"{}" as &[u8])
        .bind(&ts(i))
        .execute(&pool).await.unwrap();
    }

    // ev_p0: assistant Bash tool_call.
    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id, raw_event_id, schema_version, session_id, observed_at, \
          actor, kind, tool_name, tool_use_id, parser_version, payload) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?)"
    )
    .bind("ev_p0").bind("raw_p0")
    .bind("observed_event.v1").bind(sess)
    .bind(ts(0))
    .bind("assistant").bind("tool_call").bind("Bash").bind("tid_p0")
    .bind("test")
    .bind(r#"{"tool_use_id":"tid_p0","name":"Bash","input":{"command":"cargo test"}}"#)
    .execute(&pool).await.unwrap();

    // ev_p1: failing tool_result for the same tool_use_id (no successful retry follows).
    // Plan 6: ToolFailure fires on resolve_outcome==Failed. The "exit code: 1" in
    // content drives Tier-3 (structural parse) → Failed/Measured. is_error=true is
    // retained as a tool-execution fact but is no longer the trigger.
    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id, raw_event_id, schema_version, session_id, observed_at, \
          actor, kind, tool_use_id, parser_version, payload) \
         VALUES (?,?,?,?,?,?,?,?,?,?)"
    )
    .bind("ev_p1").bind("raw_p1")
    .bind("observed_event.v1").bind(sess)
    .bind(ts(1))
    .bind("tool").bind("tool_result").bind("tid_p0")
    .bind("test")
    .bind(r#"{"tool_result":{"tool_use_id":"tid_p0","is_error":true,"content":"compile error E0001\nexit code: 1"}}"#)
    .execute(&pool).await.unwrap();

    wimcc::insight::pipeline::run_detectors(&pool, sess).await.unwrap();

    let provenance_rows: Vec<(String,)> =
        sqlx::query_as("SELECT provenance FROM signal WHERE session_id = ?")
            .bind(sess)
            .fetch_all(&pool)
            .await
            .unwrap();

    assert!(!provenance_rows.is_empty(), "pipeline must produce at least one signal");

    // The deterministic tool_failure signal must be present with correct provenance.
    let mut saw_tool_failure = false;
    for (prov_str,) in &provenance_rows {
        let prov: serde_json::Value = serde_json::from_str(prov_str).unwrap();
        assert_eq!(prov["version"].as_str().unwrap(), "L1",
            "all pipeline-generated signals must have version=L1");
        assert!(prov.get("judge").is_none(),
            "pipeline signals must omit the judge field entirely, got: {prov}");
        assert!(prov.get("judge_template_version").is_none(),
            "pipeline signals must omit judge_template_version, got: {prov}");
        if prov["detector"].as_str() == Some("tool_failure@v1") {
            saw_tool_failure = true;
        }
    }
    assert!(saw_tool_failure,
        "expected a signal stamped with detector=tool_failure@v1 in provenance");
}
