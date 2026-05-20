use axum_test::TestServer;
use chrono::Utc;
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::migrate;
use witmcc::graph::build;
use witmcc::ingest::otel;

async fn make_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

fn fixture(path: &str) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[tokio::test]
async fn store_single_span_inserts_one_observed_event() {
    let pool = make_pool().await;
    let body = fixture("tests/fixtures/otel/single_span.json");
    let parsed = otel::parse_otlp_json(&body);
    let res = otel::store(&pool, parsed, Utc::now()).await.unwrap();
    assert_eq!(res.accepted_spans, 1);
    assert_eq!(res.rejected_spans, 0);
    assert_eq!(res.sessions_touched, vec!["sess-otel-A".to_string()]);

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM observed_event WHERE kind = 'otel_span'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count.0, 1);

    let row: (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT trace_id, span_id FROM observed_event WHERE kind = 'otel_span' LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0.as_deref(), Some("5b8aa5a2d2c872e8321cf37308d69df2"));
    assert_eq!(row.1.as_deref(), Some("051581bf3cb55c13"));
}

#[tokio::test]
async fn store_is_idempotent() {
    let pool = make_pool().await;
    let body = fixture("tests/fixtures/otel/single_span.json");
    let r1 = otel::store(&pool, otel::parse_otlp_json(&body), Utc::now())
        .await
        .unwrap();
    let r2 = otel::store(&pool, otel::parse_otlp_json(&body), Utc::now())
        .await
        .unwrap();
    assert_eq!(r1.accepted_spans, 1);
    assert_eq!(r2.accepted_spans, 0);
    assert_eq!(r2.duplicate_spans, 1);

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM observed_event WHERE kind = 'otel_span'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count.0, 1);
}

#[tokio::test]
async fn graph_has_otel_span_node_after_ingest() {
    let pool = make_pool().await;
    let body = fixture("tests/fixtures/otel/parent_child.json");
    otel::store(&pool, otel::parse_otlp_json(&body), Utc::now())
        .await
        .unwrap();
    let (n, e) = build::rebuild_session(&pool, "sess-otel-B").await.unwrap();
    assert_eq!(n, 2, "two otel_span nodes from parent_child fixture");
    assert_eq!(e, 0, "no edges emitted in slice-3");

    let row: (String,) = sqlx::query_as(
        "SELECT node_kind FROM graph_node WHERE session_id = ? ORDER BY started_at LIMIT 1",
    )
    .bind("sess-otel-B")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "otel_span");
}

async fn http_setup() -> TestServer {
    let pool = make_pool().await;
    let app = witmcc::api::router(pool);
    TestServer::new(app).unwrap()
}

#[tokio::test]
async fn post_traces_returns_accepted_count() {
    let s = http_setup().await;
    let body = fixture("tests/fixtures/otel/single_span.json");
    let resp = s
        .post("/otel/v1/traces")
        .json(&body)
        .await;
    resp.assert_status_ok();
    let v: serde_json::Value = resp.json();
    assert_eq!(v["meta"]["schema_version"], "0.2.0");
    assert_eq!(v["data"]["accepted_spans"], 1);
    assert_eq!(v["data"]["rejected_spans"], 0);
    assert_eq!(v["data"]["sessions_touched"][0], "sess-otel-A");
}

#[tokio::test]
async fn post_traces_with_malformed_trace_id_rejects_span() {
    let s = http_setup().await;
    let body = fixture("tests/fixtures/otel/malformed_traceid.json");
    let resp = s.post("/otel/v1/traces").json(&body).await;
    resp.assert_status_ok();
    let v: serde_json::Value = resp.json();
    assert_eq!(v["data"]["accepted_spans"], 0);
    assert_eq!(v["data"]["rejected_spans"], 1);
}

#[tokio::test]
async fn post_traces_with_non_json_body_is_400() {
    let s = http_setup().await;
    let resp = s
        .post("/otel/v1/traces")
        .add_header("content-type", "application/json")
        .text("not json")
        .await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
}

use std::io::Write;

fn gzip(bytes: &[u8]) -> Vec<u8> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(bytes).unwrap();
    enc.finish().unwrap()
}

#[tokio::test]
async fn post_traces_gzip_body_is_decompressed() {
    let s = http_setup().await;
    let body = fixture("tests/fixtures/otel/parent_child.json");
    let bytes = serde_json::to_vec(&body).unwrap();
    let gz = gzip(&bytes);
    let resp = s
        .post("/otel/v1/traces")
        .add_header("content-type", "application/json")
        .add_header("content-encoding", "gzip")
        .bytes(gz.into())
        .await;
    resp.assert_status_ok();
    let v: serde_json::Value = resp.json();
    assert_eq!(v["data"]["accepted_spans"], 2);
}

#[tokio::test]
async fn post_traces_without_session_id_skips_session_listing() {
    let s = http_setup().await;
    let body = fixture("tests/fixtures/otel/missing_session_id.json");
    let resp = s.post("/otel/v1/traces").json(&body).await;
    resp.assert_status_ok();
    let v: serde_json::Value = resp.json();
    assert_eq!(v["data"]["accepted_spans"], 1);
    assert!(
        v["data"]["sessions_touched"].as_array().unwrap().is_empty(),
        "no session.id means no session listed"
    );
    let listed: serde_json::Value = s.get("/v1/sessions").await.json();
    let arr = listed["data"].as_array().unwrap();
    assert!(arr.iter().all(|s| s["session_id"] != ""));
}

#[tokio::test]
async fn session_detail_returns_otel_span_with_telemetry() {
    let pool = make_pool().await;
    let body = fixture("tests/fixtures/otel/single_span.json");
    otel::store(&pool, otel::parse_otlp_json(&body), Utc::now())
        .await
        .unwrap();

    let app = witmcc::api::router(pool);
    let server = TestServer::new(app).unwrap();

    let resp = server.get("/v1/sessions/sess-otel-A").await;
    resp.assert_status_ok();
    let v: serde_json::Value = resp.json();

    let events = v["data"]["events"].as_array().expect("events array");
    let otel_event = events
        .iter()
        .find(|e| e["kind"] == "otel_span")
        .expect("otel_span event present in session detail");
    assert_eq!(otel_event["kind"], "otel_span");
    assert!(
        !otel_event["telemetry"].is_null(),
        "telemetry facet must be populated on the wire"
    );
    assert_eq!(otel_event["telemetry"]["span_name"], "tool.invoke");
    assert_eq!(otel_event["trace_id"], "5b8aa5a2d2c872e8321cf37308d69df2");
    assert_eq!(otel_event["span_id"], "051581bf3cb55c13");
}

#[tokio::test]
async fn post_traces_makes_graph_visible_to_http_consumers() {
    // Regression: previously otel::store inserted observed_event rows but did
    // not rebuild the graph, so GET /v1/sessions/:id/graph returned 404 for
    // OTel-only sessions even after a successful POST /otel/v1/traces.
    let s = http_setup().await;
    let body = fixture("tests/fixtures/otel/single_span.json");
    s.post("/otel/v1/traces").json(&body).await.assert_status_ok();

    let resp = s.get("/v1/sessions/sess-otel-A/graph").await;
    resp.assert_status_ok();
    let v: serde_json::Value = resp.json();
    let nodes = v["data"]["nodes"].as_array().expect("nodes array");
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["node_kind"], "otel_span");
}
