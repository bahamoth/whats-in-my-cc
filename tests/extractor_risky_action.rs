//! Slice-16 — unit tests for the `RiskyAction` L1 extractor.
//! All tests use synthetic `SessionInsightView` data — no DB, no I/O.

use chrono::{TimeZone, Utc};
use serde_json::json;
use wimcc::db::repo_diff_hunk::DiffHunkRow;
use wimcc::db::repo_verification_run::VerificationRunRow;
use wimcc::insight::extractor::InsightExtractor;
use wimcc::insight::extractors::risky_action::RiskyAction;
use wimcc::insight::types::PromotionPolicy;
use wimcc::insight::view::SessionInsightView;
use wimcc::model::graph::{GraphEdge, GraphNode};
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

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

fn bash_tool_call(i: usize, command: &str) -> ObservedEvent {
    let mut ev = base_event(i, Actor::Assistant, EventKind::ToolCall);
    ev.tool_name = Some("Bash".into());
    ev.tool_use_id = Some(format!("tu_{i:03}"));
    ev.payload = json!({
        "tool_use": {
            "name": "Bash",
            "input": { "command": command }
        }
    });
    ev
}

fn synth_view_with_bash<'a>(
    events: &'a [ObservedEvent],
    diff_hunks: &'a [DiffHunkRow],
) -> SessionInsightView<'a> {
    SessionInsightView {
        session_id: "sess_t",
        events,
        diff_hunks,
        verification_runs: &[],
        nodes: &[],
        edges: &[],
    }
}

fn diff_hunk_user_modified(id: &str, by_event: &str) -> DiffHunkRow {
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
        patch_preview: "-old\n+new".into(),
        lines_added: 1,
        lines_removed: 1,
        user_modified: true,
    }
}

fn diff_hunk_not_user_modified(id: &str, by_event: &str) -> DiffHunkRow {
    let mut h = diff_hunk_user_modified(id, by_event);
    h.user_modified = false;
    h
}

// ---------------------------------------------------------------------------
// Destructive Bash branch
// ---------------------------------------------------------------------------

#[test]
fn fires_on_rm_rf() {
    let events = vec![bash_tool_call(0, "rm -rf /tmp/foo")];
    let view = synth_view_with_bash(&events, &[]);
    let cands = RiskyAction.extract(&view);
    assert_eq!(cands.len(), 1, "rm -rf must fire risky_action");
    let c = &cands[0];
    assert_eq!(c.category, "risky_action");
    assert!((c.confidence_l1 - 0.7).abs() < f32::EPSILON, "confidence_l1 must be 0.7");
    assert_eq!(c.severity, "high");
    assert!(!c.evidence_refs.is_empty(), "evidence_refs must be non-empty");
}

#[test]
fn fires_on_git_push_force() {
    let events = vec![bash_tool_call(0, "git push --force origin main")];
    let view = synth_view_with_bash(&events, &[]);
    let cands = RiskyAction.extract(&view);
    assert_eq!(cands.len(), 1, "git push --force must fire");
}

#[test]
fn fires_on_git_push_dash_f() {
    let events = vec![bash_tool_call(0, "git push -f")];
    let view = synth_view_with_bash(&events, &[]);
    let cands = RiskyAction.extract(&view);
    assert_eq!(cands.len(), 1, "git push -f must fire");
}

#[test]
fn fires_on_git_reset_hard() {
    let events = vec![bash_tool_call(0, "git reset --hard HEAD~1")];
    let view = synth_view_with_bash(&events, &[]);
    let cands = RiskyAction.extract(&view);
    assert_eq!(cands.len(), 1, "git reset --hard must fire");
}

#[test]
fn fires_on_sudo_rm() {
    let events = vec![bash_tool_call(0, "sudo rm -rf /var/log")];
    let view = synth_view_with_bash(&events, &[]);
    let cands = RiskyAction.extract(&view);
    assert_eq!(cands.len(), 1, "sudo rm must fire");
}

#[test]
fn does_not_fire_on_safe_bash_ls() {
    let events = vec![bash_tool_call(0, "ls -la")];
    let view = synth_view_with_bash(&events, &[]);
    let cands = RiskyAction.extract(&view);
    assert!(cands.is_empty(), "ls -la must not fire");
}

