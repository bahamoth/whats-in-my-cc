//! Slice-14 — unit tests for the `MissingVerification` L1 extractor.
//! All tests use synthetic `SessionInsightView` data — no DB, no I/O.

use chrono::{TimeZone, Utc};
use serde_json::json;
use witmcc::db::repo_diff_hunk::DiffHunkRow;
use witmcc::db::repo_episode::EpisodeRow;
use witmcc::db::repo_verification_run::VerificationRunRow;
use witmcc::insight::extractors::missing_verification::MissingVerification;
use witmcc::insight::extractor::InsightExtractor;
use witmcc::insight::view::SessionInsightView;
use witmcc::model::graph::{GraphEdge, GraphNode};
use witmcc::model::observed::{Actor, EventKind, ObservedEvent};

fn base_event(i: usize, actor: Actor, kind: EventKind) -> ObservedEvent {
    ObservedEvent {
        event_id: format!("ev_{i:03}"),
        raw_event_id: format!("raw_{i:03}"),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_t".into(),
        event_uuid: Some(format!("uuid_{i}")),
        observed_at: Utc.timestamp_opt(1_700_000_000 + i as i64 * 10, 0).unwrap(),
        actor,
        kind,
        parser_version: "test".into(),
        ..Default::default()
    }
}

fn ep(
    id: &str,
    phase: &str,
    start: &str,
    end: &str,
) -> EpisodeRow {
    EpisodeRow {
        episode_id: id.into(),
        schema_version: "episode.v1".into(),
        session_id: "sess_t".into(),
        phase: phase.into(),
        start_event_id: start.into(),
        end_event_id: end.into(),
        started_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap().to_rfc3339(),
        ended_at: Utc.timestamp_opt(1_700_000_100, 0).unwrap().to_rfc3339(),
        evidence_node_ids: "[]".into(),
        classification_basis: "[]".into(),
        confidence: 0.9,
        summary: None,
        classifier_version: "episode_classifier@v1".into(),
        created_at: Utc::now().to_rfc3339(),
    }
}

fn diff_hunk(id: &str, by_event: &str) -> DiffHunkRow {
    DiffHunkRow {
        diff_hunk_id: id.into(),
        schema_version: "diff_hunk.v1".into(),
        session_id: "sess_t".into(),
        file_path: "src/foo.rs".into(),
        change_type: "modify".into(),
        line_range_after_start: Some(1),
        line_range_after_end: Some(10),
        introduced_by_event_id: by_event.into(),
        introduced_by_tool_use_id: None,
        patch_preview: "+line".into(),
        lines_added: 1,
        lines_removed: 0,
        user_modified: false,
    }
}

fn empty_view<'a>(
    events: &'a [ObservedEvent],
    episodes: &'a [EpisodeRow],
    diff_hunks: &'a [DiffHunkRow],
) -> SessionInsightView<'a> {
    SessionInsightView {
        session_id: "sess_t",
        events,
        diff_hunks,
        verification_runs: &[],
        episodes,
        nodes: &[],
        edges: &[],
    }
}

// ---------------------------------------------------------------------------
// Scenario helpers
// ---------------------------------------------------------------------------

/// Action episode + diff hunk produced inside it, no following verification.
/// Expected: 1 candidate fires.
#[test]
fn fires_when_action_has_no_following_verification() {
    // Events: user_message (intake), tool_call (action), tool_result
    let events = vec![
        base_event(0, Actor::User, EventKind::UserMessage),
        base_event(1, Actor::Assistant, EventKind::ToolCall),
        base_event(2, Actor::Tool, EventKind::ToolResult),
    ];
    let episodes = vec![
        ep("ep_001", "intake", "ev_000", "ev_000"),
        ep("ep_002", "action", "ev_001", "ev_002"),
    ];
    // Diff hunk introduced inside the action episode range (ev_001..ev_002)
    let diff_hunks = vec![diff_hunk("dh_001", "ev_001")];
    let view = empty_view(&events, &episodes, &diff_hunks);

    let cands = MissingVerification.extract(&view);
    assert_eq!(cands.len(), 1, "expected 1 candidate, got {:?}", cands.len());
    let c = &cands[0];
    assert_eq!(c.category, "missing_verification");
    assert!(
        (c.confidence_l1 - 0.9).abs() < f32::EPSILON,
        "confidence_l1 must be 0.9, got {}",
        c.confidence_l1
    );
    assert!(
        c.evidence_refs.len() >= 2,
        "evidence_refs must have at least 2 entries, got {:?}",
        c.evidence_refs
    );
}

/// Action episode followed by a verification episode → rule must NOT fire.
#[test]
fn does_not_fire_when_verification_follows() {
    let events = vec![
        base_event(0, Actor::User, EventKind::UserMessage),
        base_event(1, Actor::Assistant, EventKind::ToolCall),
        base_event(2, Actor::Tool, EventKind::ToolResult),
        base_event(3, Actor::Tool, EventKind::ToolResult), // verify output
    ];
    let episodes = vec![
        ep("ep_001", "intake", "ev_000", "ev_000"),
        ep("ep_002", "action", "ev_001", "ev_002"),
        ep("ep_003", "verification", "ev_003", "ev_003"),
    ];
    let diff_hunks = vec![diff_hunk("dh_001", "ev_001")];
    let view = empty_view(&events, &episodes, &diff_hunks);

    let cands = MissingVerification.extract(&view);
    assert!(
        cands.is_empty(),
        "must not fire when verification follows action, got {:?}",
        cands.len()
    );
}

