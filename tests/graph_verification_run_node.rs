//! Slice-11 — graph builder integration tests for verification_run nodes/edges.
//! (TDD red — Phase 1 commit 1.)
//!
//! Tests:
//! - `compute()` with `VerificationRunRow` input emits `verification_run` node
//!   and `triggered_verification` edge.
//! - `covers_diff_hunk` edge is emitted when a diff_hunk precedes the run.

use chrono::Utc;
use serde_json::json;
use witmcc::db::repo_diff_hunk::DiffHunkRow;
use witmcc::db::repo_verification_run::VerificationRunRow;
use witmcc::graph::build::compute;
use witmcc::model::observed::{Actor, EventKind, ObservedEvent};

const SESSION_ID: &str = "sess_vr_test";
const TOOL_USE_ID: &str = "toolu_vr_test_01";

/// Build a minimal session: one assistant (tool_call Bash) + one user
/// (tool_result). The tool_call carries TOOL_USE_ID for Bash.
fn minimal_bash_events() -> Vec<ObservedEvent> {
    let t0 = Utc::now();
    let t1 = t0 + chrono::Duration::seconds(2);

    vec![
        ObservedEvent {
            event_id: "ev_call_01".into(),
            raw_event_id: "raw_call_01".into(),
            schema_version: "0.5.0".into(),
            session_id: SESSION_ID.into(),
            event_uuid: Some("u_a_vr_01".into()),
            observed_at: t0,
            actor: Actor::Assistant,
            kind: EventKind::ToolCall,
            tool_use_id: Some(TOOL_USE_ID.into()),
            tool_name: Some("Bash".into()),
            payload: json!({
                "tool_use": {
                    "id": TOOL_USE_ID,
                    "name": "Bash",
                    "input": {"command": "cargo test"}
                }
            }),
            parser_version: "transcript@0.1.0".into(),
            ..Default::default()
        },
        ObservedEvent {
            event_id: "ev_result_01".into(),
            raw_event_id: "raw_result_01".into(),
            schema_version: "0.5.0".into(),
            session_id: SESSION_ID.into(),
            event_uuid: Some("u_u_vr_01".into()),
            parent_uuid: Some("u_a_vr_01".into()),
            observed_at: t1,
            actor: Actor::User,
            kind: EventKind::ToolResult,
            tool_use_id: Some(TOOL_USE_ID.into()),
            tool_name: Some("Bash".into()),
            payload: json!({
                "tool_result": {
                    "tool_use_id": TOOL_USE_ID,
                    "is_error": false,
                    "content": "test result: ok. 5 passed"
                }
            }),
            parser_version: "transcript@0.1.0".into(),
            ..Default::default()
        },
    ]
}

#[test]
fn compute_emits_verification_run_node_and_triggered_edge() {
    let evs = minimal_bash_events();
    let hunks: Vec<DiffHunkRow> = vec![];
    let runs = vec![VerificationRunRow {
        verification_run_id: "vr_test_x".into(),
        schema_version: "verification_run.v1".into(),
        session_id: SESSION_ID.into(),
        source: "bash".into(),
        command: "cargo test".into(),
        command_kind: "test_suite_rust".into(),
        trigger_event_id: "ev_result_01".into(),
        trigger_tool_use_id: Some(TOOL_USE_ID.into()),
        status: "passed".into(),
        detection_basis: "known_tool".into(),
        status_basis: "exit".into(),
        started_at: "2026-05-27T10:00:00Z".into(),
        ended_at: Some("2026-05-27T10:00:05Z".into()),
        exit_code: Some(0),
        failure_summary: None,
        raw_event_id: "raw_result_01".into(),
        parser_version: "verification_run@v1".into(),
    }];

    let (nodes, edges) = compute(SESSION_ID, &evs, &hunks, &runs);

    assert!(
        nodes.iter().any(|n| n.node_kind == "verification_run"),
        "compute() must emit a verification_run node; got node kinds: {:?}",
        nodes.iter().map(|n| &n.node_kind).collect::<Vec<_>>()
    );

    assert!(
        edges.iter().any(|e| e.edge_kind == "triggered_verification"),
        "compute() must emit a triggered_verification edge; got edge kinds: {:?}",
        edges.iter().map(|e| &e.edge_kind).collect::<Vec<_>>()
    );

    // Verify the triggered_verification edge goes from tool_call → verification_run
    let tv_edge = edges.iter().find(|e| e.edge_kind == "triggered_verification").unwrap();
    let from_node = nodes.iter().find(|n| n.node_id == tv_edge.from_node_id);
    assert!(
        from_node.is_some(),
        "triggered_verification from_node_id must reference a node in the output"
    );
}