#[test]
fn does_not_fire_on_safe_bash_grep() {
    let events = vec![bash_tool_call(0, "grep -r 'todo' .")];
    let view = synth_view_with_bash(&events, &[]);
    let cands = RiskyAction.extract(&view);
    assert!(cands.is_empty(), "grep must not fire");
}

#[test]
fn does_not_fire_on_non_bash_tool() {
    let mut ev = base_event(0, Actor::Assistant, EventKind::ToolCall);
    ev.tool_name = Some("Read".into());
    ev.payload = json!({
        "tool_use": {
            "name": "Read",
            "input": { "file_path": "/tmp/foo" }
        }
    });
    let events = vec![ev];
    let view = synth_view_with_bash(&events, &[]);
    let cands = RiskyAction.extract(&view);
    assert!(cands.is_empty(), "non-Bash tool with destructive text must not fire");
}

// ---------------------------------------------------------------------------
// user_modified hunk branch
// ---------------------------------------------------------------------------

#[test]
fn fires_on_user_modified_hunk() {
    let events = vec![base_event(0, Actor::Assistant, EventKind::ToolCall)];
    let diff_hunks = vec![diff_hunk_user_modified("dh_001", "ev_000")];
    let view = synth_view_with_bash(&events, &diff_hunks);
    let cands = RiskyAction.extract(&view);
    assert_eq!(cands.len(), 1, "user_modified hunk must fire");
    let c = &cands[0];
    assert_eq!(c.category, "risky_action");
    assert!((c.confidence_l1 - 0.7).abs() < f32::EPSILON);
}

#[test]
fn does_not_fire_on_non_user_modified_hunk() {
    let events = vec![base_event(0, Actor::Assistant, EventKind::ToolCall)];
    let diff_hunks = vec![diff_hunk_not_user_modified("dh_001", "ev_000")];
    let view = synth_view_with_bash(&events, &diff_hunks);
    let cands = RiskyAction.extract(&view);
    assert!(cands.is_empty(), "non-user-modified hunk must not fire");
}

// ---------------------------------------------------------------------------
// Empty session
// ---------------------------------------------------------------------------

#[test]
fn does_not_fire_on_empty_session() {
    let view = synth_view_with_bash(&[], &[]);
    let cands = RiskyAction.extract(&view);
    assert!(cands.is_empty());
}

// ---------------------------------------------------------------------------
// Promotion policy — must be IfAbove(1.0) so judge is always required
// ---------------------------------------------------------------------------

#[test]
fn promotion_policy_is_if_above_1() {
    let policy = RiskyAction.promotion_policy();
    assert_eq!(
        policy,
        PromotionPolicy::IfAbove(1.0),
        "risky_action must use IfAbove(1.0) to always require judge"
    );
}

// ---------------------------------------------------------------------------
// Evidence projection fields
// ---------------------------------------------------------------------------

#[test]
fn projection_includes_required_fields_for_destructive_bash() {
    let events = vec![bash_tool_call(0, "rm -rf /tmp/foo")];
    let view = synth_view_with_bash(&events, &[]);
    let cands = RiskyAction.extract(&view);
    assert_eq!(cands.len(), 1);
    let proj = &cands[0].evidence_projection;
    assert_eq!(proj["category"], "risky_action");
    assert!(proj["trigger"]["kind"].is_string());
    assert!(proj["trigger"]["command_redacted"].is_string());
}

#[test]
fn projection_includes_required_fields_for_user_modified() {
    let events = vec![base_event(0, Actor::Assistant, EventKind::ToolCall)];
    let diff_hunks = vec![diff_hunk_user_modified("dh_001", "ev_000")];
    let view = synth_view_with_bash(&events, &diff_hunks);
    let cands = RiskyAction.extract(&view);
    assert_eq!(cands.len(), 1);
    let proj = &cands[0].evidence_projection;
    assert_eq!(proj["category"], "risky_action");
    assert_eq!(proj["trigger"]["kind"], "user_modified_hunk");
}
