//! Slice-16 — unit tests for the `ContextBloat` L1 extractor.
//! All tests use synthetic `SessionInsightView` data — no DB, no I/O.

use chrono::{TimeZone, Utc};
use serde_json::json;
use wimcc::insight::config::DetectorConfig;
use wimcc::insight::extractor::Detector;
use wimcc::insight::extractors::context_bloat::ContextBloat;
use wimcc::insight::view::SessionInsightView;
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

/// Build a large (>50KB) tool_result event at index `i`.
fn large_tool_result(i: usize, size_bytes: usize) -> ObservedEvent {
    let big_content = "X".repeat(size_bytes);
    let mut ev = base_event(i, Actor::Tool, EventKind::ToolResult);
    ev.tool_name = Some("Grep".into());
    ev.tool_use_id = Some(format!("tu_{i:03}"));
    ev.payload = json!({
        "tool_result": {
            "tool_use_id": format!("tu_{i:03}"),
            "content": big_content,
            "is_error": false
        }
    });
    ev
}

/// Build a short assistant message at index `i`.
fn short_assistant_msg(i: usize) -> ObservedEvent {
    let mut ev = base_event(i, Actor::Assistant, EventKind::AssistantMessage);
    ev.payload = json!({ "message": { "content": [{"type":"text","text":"ok"}] } });
    ev
}

/// Build a tool_call event at index `i` with specific input (for overlap testing).
fn tool_call_with_input(i: usize, input_text: &str) -> ObservedEvent {
    let mut ev = base_event(i, Actor::Assistant, EventKind::ToolCall);
    ev.tool_name = Some("Bash".into());
    ev.tool_use_id = Some(format!("tu_{i:03}"));
    ev.payload = json!({
        "tool_use": {
            "name": "Bash",
            "input": { "command": input_text }
        }
    });
    ev
}

fn empty_view<'a>(events: &'a [ObservedEvent]) -> SessionInsightView<'a> {
    SessionInsightView {
        session_id: "sess_t",
        events,
        diff_hunks: &[],
        verification_runs: &[],
    }
}

// ---------------------------------------------------------------------------
// Core firing rule: large tool_result + next assistant_message + no downstream use
// ---------------------------------------------------------------------------

/// 100KB output followed by a short assistant message with no downstream overlap → fires.
#[test]
fn fires_on_large_tool_result_with_no_downstream_use() {
    let events = vec![
        large_tool_result(0, 100_000),
        short_assistant_msg(1),
    ];
    let view = empty_view(&events);
    let cands = ContextBloat.detect(&view, &DetectorConfig::default());
    assert_eq!(cands.len(), 1, "100KB bloat with short assistant reply must fire");
    let c = &cands[0];
    assert_eq!(c.detector, "context_bloat");
    assert!(c.subkind.is_none());
    // No severity/confidence judgment leaks into the facts.
    assert!(c.facts.get("severity").is_none());
    assert!(c.facts["tool_result"]["payload_size_bytes"].is_number());
}

/// Below the 50KB threshold → no fire.
#[test]
fn does_not_fire_below_threshold() {
    let events = vec![
        large_tool_result(0, 10_000), // 10KB — below 50KB threshold
        short_assistant_msg(1),
    ];
    let view = empty_view(&events);
    let cands = ContextBloat.detect(&view, &DetectorConfig::default());
    assert!(cands.is_empty(), "10KB output must not fire context_bloat");
}

/// Large tool_result but the immediately following tool_call references content from it
/// (≥ 3 stem overlap) → no fire (bloat was reused).
#[test]
fn does_not_fire_when_downstream_uses_content() {
    // Build a 100KB result with specific content words
    let big_content = "alpha_word beta_word gamma_word ".repeat(3200); // ~100KB
    let mut result_ev = base_event(0, Actor::Tool, EventKind::ToolResult);
    result_ev.tool_name = Some("Grep".into());
    result_ev.tool_use_id = Some("tu_000".into());
    result_ev.payload = json!({
        "tool_result": {
            "tool_use_id": "tu_000",
            "content": big_content,
            "is_error": false
        }
    });

    let assistant_ev = short_assistant_msg(1);

    // The downstream tool_call references 3 stems from the bloat content
    let downstream_call = tool_call_with_input(2, "process alpha_word beta_word gamma_word data");

    let events = vec![result_ev, assistant_ev, downstream_call];
    let view = empty_view(&events);
    let cands = ContextBloat.detect(&view, &DetectorConfig::default());
    assert!(cands.is_empty(), "downstream use with ≥3 stems must not fire");
}

/// Large result but no following assistant message within M=3 events → no fire.
#[test]
fn does_not_fire_without_following_assistant_message() {
    let events = vec![
        large_tool_result(0, 100_000),
        // followed only by more tool calls — no assistant message in next 3 events
        tool_call_with_input(1, "another tool call"),
        tool_call_with_input(2, "yet another"),
        tool_call_with_input(3, "and another"),
    ];
    let view = empty_view(&events);
    // No assistant_message in next 3 events → does not fire per spec §4.3
    let cands = ContextBloat.detect(&view, &DetectorConfig::default());
    // The bloat has no following assistant_message in M=3, so no candidate
    assert!(cands.is_empty(), "no assistant message in next 3 events → no fire");
}

/// Empty session → no fire.
#[test]
fn does_not_fire_on_empty_session() {
    let view = empty_view(&[]);
    let cands = ContextBloat.detect(&view, &DetectorConfig::default());
    assert!(cands.is_empty());
}

// ---------------------------------------------------------------------------
// Facts fields
// ---------------------------------------------------------------------------

#[test]
fn facts_include_required_fields() {
    let events = vec![
        large_tool_result(0, 100_000),
        short_assistant_msg(1),
    ];
    let view = empty_view(&events);
    let cands = ContextBloat.detect(&view, &DetectorConfig::default());
    assert_eq!(cands.len(), 1);
    let proj = &cands[0].facts;
    assert!(proj["tool_result"]["event_id"].is_string());
    assert!(proj["tool_result"]["payload_size_bytes"].is_number());
    assert!(proj["next_assistant"]["event_id"].is_string());
    assert!(proj["downstream_usage_signal"]["lexical_overlap_with_next_tool_calls"].is_number());
}

// ---------------------------------------------------------------------------
// Config-driven threshold
// ---------------------------------------------------------------------------

/// The byte threshold is config-driven: a 10KB result is below the default 50KB
/// (no fire) but a config that lowers `threshold_bytes` to 5KB makes it fire.
#[test]
fn threshold_bytes_from_config() {
    let events = vec![
        large_tool_result(0, 10_000),
        short_assistant_msg(1),
    ];
    let view = empty_view(&events);
    assert!(
        ContextBloat.detect(&view, &DetectorConfig::default()).is_empty(),
        "10KB is below default 50KB threshold"
    );
    let cfg = DetectorConfig::from_toml_str("[detector.context_bloat]\nthreshold_bytes = 5000\n");
    assert_eq!(
        ContextBloat.detect(&view, &cfg).len(),
        1,
        "lowering threshold_bytes to 5000 makes 10KB fire"
    );
}