#[test]
fn covers_diff_hunk_edge_links_temporal_precedence() {
    // Build: Edit tool_call/result (introduces diff_hunk), then Bash test (verification).
    // Assert: covers_diff_hunk edge from verification_run → diff_hunk.
    let t0 = chrono::DateTime::parse_from_rfc3339("2026-05-27T10:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let t1 = t0 + chrono::Duration::seconds(1);
    let t2 = t0 + chrono::Duration::seconds(5);
    let t3 = t0 + chrono::Duration::seconds(6);

    let edit_use_id = "toolu_edit_01";
    let bash_use_id = "toolu_bash_01";

    let evs = vec![
        ObservedEvent {
            event_id: "ev_edit_call".into(),
            raw_event_id: "raw_ec".into(),
            schema_version: "0.5.0".into(),
            session_id: SESSION_ID.into(),
            event_uuid: Some("u_a_edit".into()),
            observed_at: t0,
            actor: Actor::Assistant,
            kind: EventKind::ToolCall,
            tool_use_id: Some(edit_use_id.into()),
            tool_name: Some("Edit".into()),
            payload: json!({"tool_use": {"id": edit_use_id, "name": "Edit"}}),
            parser_version: "transcript@0.1.0".into(),
            ..Default::default()
        },
        ObservedEvent {
            event_id: "ev_edit_result".into(),
            raw_event_id: "raw_er".into(),
            schema_version: "0.5.0".into(),
            session_id: SESSION_ID.into(),
            event_uuid: Some("u_u_edit".into()),
            parent_uuid: Some("u_a_edit".into()),
            observed_at: t1,
            actor: Actor::User,
            kind: EventKind::ToolResult,
            tool_use_id: Some(edit_use_id.into()),
            tool_name: Some("Edit".into()),
            payload: json!({"tool_result": {"tool_use_id": edit_use_id, "content": "ok"}}),
            parser_version: "transcript@0.1.0".into(),
            ..Default::default()
        },
        ObservedEvent {
            event_id: "ev_bash_call".into(),
            raw_event_id: "raw_bc".into(),
            schema_version: "0.5.0".into(),
            session_id: SESSION_ID.into(),
            event_uuid: Some("u_a_bash".into()),
            observed_at: t2,
            actor: Actor::Assistant,
            kind: EventKind::ToolCall,
            tool_use_id: Some(bash_use_id.into()),
            tool_name: Some("Bash".into()),
            payload: json!({"tool_use": {"id": bash_use_id, "name": "Bash", "input": {"command": "cargo test"}}}),
            parser_version: "transcript@0.1.0".into(),
            ..Default::default()
        },
        ObservedEvent {
            event_id: "ev_bash_result".into(),
            raw_event_id: "raw_br".into(),
            schema_version: "0.5.0".into(),
            session_id: SESSION_ID.into(),
            event_uuid: Some("u_u_bash".into()),
            parent_uuid: Some("u_a_bash".into()),
            observed_at: t3,
            actor: Actor::User,
            kind: EventKind::ToolResult,
            tool_use_id: Some(bash_use_id.into()),
            tool_name: Some("Bash".into()),
            payload: json!({"tool_result": {"tool_use_id": bash_use_id, "is_error": false, "content": "ok. 5 passed"}}),
            parser_version: "transcript@0.1.0".into(),
            ..Default::default()
        },
    ];

    let hunks = vec![DiffHunkRow {
        diff_hunk_id: "dh_covers_01".into(),
        schema_version: "0.5.0".into(),
        session_id: SESSION_ID.into(),
        file_path: "src/lib.rs".into(),
        change_type: "modified".into(),
        line_range_after_start: Some(1),
        line_range_after_end: Some(5),
        introduced_by_event_id: "ev_edit_result".into(),
        introduced_by_tool_use_id: Some(edit_use_id.into()),
        patch_preview: "@@ -1 +1 @@\n+x\n".into(),
        lines_added: 1,
        lines_removed: 0,
        user_modified: false,
    }];

    let runs = vec![VerificationRunRow {
        verification_run_id: "vr_covers_01".into(),
        schema_version: "verification_run.v1".into(),
        session_id: SESSION_ID.into(),
        source: "bash".into(),
        command: "cargo test".into(),
        command_kind: "test_suite_rust".into(),
        trigger_event_id: "ev_bash_result".into(),
        trigger_tool_use_id: Some(bash_use_id.into()),
        status: "passed".into(),
        detection_basis: "known_tool".into(),
        status_basis: "exit".into(),
        started_at: t2.to_rfc3339(),
        ended_at: Some(t3.to_rfc3339()),
        exit_code: Some(0),
        failure_summary: None,
        raw_event_id: "raw_br".into(),
        parser_version: "verification_run@v1".into(),
    }];

    let (nodes, edges) = compute(SESSION_ID, &evs, &hunks, &runs);

    assert!(
        edges.iter().any(|e| e.edge_kind == "covers_diff_hunk"),
        "covers_diff_hunk edge must be emitted when diff_hunk precedes verification_run; edges: {:?}",
        edges.iter().map(|e| &e.edge_kind).collect::<Vec<_>>()
    );

    let cv_edge = edges.iter().find(|e| e.edge_kind == "covers_diff_hunk").unwrap();
    let from_node = nodes.iter().find(|n| n.node_id == cv_edge.from_node_id)
        .expect("covers_diff_hunk from node must exist");
    let to_node = nodes.iter().find(|n| n.node_id == cv_edge.to_node_id)
        .expect("covers_diff_hunk to node must exist");
    assert_eq!(from_node.node_kind, "verification_run", "covers_diff_hunk must go FROM verification_run");
    assert_eq!(to_node.node_kind, "diff_hunk", "covers_diff_hunk must go TO diff_hunk");
}
