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
