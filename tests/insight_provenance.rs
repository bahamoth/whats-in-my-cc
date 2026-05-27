//! Slice-14 — locks that L1 findings always have null judge, layer="L1",
//! and the correct extractor version stamp.

use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::migrate;

#[tokio::test]
async fn l1_finding_has_null_judge_and_l1_layer() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();

    // Insert a finding directly with the expected provenance shape
    let provenance = serde_json::json!({
        "extractor": "missing_verification@v1",
        "layer": "L1",
        "judge": null,
        "judge_template_version": null,
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
    assert!(prov["judge"].is_null(), "judge must be null for L1 finding");
    assert_eq!(
        prov["extractor"].as_str().unwrap(),
        "missing_verification@v1"
    );
}

#[tokio::test]
async fn pipeline_l1_findings_have_null_judge() {
    use witmcc::db::repo_diff_hunk::{self, NewDiffHunk};
    use witmcc::db::repo_episode::{self, EpisodeRow};

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();

    let sess = "sess_prov";

    // Seed observed events
    for (i, actor, kind, tool_use_id) in [
        (0usize, "user", "user_message", None::<&str>),
        (1, "assistant", "tool_call", Some("tid_0")),
        (2, "tool", "tool_result", Some("tid_0")),
    ] {
        let payload = if kind == "tool_result" {
            serde_json::json!({"tool_use_id":"tid_0","is_error":true,"content":"fail"}).to_string()
        } else if kind == "tool_call" {
            serde_json::json!({"tool_use_id":"tid_0","name":"Bash","input":{}}).to_string()
        } else {
            "{}".into()
        };

        sqlx::query(
            "INSERT OR IGNORE INTO raw_event (raw_event_id, source_type, payload, captured_at) \
             VALUES (?,?,?,?)",
        )
        .bind(format!("raw_p{i}"))
        .bind("claude_transcript")
        .bind("{}")
        .bind("2026-01-01T00:00:00Z")
        .execute(&pool)
        .await
        .unwrap();

        let q = if let Some(tuid) = tool_use_id {
            sqlx::query(
                "INSERT OR IGNORE INTO observed_event \
                 (event_id, raw_event_id, schema_version, session_id, observed_at, \
                  actor, kind, tool_name, tool_use_id, parser_version, payload) \
                 VALUES (?,?,?,?,?,?,?,?,?,?,?)",
            )
            .bind(format!("ev_p{i}"))
            .bind(format!("raw_p{i}"))
            .bind("observed_event.v1")
            .bind(sess)
            .bind(format!("2026-01-01T00:00:{i:02}Z"))
            .bind(actor)
            .bind(kind)
            .bind("Bash")
            .bind(tuid)
            .bind("test")
            .bind(payload)
            .execute(&pool)
            .await
            .unwrap()
        } else {
            sqlx::query(
                "INSERT OR IGNORE INTO observed_event \
                 (event_id, raw_event_id, schema_version, session_id, observed_at, \
                  actor, kind, parser_version, payload) \
                 VALUES (?,?,?,?,?,?,?,?,?)",
            )
            .bind(format!("ev_p{i}"))
            .bind(format!("raw_p{i}"))
            .bind("observed_event.v1")
            .bind(sess)
            .bind(format!("2026-01-01T00:00:{i:02}Z"))
            .bind(actor)
            .bind(kind)
            .bind("test")
            .bind(payload)
            .execute(&pool)
            .await
            .unwrap()
        };
        let _ = q;
    }

    // diff hunk + episodes
    repo_diff_hunk::insert(&pool, &NewDiffHunk {
        diff_hunk_id: "dh_p001".into(),
        schema_version: "diff_hunk.v1".into(),
        session_id: sess.into(),
        file_path: "src/lib.rs".into(),
        change_type: "modify".into(),
        line_range_after_start: Some(1),
        line_range_after_end: Some(2),
        introduced_by_event_id: "ev_p1".into(),
        introduced_by_tool_use_id: Some("tid_0".into()),
        patch_preview: "+".into(),
        lines_added: 1,
        lines_removed: 0,
        user_modified: false,
    }).await.unwrap();

    let mk_ep = |eid: &str, phase: &str, start: &str, end: &str| EpisodeRow {
        episode_id: eid.into(),
        schema_version: "episode.v1".into(),
        session_id: sess.into(),
        phase: phase.into(),
        start_event_id: start.into(),
        end_event_id: end.into(),
        started_at: "2026-01-01T00:00:00Z".into(),
        ended_at: "2026-01-01T00:00:30Z".into(),
        evidence_node_ids: "[]".into(),
        classification_basis: "[]".into(),
        confidence: 0.9,
        summary: None,
        classifier_version: "episode_classifier@v1".into(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    repo_episode::insert(&pool, &mk_ep("ep_p001", "intake", "ev_p0", "ev_p0")).await.unwrap();
    repo_episode::insert(&pool, &mk_ep("ep_p002", "action", "ev_p1", "ev_p2")).await.unwrap();

    witmcc::insight::pipeline::run_extractors(&pool, sess).await.unwrap();

    let provenance_rows: Vec<(String,)> =
        sqlx::query_as("SELECT provenance FROM finding WHERE session_id = ?")
            .bind(sess)
            .fetch_all(&pool)
            .await
            .unwrap();

    assert!(!provenance_rows.is_empty(), "pipeline must produce at least one finding");

    for (prov_str,) in &provenance_rows {
        let prov: serde_json::Value = serde_json::from_str(prov_str).unwrap();
        assert_eq!(prov["layer"].as_str().unwrap(), "L1",
            "all pipeline-generated findings must have layer=L1");
        assert!(prov["judge"].is_null(),
            "all pipeline-generated L1 findings must have null judge");
    }
}
