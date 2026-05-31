use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::{migrate, repo_graph};
use witmcc::graph::build;
use witmcc::ingest::store;
use witmcc::model::observed::{Actor, EventKind, ObservedEvent};

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
        &witmcc::live::NoopSink,
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

// Slice 2 (telemetry fold): hook_event is orphan telemetry — it carries no
// conversation/action backbone role, so `compute()` drops it from the graph.
// The hook record itself remains in observed_event (SSOT). (Pre-Slice-2 these
// tests asserted the hook_event node's merge_key derivation; that derivation
// code still runs during materialization but its output is dropped before the
// graph is returned.)
#[test]
fn external_hook_produces_no_graph_node() {
    use chrono::Utc;
    use serde_json::json;
    let session = "sess_HK";
    let ev = ObservedEvent {
        event_id: "ev1".into(),
        session_id: session.into(),
        observed_at: Utc::now(),
        actor: Actor::Hook,
        kind: EventKind::HookEvent,
        subkind: Some("pre_tool_use".into()),
        tool_use_id: Some("toolu_01".into()),
        payload: json!({"hook": {"hook_event_name": "PreToolUse"}}),
        parser_version: "hook@0.1.0".into(),
        ..Default::default()
    };
    let (nodes, _) = build::compute(session, &[ev], &[], &[]);
    assert!(
        !nodes.iter().any(|n| n.node_kind == "hook_event"),
        "external hook_event is dropped from the graph (orphan telemetry); got {:?}",
        nodes.iter().map(|n| n.node_kind.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn transcript_internal_hook_produces_no_graph_node() {
    use chrono::Utc;
    use serde_json::json;
    let session = "sess_HK";
    let ev = ObservedEvent {
        event_id: "ev1".into(),
        session_id: session.into(),
        event_uuid: Some("uuid-abc".into()),
        observed_at: Utc::now(),
        actor: Actor::Hook,
        kind: EventKind::HookEvent,
        subkind: Some("hook_additional_context".into()),
        payload: json!({}),
        parser_version: "transcript@0.1.0".into(),
        ..Default::default()
    };
    let (nodes, _) = build::compute(session, &[ev], &[], &[]);
    assert!(
        !nodes.iter().any(|n| n.node_kind == "hook_event"),
        "internal hook_event is dropped from the graph (orphan telemetry); got {:?}",
        nodes.iter().map(|n| n.node_kind.as_str()).collect::<Vec<_>>()
    );
}