/// Read-only session: no action episodes, no diff hunks → zero candidates.
#[test]
fn does_not_fire_for_read_only_session() {
    let events = vec![
        base_event(0, Actor::User, EventKind::UserMessage),
        base_event(1, Actor::Assistant, EventKind::ToolCall),
        base_event(2, Actor::Tool, EventKind::ToolResult),
    ];
    let episodes = vec![
        ep("ep_001", "intake", "ev_000", "ev_000"),
        ep("ep_002", "exploration", "ev_001", "ev_002"),
    ];
    // No diff hunks → no mutation happened
    let view = empty_view(&events, &episodes, &[]);

    let cands = MissingVerification.extract(&view);
    assert!(
        cands.is_empty(),
        "must not fire for read-only session, got {:?}",
        cands.len()
    );
}

/// Action episode at the end of the session (no window close) → rule fires.
#[test]
fn fires_for_trailing_action_at_session_end() {
    let events = vec![
        base_event(0, Actor::User, EventKind::UserMessage),
        base_event(1, Actor::Assistant, EventKind::ToolCall),
        base_event(2, Actor::Tool, EventKind::ToolResult),
    ];
    // Only one intake window, action episode is last — never closed by a
    // second intake or a verification.
    let episodes = vec![
        ep("ep_001", "intake", "ev_000", "ev_000"),
        ep("ep_002", "action", "ev_001", "ev_002"),
    ];
    let diff_hunks = vec![diff_hunk("dh_001", "ev_001")];
    let view = empty_view(&events, &episodes, &diff_hunks);

    let cands = MissingVerification.extract(&view);
    assert_eq!(
        cands.len(),
        1,
        "trailing action at session end must fire; got {:?}",
        cands.len()
    );
}

/// Action episode with a FAILED verification in the window → rule must NOT fire.
/// (A verification existed; `final_state_mismatch` handles failed-verify cases.)
#[test]
fn does_not_fire_when_failed_verification_follows() {
    let events = vec![
        base_event(0, Actor::User, EventKind::UserMessage),
        base_event(1, Actor::Assistant, EventKind::ToolCall),
        base_event(2, Actor::Tool, EventKind::ToolResult),
        base_event(3, Actor::Tool, EventKind::ToolResult),
    ];
    let episodes = vec![
        ep("ep_001", "intake", "ev_000", "ev_000"),
        ep("ep_002", "action", "ev_001", "ev_002"),
        ep("ep_003", "verification", "ev_003", "ev_003"),
    ];
    let diff_hunks = vec![diff_hunk("dh_001", "ev_001")];
    let view = empty_view(&events, &episodes, &diff_hunks);

    let cands = MissingVerification.extract(&view);
    assert!(
        cands.is_empty(),
        "must not fire when verification (even failed) exists, got {:?}",
        cands.len()
    );
}

/// Action episode has NO diff hunks inside it → rule must NOT fire.
/// (Spec §4: the rule requires at least one diff_hunk produced inside the
/// action episode's event range.)
#[test]
fn does_not_fire_when_action_episode_has_no_diff_hunks() {
    let events = vec![
        base_event(0, Actor::User, EventKind::UserMessage),
        base_event(1, Actor::Assistant, EventKind::ToolCall),
        base_event(2, Actor::Tool, EventKind::ToolResult),
    ];
    let episodes = vec![
        ep("ep_001", "intake", "ev_000", "ev_000"),
        ep("ep_002", "action", "ev_001", "ev_002"),
    ];
    // No diff hunks in the action window
    let view = empty_view(&events, &episodes, &[]);

    let cands = MissingVerification.extract(&view);
    assert!(
        cands.is_empty(),
        "must not fire when action episode has no diff hunks, got {:?}",
        cands.len()
    );
}

/// Two action episodes both missing verification → two candidates.
#[test]
fn fires_for_each_action_episode_missing_verification() {
    let events: Vec<ObservedEvent> = (0..8).map(|i| {
        let actor = if i == 0 || i == 4 { Actor::User } else if i % 2 == 0 { Actor::Tool } else { Actor::Assistant };
        let kind = if i == 0 || i == 4 { EventKind::UserMessage } else if i % 2 == 1 { EventKind::ToolCall } else { EventKind::ToolResult };
        base_event(i, actor, kind)
    }).collect();

    let episodes = vec![
        ep("ep_001", "intake", "ev_000", "ev_000"),
        ep("ep_002", "action", "ev_001", "ev_002"),
        ep("ep_003", "intake", "ev_004", "ev_004"),
        ep("ep_004", "action", "ev_005", "ev_006"),
    ];
    let diff_hunks = vec![
        diff_hunk("dh_001", "ev_001"),
        diff_hunk("dh_002", "ev_005"),
    ];
    let view = empty_view(&events, &episodes, &diff_hunks);

    let cands = MissingVerification.extract(&view);
    assert_eq!(
        cands.len(),
        2,
        "two action episodes missing verification must produce two candidates, got {:?}",
        cands.len()
    );
}
