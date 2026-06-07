//! Unit tests for the `RiskyAction` detector (Plan 1: finding → signal).
//! All tests use synthetic `SessionInsightView` data — no DB, no I/O.
//! Facts only: `trigger.kind` / `command_redacted`. NO severity (judgment).

use chrono::{TimeZone, Utc};
use serde_json::json;
use wimcc::db::repo_diff_hunk::DiffHunkRow;
use wimcc::insight::config::DetectorConfig;
use wimcc::insight::extractor::Detector;
use wimcc::insight::extractors::risky_action::RiskyAction;
use wimcc::insight::view::SessionInsightView;
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

fn detect(view: &SessionInsightView<'_>) -> Vec<wimcc::insight::types::SignalCandidate> {
    RiskyAction.detect(view, &DetectorConfig::default())
}

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

/// Build a Bash ToolCall event using the REAL mapping.rs:193 payload shape:
/// `{"content_ordinal": N, "tool_name": "Bash", "input": {"command": ...}}`.
/// This is the shape arriving from actual Claude Code sessions — command at /input/command.
fn bash_tool_call(i: usize, command: &str) -> ObservedEvent {
    let mut ev = base_event(i, Actor::Assistant, EventKind::ToolCall);
    ev.tool_name = Some("Bash".into());
    ev.tool_use_id = Some(format!("tu_{i:03}"));
    ev.payload = json!({
        "content_ordinal": 0,
        "tool_name": "Bash",
        "input": { "command": command }
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

/// Build a Bash ToolCall event using the REAL mapping.rs payload shape:
/// `{"content_ordinal": N, "tool_name": "Bash", "input": {"command": ...}}`
/// (per src/ingest/mapping.rs:193, verified against real transcripts).
/// This is the shape that arrives from actual Claude Code sessions.
fn bash_tool_call_real_shape(i: usize, command: &str) -> ObservedEvent {
    let mut ev = base_event(i, Actor::Assistant, EventKind::ToolCall);
    ev.tool_name = Some("Bash".into());
    ev.tool_use_id = Some(format!("tu_{i:03}"));
    ev.payload = json!({
        "content_ordinal": 0,
        "tool_name": "Bash",
        "input": { "command": command }
    });
    ev
}

// ---------------------------------------------------------------------------
// Real payload shape — TDD guard for pointer bug fix
// Command is at /input/command (NOT /tool_use/input/command).
// These tests use the mapping.rs:193 shape and must be RED before the fix,
// GREEN after.
// ---------------------------------------------------------------------------

#[test]
fn fires_on_rm_rf_real_shape() {
    let events = vec![bash_tool_call_real_shape(0, "rm -rf /tmp/foo")];
    let view = synth_view_with_bash(&events, &[]);
    let cands = detect(&view);
    assert_eq!(cands.len(), 1, "rm -rf (real shape) must fire risky_action");
    assert_eq!(cands[0].facts["trigger"]["kind"], json!("destructive_bash"));
}

#[test]
fn fires_on_git_push_force_real_shape() {
    let events = vec![bash_tool_call_real_shape(0, "git push --force origin main")];
    let view = synth_view_with_bash(&events, &[]);
    let cands = detect(&view);
    assert_eq!(cands.len(), 1, "git push --force (real shape) must fire");
}

#[test]
fn fires_on_git_reset_hard_real_shape() {
    let events = vec![bash_tool_call_real_shape(0, "git reset --hard HEAD~1")];
    let view = synth_view_with_bash(&events, &[]);
    let cands = detect(&view);
    assert_eq!(cands.len(), 1, "git reset --hard (real shape) must fire");
}

#[test]
fn does_not_fire_on_safe_bash_real_shape() {
    let events = vec![bash_tool_call_real_shape(0, "ls -la")];
    let view = synth_view_with_bash(&events, &[]);
    let cands = detect(&view);
    assert!(cands.is_empty(), "ls -la (real shape) must not fire");
}

// ---------------------------------------------------------------------------
// Destructive Bash branch
// ---------------------------------------------------------------------------

#[test]
fn fires_on_rm_rf() {
    let events = vec![bash_tool_call(0, "rm -rf /tmp/foo")];
    let view = synth_view_with_bash(&events, &[]);
    let cands = detect(&view);
    assert_eq!(cands.len(), 1, "rm -rf must fire risky_action");
    let c = &cands[0];
    assert_eq!(c.detector, "risky_action");
    assert!(!c.evidence_refs.is_empty(), "evidence_refs must be non-empty");
    assert!(c.subkind.is_none());
    // No severity/confidence judgment leaks into the facts.
    assert!(c.facts.get("severity").is_none());
    assert_eq!(c.facts["trigger"]["kind"], json!("destructive_bash"));
}

#[test]
fn fires_on_git_push_force() {
    let events = vec![bash_tool_call(0, "git push --force origin main")];
    let view = synth_view_with_bash(&events, &[]);
    let cands = detect(&view);
    assert_eq!(cands.len(), 1, "git push --force must fire");
}

#[test]
fn fires_on_git_push_dash_f() {
    let events = vec![bash_tool_call(0, "git push -f")];
    let view = synth_view_with_bash(&events, &[]);
    let cands = detect(&view);
    assert_eq!(cands.len(), 1, "git push -f must fire");
}

#[test]
fn fires_on_git_reset_hard() {
    let events = vec![bash_tool_call(0, "git reset --hard HEAD~1")];
    let view = synth_view_with_bash(&events, &[]);
    let cands = detect(&view);
    assert_eq!(cands.len(), 1, "git reset --hard must fire");
}

#[test]
fn fires_on_sudo_rm() {
    let events = vec![bash_tool_call(0, "sudo rm -rf /var/log")];
    let view = synth_view_with_bash(&events, &[]);
    let cands = detect(&view);
    assert_eq!(cands.len(), 1, "sudo rm must fire");
}

#[test]
fn does_not_fire_on_safe_bash_ls() {
    let events = vec![bash_tool_call(0, "ls -la")];
    let view = synth_view_with_bash(&events, &[]);
    let cands = detect(&view);
    assert!(cands.is_empty(), "ls -la must not fire");
}

#[test]
fn does_not_fire_on_safe_bash_grep() {
    let events = vec![bash_tool_call(0, "grep -r 'todo' .")];
    let view = synth_view_with_bash(&events, &[]);
    let cands = detect(&view);
    assert!(cands.is_empty(), "grep must not fire");
}

#[test]
fn does_not_fire_on_non_bash_tool() {
    // Real mapping.rs:193 payload shape for a Read tool_call
    let mut ev = base_event(0, Actor::Assistant, EventKind::ToolCall);
    ev.tool_name = Some("Read".into());
    ev.payload = json!({
        "content_ordinal": 0,
        "tool_name": "Read",
        "input": { "file_path": "/tmp/foo" }
    });
    let events = vec![ev];
    let view = synth_view_with_bash(&events, &[]);
    let cands = detect(&view);
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
    let cands = detect(&view);
    assert_eq!(cands.len(), 1, "user_modified hunk must fire");
    let c = &cands[0];
    assert_eq!(c.detector, "risky_action");
    assert_eq!(c.facts["trigger"]["kind"], json!("user_modified_hunk"));
}

#[test]
fn does_not_fire_on_non_user_modified_hunk() {
    let events = vec![base_event(0, Actor::Assistant, EventKind::ToolCall)];
    let diff_hunks = vec![diff_hunk_not_user_modified("dh_001", "ev_000")];
    let view = synth_view_with_bash(&events, &diff_hunks);
    let cands = detect(&view);
    assert!(cands.is_empty(), "non-user-modified hunk must not fire");
}

// ---------------------------------------------------------------------------
// UTF-8 multibyte safety (fix: byte-slice on &str must not panic on non-ASCII)
// ---------------------------------------------------------------------------

/// A Bash tool_call whose command contains Korean (multibyte UTF-8) and is
/// longer than 80 chars so the `&command[..command.len().min(80)]` slice path is
/// reached.  Before the fix this panics if byte offset 80 lands mid-codepoint.
/// After the fix it must not panic and must return exactly one signal.
#[test]
fn does_not_panic_on_multibyte_utf8_command() {
    // "rm -rf " is 7 bytes; Korean chars are 3 bytes each.
    // We want the total > 80 bytes but the 80th byte to land inside a Korean char.
    // "rm -rf " (7) + "한" (3) * 25 = 7 + 75 = 82 bytes total — 80th byte is
    // byte 80 which is the 2nd byte of the 25th "한" (0xED 0xED 0x95), i.e. mid-char.
    let command = format!("rm -rf {}", "한".repeat(25));
    assert!(command.len() > 80, "command must exceed 80 bytes for the slice to be reached");

    let events = vec![bash_tool_call(0, &command)];
    let view = synth_view_with_bash(&events, &[]);
    // Must not panic; must fire exactly one signal.
    let cands = detect(&view);
    assert_eq!(cands.len(), 1, "Korean UTF-8 destructive command must fire one signal without panic");
    assert_eq!(cands[0].facts["trigger"]["kind"], serde_json::json!("destructive_bash"));
}

// ---------------------------------------------------------------------------
// Empty session
// ---------------------------------------------------------------------------

#[test]
fn does_not_fire_on_empty_session() {
    let view = synth_view_with_bash(&[], &[]);
    let cands = detect(&view);
    assert!(cands.is_empty());
}

// ---------------------------------------------------------------------------
// Facts fields
// ---------------------------------------------------------------------------

#[test]
fn facts_include_required_fields_for_destructive_bash() {
    let events = vec![bash_tool_call(0, "rm -rf /tmp/foo")];
    let view = synth_view_with_bash(&events, &[]);
    let cands = detect(&view);
    assert_eq!(cands.len(), 1);
    let proj = &cands[0].facts;
    assert!(proj["trigger"]["kind"].is_string());
    assert!(proj["trigger"]["command_redacted"].is_string());
}

#[test]
fn facts_include_required_fields_for_user_modified() {
    let events = vec![base_event(0, Actor::Assistant, EventKind::ToolCall)];
    let diff_hunks = vec![diff_hunk_user_modified("dh_001", "ev_000")];
    let view = synth_view_with_bash(&events, &diff_hunks);
    let cands = detect(&view);
    assert_eq!(cands.len(), 1);
    let proj = &cands[0].facts;
    assert_eq!(proj["trigger"]["kind"], "user_modified_hunk");
}
