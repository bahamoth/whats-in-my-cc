use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::migrate;
use witmcc::graph::build;
use witmcc::ingest::store;

async fn ingest_twice(
    path: &str,
    session_id: &str,
) -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
    let pool_a = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool_a).await.unwrap();
    store::ingest_file(&pool_a, std::path::Path::new(path))
        .await
        .unwrap();
    build::rebuild_session(&pool_a, session_id).await.unwrap();
    let (n_a, e_a) = witmcc::db::repo_graph::load_session(&pool_a, session_id)
        .await
        .unwrap();

    let pool_b = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool_b).await.unwrap();
    store::ingest_file(&pool_b, std::path::Path::new(path))
        .await
        .unwrap();
    build::rebuild_session(&pool_b, session_id).await.unwrap();
    let (n_b, e_b) = witmcc::db::repo_graph::load_session(&pool_b, session_id)
        .await
        .unwrap();

    let ids = |v: &[witmcc::model::graph::GraphNode]| {
        v.iter().map(|x| x.node_id.clone()).collect::<Vec<_>>()
    };
    let eids = |v: &[witmcc::model::graph::GraphEdge]| {
        v.iter().map(|x| x.edge_id.clone()).collect::<Vec<_>>()
    };
    (ids(&n_a), ids(&n_b), eids(&e_a), eids(&e_b))
}

#[tokio::test]
async fn minimal_session_ids_stable_across_databases() {
    let (na, nb, ea, eb) =
        ingest_twice("tests/fixtures/transcripts/minimal_session.jsonl", "sess-A").await;
    pretty_assertions::assert_eq!(na, nb);
    pretty_assertions::assert_eq!(ea, eb);
}

#[tokio::test]
async fn dangling_tool_use_creates_separate_call_node_no_result_edge() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/transcripts/dangling_tool_use.jsonl"),
    )
    .await
    .unwrap();
    build::rebuild_session(&pool, "sess-D").await.unwrap();
    let (nodes, edges) = witmcc::db::repo_graph::load_session(&pool, "sess-D")
        .await
        .unwrap();
    assert!(nodes.iter().any(|n| n.node_kind == "tool_call"));
    assert!(!edges.iter().any(|e| e.edge_kind == "tool_call_to_result"));
}

#[tokio::test]
async fn sidechain_edge_is_marked() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/transcripts/sidechain.jsonl"),
    )
    .await
    .unwrap();
    build::rebuild_session(&pool, "sess-S").await.unwrap();
    let (_, edges) = witmcc::db::repo_graph::load_session(&pool, "sess-S")
        .await
        .unwrap();
    let crossing = edges.iter().find(|e| {
        e.edge_kind == "message_reply"
            && e.to_node_id.starts_with("nd_")
            && e.attributes.get("crosses_sidechain") == Some(&serde_json::Value::Bool(true))
    });
    assert!(
        crossing.is_some(),
        "expected a message_reply edge flagged crosses_sidechain"
    );
}
