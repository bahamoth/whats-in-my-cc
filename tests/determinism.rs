use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::migrate;
use wimcc::graph::build;
use wimcc::ingest::store;

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
    store::ingest_file(&pool_a, std::path::Path::new(path), &wimcc::live::NoopSink)
        .await
        .unwrap();
    build::rebuild_session(&pool_a, session_id).await.unwrap();
    let (n_a, e_a) = wimcc::db::repo_graph::load_session(&pool_a, session_id)
        .await
        .unwrap();

    let pool_b = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool_b).await.unwrap();
    store::ingest_file(&pool_b, std::path::Path::new(path), &wimcc::live::NoopSink)
        .await
        .unwrap();
    build::rebuild_session(&pool_b, session_id).await.unwrap();
    let (n_b, e_b) = wimcc::db::repo_graph::load_session(&pool_b, session_id)
        .await
        .unwrap();

    let ids = |v: &[wimcc::model::graph::GraphNode]| {
        v.iter().map(|x| x.node_id.clone()).collect::<Vec<_>>()
    };
    let eids = |v: &[wimcc::model::graph::GraphEdge]| {
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
        &wimcc::live::NoopSink,
    )
    .await
    .unwrap();
    build::rebuild_session(&pool, "sess-D").await.unwrap();
    let (nodes, edges) = wimcc::db::repo_graph::load_session(&pool, "sess-D")
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
        &wimcc::live::NoopSink,
    )
    .await
    .unwrap();
    build::rebuild_session(&pool, "sess-S").await.unwrap();
    let (_, edges) = wimcc::db::repo_graph::load_session(&pool, "sess-S")
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

#[tokio::test]
async fn multi_text_user_message_dedupes_node() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    wimcc::db::migrate(&pool).await.unwrap();
    wimcc::ingest::store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/transcripts/multi_text_user.jsonl"),
        &wimcc::live::NoopSink,
    )
    .await
    .expect("ingest should not crash on multi-text user message");
    wimcc::graph::build::rebuild_session(&pool, "sess-M")
        .await
        .unwrap();
    let (nodes, _edges) = wimcc::db::repo_graph::load_session(&pool, "sess-M")
        .await
        .unwrap();
    let user_nodes: Vec<_> = nodes
        .iter()
        .filter(|n| n.node_kind == "user_message")
        .collect();
    assert_eq!(
        user_nodes.len(),
        1,
        "expected exactly 1 user_message node, got {}",
        user_nodes.len()
    );
    assert_eq!(
        user_nodes[0].source_event_ids.len(),
        2,
        "expected 2 source_event_ids, got {}: {:?}",
        user_nodes[0].source_event_ids.len(),
        user_nodes[0].source_event_ids
    );
}
