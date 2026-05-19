use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::{migrate, repo_graph};
use witmcc::graph::build;
use witmcc::ingest::store;

#[tokio::test]
async fn deterministic_minimal_graph() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/transcripts/minimal_session.jsonl"),
    )
    .await
    .unwrap();
    build::rebuild_session(&pool, "sess-A").await.unwrap();
    let (n1, e1) = repo_graph::load_session(&pool, "sess-A").await.unwrap();

    // Re-run to verify identical ids/contents.
    build::rebuild_session(&pool, "sess-A").await.unwrap();
    let (n2, e2) = repo_graph::load_session(&pool, "sess-A").await.unwrap();

    let ids = |ns: &[witmcc::model::graph::GraphNode]| -> Vec<String> {
        ns.iter().map(|n| n.node_id.clone()).collect()
    };
    let eids = |es: &[witmcc::model::graph::GraphEdge]| -> Vec<String> {
        es.iter().map(|e| e.edge_id.clone()).collect()
    };
    assert_eq!(ids(&n1), ids(&n2));
    assert_eq!(eids(&e1), eids(&e2));

    // Spot-check edge kinds present.
    let kinds: std::collections::BTreeSet<_> = e1.iter().map(|e| e.edge_kind.clone()).collect();
    for k in ["turn_order", "message_reply", "tool_call_to_result"] {
        assert!(kinds.contains(k), "missing edge kind: {k}, got {kinds:?}");
    }
}
