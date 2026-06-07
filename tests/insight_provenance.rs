//! Slice-14 / judge-removal — locks the L1 finding provenance shape: an
//! `<extractor>@v1` stamp, `layer="L1"`, and NO judge fields. The judge
//! subsystem was deleted, so `judge` / `judge_template_version` are no longer
//! emitted at all (not present-as-null). `.get(k).is_none()` distinguishes an
//! absent key from a present-null one — `value[k].is_null()` would pass for
//! both and so would not lock the removal.

use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::migrate;

#[tokio::test]
async fn l1_finding_provenance_shape_has_no_judge_fields() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();

    // Insert a finding directly with the canonical post-judge provenance shape.
    // No FK checking needed (finding table has no FKs).
    let provenance = serde_json::json!({
        "extractor": "missing_verification@v1",
        "layer": "L1",
        "rule_pack": null
    });
    sqlx::query(
        "INSERT INTO finding \
         (finding_id, session_id, category, severity, confidence, summary, \
          evidence_refs, evidence_projection, provenance, status) \
         VALUES (?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("find_demo_001")
    .bind("sess_x")
    .bind("missing_verification")
    .bind("medium")
    .bind(0.9_f64)
    .bind("test")
    .bind(r#"["ev_001"]"#)
    .bind("{}")
    .bind(provenance.to_string())
    .bind("active")
    .execute(&pool)
    .await
    .unwrap();

    let row: (String,) = sqlx::query_as(
        "SELECT provenance FROM finding WHERE finding_id='find_demo_001'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let prov: serde_json::Value = serde_json::from_str(&row.0).unwrap();
    assert_eq!(prov["layer"].as_str().unwrap(), "L1");
    assert!(
        prov.get("judge").is_none(),
        "judge field must be absent for L1 finding (judge subsystem removed), got: {prov}"
    );
    assert!(
        prov.get("judge_template_version").is_none(),
        "judge_template_version field must be absent for L1 finding, got: {prov}"
    );
    assert_eq!(
        prov["extractor"].as_str().unwrap(),
        "missing_verification@v1"
    );
}

/// Pipeline-generated L1 findings must carry the right provenance: layer="L1",
/// NO judge fields, and an `<extractor>@v1` stamp matching the firing extractor.
///
/// We drive the deterministic `tool_failure` extractor: a Bash tool_call whose
/// paired tool_result has `is_error=true` and no compensating successful retry
/// (`tool_failure` is L1/Always → confidence 1.0, judge never consulted).
#[tokio::test]
async fn pipeline_l1_findings_omit_judge_fields() {
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
    .bind(r#"{"tool_result":{"tool_use_id":"tid_p0","is_error":true,"content":"compile error E0001"}}"#)
    .execute(&pool).await.unwrap();

    wimcc::insight::pipeline::run_extractors(&pool, sess).await.unwrap();

    let provenance_rows: Vec<(String,)> =
        sqlx::query_as("SELECT provenance FROM finding WHERE session_id = ?")
            .bind(sess)
            .fetch_all(&pool)
            .await
            .unwrap();

    assert!(!provenance_rows.is_empty(), "pipeline must produce at least one finding");

    // The deterministic tool_failure finding must be present with correct provenance.
    let mut saw_tool_failure = false;
    for (prov_str,) in &provenance_rows {
        let prov: serde_json::Value = serde_json::from_str(prov_str).unwrap();
        assert_eq!(prov["layer"].as_str().unwrap(), "L1",
            "all pipeline-generated findings must have layer=L1");
        assert!(prov.get("judge").is_none(),
            "pipeline L1 findings must omit the judge field entirely, got: {prov}");
        assert!(prov.get("judge_template_version").is_none(),
            "pipeline L1 findings must omit judge_template_version, got: {prov}");
        if prov["extractor"].as_str() == Some("tool_failure@v1") {
            saw_tool_failure = true;
        }
    }
    assert!(saw_tool_failure,
        "expected a finding stamped with extractor=tool_failure@v1 in provenance");
}
