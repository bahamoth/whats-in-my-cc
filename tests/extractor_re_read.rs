//! Unit tests for the `ReRead` sequence detector (Plan 5).
//! All tests use synthetic `SessionInsightView` data — no DB, no I/O.
//!
//! Real ToolCall payload shape (verified against mapping.rs:193):
//!   {"content_ordinal": N, "tool_name": "Read", "input": {"file_path": "/path"}}
//! Pointer: /input/file_path

use chrono::{TimeZone, Utc};
use serde_json::json;
use wimcc::insight::config::DetectorConfig;
use wimcc::insight::extractor::Detector;
use wimcc::insight::extractors::re_read::ReRead;
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

/// Build a ToolCall event for the Read tool with the real payload shape
/// (matching mapping.rs: {"content_ordinal": i, "tool_name": "Read", "input": {"file_path": path}}).
fn read_call(i: usize, tid: &str, path: &str) -> ObservedEvent {
    ObservedEvent {
        tool_use_id: Some(tid.into()),
        tool_name: Some("Read".into()),
        payload: json!({
            "content_ordinal": i,
            "tool_name": "Read",
            "input": { "file_path": path }
        }),
        ..base_event(i, Actor::Assistant, EventKind::ToolCall)
    }
}

fn view_from_events(events: &[ObservedEvent]) -> SessionInsightView<'_> {
    SessionInsightView {
        session_id: "sess_t",
        events,
        diff_hunks: &[],
        verification_runs: &[],
    }
}

#[test]
fn fires_when_same_file_read_twice() {
    let events = vec![read_call(0, "tid0", "/a.rs"), read_call(1, "tid1", "/a.rs")];
    let cands = ReRead.detect(&view_from_events(&events), &DetectorConfig::default());
    assert_eq!(cands.len(), 1);
    assert_eq!(cands[0].detector, "re_read");
    assert_eq!(cands[0].evidence_refs.len(), 2);
    assert_eq!(cands[0].facts["file_path"], json!("/a.rs"));
    assert_eq!(cands[0].facts["read_count"], json!(2));
}

#[test]
fn no_fire_for_distinct_files() {
    let events = vec![read_call(0, "t0", "/a.rs"), read_call(1, "t1", "/b.rs")];
    assert_eq!(
        ReRead.detect(&view_from_events(&events), &DetectorConfig::default()).len(),
        0
    );
}

#[test]
fn min_reads_from_config() {
    let events = vec![read_call(0, "t0", "/a.rs"), read_call(1, "t1", "/a.rs")];
    let cfg = DetectorConfig::from_toml_str("[detector.re_read]\nmin_reads = 3\n");
    assert_eq!(ReRead.detect(&view_from_events(&events), &cfg).len(), 0);
}

/// Three reads of same file → fires with read_count = 3.
#[test]
fn fires_with_read_count_three() {
    let events = vec![
        read_call(0, "t0", "/src/main.rs"),
        read_call(1, "t1", "/src/main.rs"),
        read_call(2, "t2", "/src/main.rs"),
    ];
    let cands = ReRead.detect(&view_from_events(&events), &DetectorConfig::default());
    assert_eq!(cands.len(), 1);
    assert_eq!(cands[0].facts["read_count"], json!(3));
    assert_eq!(cands[0].evidence_refs.len(), 3);
}

/// Two different files both read twice → two signals.
#[test]
fn fires_for_each_re_read_path() {
    let events = vec![
        read_call(0, "t0", "/a.rs"),
        read_call(1, "t1", "/b.rs"),
        read_call(2, "t2", "/a.rs"),
        read_call(3, "t3", "/b.rs"),
    ];
    let cands = ReRead.detect(&view_from_events(&events), &DetectorConfig::default());
    assert_eq!(cands.len(), 2);
    let paths: Vec<&str> = cands.iter().map(|c| c.facts["file_path"].as_str().unwrap()).collect();
    assert!(paths.contains(&"/a.rs"));
    assert!(paths.contains(&"/b.rs"));
}

/// Non-Read tool calls (Bash, Edit) are ignored.
#[test]
fn ignores_non_read_tool_calls() {
    let events = vec![
        ObservedEvent {
            tool_use_id: Some("t0".into()),
            tool_name: Some("Bash".into()),
            payload: json!({"input": {"command": "ls"}}),
            ..base_event(0, Actor::Assistant, EventKind::ToolCall)
        },
        read_call(1, "t1", "/a.rs"),
    ];
    assert_eq!(
        ReRead.detect(&view_from_events(&events), &DetectorConfig::default()).len(),
        0
    );
}

/// ToolResult events are ignored (only ToolCall counted).
#[test]
fn ignores_non_tool_call_events() {
    let events = vec![
        ObservedEvent {
            tool_use_id: Some("t0".into()),
            payload: json!({"tool_result": {"is_error": false, "content": "/a.rs"}}),
            ..base_event(0, Actor::Assistant, EventKind::ToolResult)
        },
        read_call(1, "t1", "/a.rs"),
    ];
    // Only one ToolCall for /a.rs → no fire.
    assert_eq!(
        ReRead.detect(&view_from_events(&events), &DetectorConfig::default()).len(),
        0
    );
}

/// manifest id matches detector id.
#[test]
fn manifest_id_matches_detector_id() {
    let m = ReRead.manifest();
    assert_eq!(m.id, ReRead.id());
    assert_eq!(m.id, "re_read");
}

/// manifest config_keys includes min_reads.
#[test]
fn manifest_config_keys_includes_min_reads() {
    let m = ReRead.manifest();
    assert!(
        m.config_keys.contains(&"min_reads"),
        "config_keys must include min_reads; got: {:?}",
        m.config_keys
    );
}
