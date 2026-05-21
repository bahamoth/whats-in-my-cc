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

#[test]
fn external_hook_node_keys_by_hook_event_name_and_tool_use_id() {
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
    let (nodes, _) = build::compute(session, &[ev]);
    assert_eq!(nodes.len(), 1);
    let n = &nodes[0];
    assert_eq!(n.node_kind, "hook_event");
    assert_eq!(
        n.merge_keys
            .get("hook_event_name")
            .and_then(|v| v.as_str()),
        Some("pre_tool_use")
    );
    assert_eq!(
        n.merge_keys.get("tool_use_id").and_then(|v| v.as_str()),
        Some("toolu_01")
    );
    assert!(
        n.merge_keys.get("event_uuid").is_none(),
        "external hook must not key by event_uuid"
    );
}

#[test]
fn transcript_internal_hook_keeps_event_uuid_merge_keys() {
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
    let (nodes, _) = build::compute(session, &[ev]);
    assert_eq!(nodes.len(), 1);
    assert_eq!(
        nodes[0].merge_keys.get("event_uuid").and_then(|v| v.as_str()),
        Some("uuid-abc")
    );
    assert!(
        nodes[0].merge_keys.get("hook_event_name").is_none(),
        "internal hook must not key by hook_event_name"
    );
}
