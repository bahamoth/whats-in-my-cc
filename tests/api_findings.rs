//! Slice-14 — HTTP route tests for /v1/findings*, /v1/sessions/:id/findings,
//! and /v1/findings/:id/evidence.

use axum_test::TestServer;
use sqlx::sqlite::SqlitePoolOptions;
use serde_json::Value;
use witmcc::api::AppState;
use witmcc::db::migrate;

async fn pool_with_seeded_findings() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();

    // Seed two findings directly (one per category)
    for (fid, cat, sev, conf, summary) in [
        ("find_demo_001", "missing_verification", "medium", 0.9_f64,
         "Action episode ep_001 had no following verification."),
        ("find_demo_002", "tool_failure", "high", 1.0_f64,
         "Tool Bash failed with is_error=true."),
    ] {
        sqlx::query(
            "INSERT OR IGNORE INTO finding \
             (finding_id, session_id, category, severity, confidence, summary, \
              evidence_refs, evidence_projection, provenance, status) \
             VALUES (?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(fid)
        .bind("sess_demo")
        .bind(cat)
        .bind(sev)
        .bind(conf)
        .bind(summary)
        .bind(r#"["ev_001","ev_002"]"#)
        .bind(r#"{"category":"test"}"#)
        .bind(r#"{"extractor":"test@v1","layer":"L1","judge":null}"#)
        .bind("active")
        .execute(&pool)
        .await
        .unwrap();
    }

    pool
}

async fn pool_with_seeded_findings_and_graph() -> sqlx::SqlitePool {
    let pool = pool_with_seeded_findings().await;

    // Seed a raw event + observed_event so evidence endpoint can return raw refs
    sqlx::query(
        "INSERT OR IGNORE INTO raw_event (raw_event_id, source_type, payload, captured_at) \
         VALUES (?,?,?,?)",
    )
    .bind("raw_001")
    .bind("claude_transcript")
    .bind("{}")
    .bind("2026-01-01T00:00:00Z")
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id, raw_event_id, schema_version, session_id, observed_at, actor, kind, parser_version, payload) \
         VALUES (?,?,?,?,?,?,?,?,?)",
    )
    .bind("ev_001")
    .bind("raw_001")
    .bind("observed_event.v1")
    .bind("sess_demo")
    .bind("2026-01-01T00:00:00Z")
    .bind("tool")
    .bind("tool_result")
    .bind("test")
    .bind("{}")
    .execute(&pool)
    .await
    .unwrap();

    // Seed a graph node for ev_001
    sqlx::query(
        "INSERT OR IGNORE INTO graph_node \
         (node_id, session_id, node_kind, source_event_ids, label, data, created_at) \
         VALUES (?,?,?,?,?,?,?)",
    )
    .bind("node_001")
    .bind("sess_demo")
    .bind("tool_result")
    .bind(r#"["ev_001"]"#)
    .bind("tool_result")
    .bind("{}")
    .bind("2026-01-01T00:00:00Z")
    .execute(&pool)
    .await
    .unwrap();

    pool
}

fn build_server(pool: sqlx::SqlitePool) -> TestServer {
    let state = AppState::new_for_tests(pool);
    let router = witmcc::api::router(state);
    TestServer::new(router).unwrap()
}

#[tokio::test]
async fn list_findings_endpoint() {
    let pool = pool_with_seeded_findings().await;
    let server = build_server(pool);
    let r = server.get("/v1/findings").await;
    r.assert_status_ok();
    let body: Value = r.json();
    let data = body["data"].as_array().unwrap();
    assert!(
        data.len() >= 2,
        "expected at least 2 findings, got {}",
        data.len()
    );
}

#[tokio::test]
async fn list_findings_filter_by_category() {
    let pool = pool_with_seeded_findings().await;
    let server = build_server(pool);
    let r = server.get("/v1/findings?category=tool_failure").await;
    r.assert_status_ok();
    let body: Value = r.json();
    let data = body["data"].as_array().unwrap();
    assert!(!data.is_empty(), "expected at least 1 tool_failure finding");
    for f in data {
        assert_eq!(f["category"].as_str().unwrap(), "tool_failure");
    }
}

#[tokio::test]
async fn finding_detail_includes_evidence_projection_and_provenance() {
    let pool = pool_with_seeded_findings().await;
    let server = build_server(pool);
    let r = server.get("/v1/findings/find_demo_001").await;
    r.assert_status_ok();
    let body: Value = r.json();
    let d = &body["data"];
    assert_eq!(d["provenance"]["layer"].as_str().unwrap(), "L1");
    assert!(d["evidence_projection"].is_object(), "evidence_projection must be an object");
    assert!(d["provenance"]["judge"].is_null(), "judge must be null for L1");
}

#[tokio::test]
async fn finding_detail_404_for_unknown_id() {
    let pool = pool_with_seeded_findings().await;
    let server = build_server(pool);
    let r = server.get("/v1/findings/find_does_not_exist").await;
    r.assert_status(axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn evidence_endpoint_returns_subgraph_and_raw_refs() {
    let pool = pool_with_seeded_findings_and_graph().await;
    let server = build_server(pool);
    let r = server.get("/v1/findings/find_demo_001/evidence").await;
    r.assert_status_ok();
    let body: Value = r.json();
    let d = &body["data"];
    let nodes = d["subgraph"]["nodes"].as_array().unwrap();
    assert!(
        !nodes.is_empty(),
        "subgraph.nodes must be non-empty; got 0"
    );
    let refs = d["raw_source_refs"].as_array().unwrap();
    assert!(
        !refs.is_empty(),
        "raw_source_refs must be non-empty; got 0"
    );
}

#[tokio::test]
async fn session_findings_alias_returns_same_rows() {
    let pool = pool_with_seeded_findings().await;
    let server = build_server(pool);
    let r1 = server.get("/v1/sessions/sess_demo/findings").await;
    r1.assert_status_ok();
    let r2 = server.get("/v1/findings?session_id=sess_demo").await;
    r2.assert_status_ok();
    let d1 = r1.json::<Value>()["data"].as_array().unwrap().len();
    let d2 = r2.json::<Value>()["data"].as_array().unwrap().len();
    assert_eq!(d1, d2, "session alias and filter must return the same count");
}
