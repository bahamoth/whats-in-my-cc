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
/// Uses the REAL mapping.rs:193 payload shape:
/// `{"content_ordinal": N, "tool_name": "Bash", "input": {"command": ...}}`.
/// Command is at /input/command — the pointer context_bloat reads.
fn tool_call_with_input(i: usize, input_text: &str) -> ObservedEvent {
    let mut ev = base_event(i, Actor::Assistant, EventKind::ToolCall);
    ev.tool_name = Some("Bash".into());
    ev.tool_use_id = Some(format!("tu_{i:03}"));
    ev.payload = json!({
        "content_ordinal": 0,
        "tool_name": "Bash",
        "input": { "command": input_text }
    });
    ev
}

/// Alias with an explicit name for real-shape new tests (same shape as tool_call_with_input).
fn tool_call_real_shape(i: usize, input_text: &str) -> ObservedEvent {
    tool_call_with_input(i, input_text)
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
    let events = vec![large_tool_result(0, 100_000), short_assistant_msg(1)];
    let view = empty_view(&events);
    let cands = ContextBloat.detect(&view, &DetectorConfig::default());
    assert_eq!(
        cands.len(),
        1,
        "100KB bloat with short assistant reply must fire"
    );
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
    assert!(
        cands.is_empty(),
        "downstream use with ≥3 stems must not fire"
    );
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
    assert!(
        cands.is_empty(),
        "no assistant message in next 3 events → no fire"
    );
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
    let events = vec![large_tool_result(0, 100_000), short_assistant_msg(1)];
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

// ---------------------------------------------------------------------------
// UTF-8 multibyte safety (fix: byte-slice on &str must not panic on non-ASCII)
// ---------------------------------------------------------------------------

/// A tool_result whose content contains Korean (multibyte UTF-8) and is large
/// enough (> 50KB AND > 256 chars) to hit the `payload_tail` slice path.
/// Before the fix `&content[content.len() - 256..]` would panic on a byte
/// boundary that splits a multibyte codepoint.  After the fix it must not panic
/// and must return exactly one signal.
#[test]
fn does_not_panic_on_multibyte_utf8_content() {
    // Each Korean char is 3 UTF-8 bytes. "한국어 테스트 " is 24 bytes but 8 chars.
    // Repeat enough to exceed 50KB threshold AND ensure content.len() > 256.
    let korean_chunk = "한국어 테스트 데이터 블로트 감지기 테스트 케이스 ";
    // ~70 bytes per chunk * 800 reps ≈ 56KB (well above 50KB)
    let big_content = korean_chunk.repeat(800);
    // Sanity: len in bytes > 50KB and > 256
    assert!(big_content.len() > 50_000);
    assert!(big_content.len() > 256);

    let mut result_ev = base_event(0, Actor::Tool, EventKind::ToolResult);
    result_ev.tool_name = Some("Bash".into());
    result_ev.tool_use_id = Some("tu_000".into());
    result_ev.payload = serde_json::json!({
        "tool_result": {
            "tool_use_id": "tu_000",
            "content": big_content,
            "is_error": false
        }
    });

    let events = vec![result_ev, short_assistant_msg(1)];
    let view = empty_view(&events);
    // Must not panic; must fire exactly one signal.
    let cands = ContextBloat.detect(&view, &DetectorConfig::default());
    assert_eq!(
        cands.len(),
        1,
        "Korean UTF-8 bloat must fire one signal without panic"
    );
}

/// The byte threshold is config-driven: a 10KB result is below the default 50KB
/// (no fire) but a config that lowers `threshold_bytes` to 5KB makes it fire.
#[test]
fn threshold_bytes_from_config() {
    let events = vec![large_tool_result(0, 10_000), short_assistant_msg(1)];
    let view = empty_view(&events);
    assert!(
        ContextBloat
            .detect(&view, &DetectorConfig::default())
            .is_empty(),
        "10KB is below default 50KB threshold"
    );
    let cfg = DetectorConfig::from_toml_str("[detector.context_bloat]\nthreshold_bytes = 5000\n");
    assert_eq!(
        ContextBloat.detect(&view, &cfg).len(),
        1,
        "lowering threshold_bytes to 5000 makes 10KB fire"
    );
}

// ---------------------------------------------------------------------------
// Real payload shape — TDD guard for downstream overlap pointer bug fix
// Downstream tool_call command is at /input/command (NOT /tool_use/input/command).
// These tests use the mapping.rs:193 shape and must be RED before the fix,
// GREEN after.
// ---------------------------------------------------------------------------

/// Large bloat + short assistant + downstream tool_call using REAL shape with
/// ≥3 stem overlap → downstream use detected, must NOT fire (overlap suppresses).
/// Before the fix the real-shape pointer is missed → fires incorrectly.
#[test]
fn does_not_fire_when_downstream_real_shape_overlaps() {
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
    // Real payload shape: /input/command (not /tool_use/input/command)
    let downstream_call = tool_call_real_shape(2, "process alpha_word beta_word gamma_word data");
    let events = vec![result_ev, assistant_ev, downstream_call];
    let view = empty_view(&events);
    // With the buggy pointer the real-shape downstream input reads as "" →
    // overlap = 0 → fires. After fix overlap = 3 → no fire.
    let cands = ContextBloat.detect(&view, &DetectorConfig::default());
    assert!(
        cands.is_empty(),
        "real-shape downstream use with ≥3 stems must suppress context_bloat"
    );
}

/// Large bloat + short assistant + downstream tool_call using REAL shape with
/// NO overlap → must fire (same as no-downstream case but verifying real shape
/// still triggers properly when there is no reuse).
#[test]
fn fires_on_large_bloat_downstream_real_shape_no_overlap() {
    let events = vec![
        large_tool_result(0, 100_000),
        short_assistant_msg(1),
        tool_call_real_shape(2, "unrelated command here"),
    ];
    let view = empty_view(&events);
    let cands = ContextBloat.detect(&view, &DetectorConfig::default());
    assert_eq!(
        cands.len(),
        1,
        "100KB bloat with real-shape downstream (no overlap) must fire"
    );
}

// ---------------------------------------------------------------------------
// Dogfood 2026-06-12 (retrospect §4) — signal quality fixes, locked here.
// Real case: session 191eddf3 fired context_bloat on a sidechain Read whose
// tool_result row carried an empty tool_name → summary said `from ""` and the
// sidechain context (a *recommended* delegation pattern) was invisible.
// ---------------------------------------------------------------------------

/// tool_result with empty tool_name + a paired tool_call (same tool_use_id)
/// named Read → facts/summary must resolve the name from the pairing.
#[test]
fn resolves_empty_tool_name_from_paired_tool_call() {
    let mut call = tool_call_with_input(0, "unrelated input");
    call.tool_name = Some("Read".into());
    call.tool_use_id = Some("tu_pair".into());
    let mut result = large_tool_result(1, 100_000);
    result.tool_name = Some("".into()); // observed-empty — the dogfood case
    result.tool_use_id = Some("tu_pair".into());
    let events = vec![call, result, short_assistant_msg(2)];
    let view = empty_view(&events);
    let cands = ContextBloat.detect(&view, &DetectorConfig::default());
    assert_eq!(cands.len(), 1);
    let c = &cands[0];
    assert_eq!(
        c.facts["tool_result"]["tool_name"], "Read",
        "empty tool_name must be resolved from the paired tool_call"
    );
    assert!(
        c.summary.contains("Read"),
        "summary must name the resolved tool, got: {}",
        c.summary
    );
}

/// Unresolvable empty tool_name (no paired call) → "unknown", never `""`.
#[test]
fn unresolvable_tool_name_reads_unknown_not_empty() {
    let mut result = large_tool_result(0, 100_000);
    result.tool_name = Some("".into());
    result.tool_use_id = None;
    let events = vec![result, short_assistant_msg(1)];
    let view = empty_view(&events);
    let cands = ContextBloat.detect(&view, &DetectorConfig::default());
    assert_eq!(cands.len(), 1);
    assert!(
        cands[0].summary.contains("unknown"),
        "summary must say unknown, got: {}",
        cands[0].summary
    );
    assert!(
        !cands[0].summary.contains("\"\""),
        "summary must never print an empty quoted name, got: {}",
        cands[0].summary
    );
}

/// Sidechain bloat (Agent delegation reading big docs) must be visible in
/// facts so consumers can judge it as the recommended pattern it usually is.
#[test]
fn facts_carry_is_sidechain_flag() {
    let mut result = large_tool_result(0, 100_000);
    result.is_sidechain = true;
    let events = vec![result, short_assistant_msg(1)];
    let view = empty_view(&events);
    let cands = ContextBloat.detect(&view, &DetectorConfig::default());
    assert_eq!(cands.len(), 1);
    assert_eq!(
        cands[0].facts["tool_result"]["is_sidechain"], true,
        "facts must expose is_sidechain"
    );
}
