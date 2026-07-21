//! Slice-11 — OTel branch invariant test (synthetic — no real fixture).
//!
//! DEV-S11-01: The OTel verification branch (`OtelSpan` with
//! `attributes["verification.kind"]`) exists in the extractor code but
//! cannot be exercised with real data because no captured OTel span has
//! been observed carrying this attribute as of 2026-05-27. This test uses
//! synthetic `ObservedEvent` data to lock the branch surface so it is not
//! accidentally deleted as dead code.
//!
//! When a real fixture arrives, replace the synthetic events below with
//! the real fixture path and remove the "synthetic" note from the test names.

use chrono::Utc;
use serde_json::json;
use wimcc::ingest::verification_run::extract_verification_runs;
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

fn make_otel_verification_span(session_id: &str, vk: &str) -> ObservedEvent {
    ObservedEvent {
        event_id: "ev_otel_vk_001".into(),
        raw_event_id: "raw_otel_001".into(),
        schema_version: "0.5.0".into(),
        session_id: session_id.into(),
        observed_at: Utc::now(),
        actor: Actor::System,
        kind: EventKind::OtelSpan,
        trace_id: Some("trace_abc".into()),
        span_id: Some("span_xyz".into()),
        // Place the verification.kind attribute in the payload under the path
        // the extractor reads: /telemetry/attributes/verification.kind
        payload: json!({
            "telemetry": {
                "attributes": {
                    "verification.kind": vk
                }
            }
        }),
        parser_version: "otel@0.1.0".into(),
        ..Default::default()
    }
}

#[test]
fn otel_span_with_verification_kind_produces_run_synthetic() {
    // SYNTHETIC — no real OTel span with verification.kind has been observed.
    // This test locks the branch interface, not real-data behaviour.
    let session_id = "sess_otel_vk";
    let ev = make_otel_verification_span(session_id, "test_suite");
    let evs = vec![ev];

    let runs = extract_verification_runs(&evs);
    assert_eq!(
        runs.len(),
        1,
        "synthetic OTel span with verification.kind must produce 1 run; got {}",
        runs.len()
    );
    assert_eq!(runs[0].source, "otel");
    assert_eq!(runs[0].command, "test_suite");
    assert_eq!(runs[0].session_id, session_id);
}

#[test]
fn otel_span_without_verification_kind_produces_no_runs_synthetic() {
    // SYNTHETIC — OTel spans without verification.kind must be silently skipped.
    let session_id = "sess_otel_no_vk";
    let ev = ObservedEvent {
        event_id: "ev_otel_no_vk".into(),
        raw_event_id: "raw_otel_no_vk".into(),
        schema_version: "0.5.0".into(),
        session_id: session_id.into(),
        observed_at: Utc::now(),
        actor: Actor::System,
        kind: EventKind::OtelSpan,
        trace_id: Some("trace_def".into()),
        span_id: Some("span_uvw".into()),
        payload: json!({"telemetry": {"span_name": "some_other_span", "attributes": {}}}),
        parser_version: "otel@0.1.0".into(),
        ..Default::default()
    };

    let runs = extract_verification_runs(&[ev]);
    assert!(
        runs.is_empty(),
        "OTel spans without verification.kind must produce no runs"
    );
}

#[test]
fn otel_span_missing_trace_id_is_dropped_synthetic() {
    // SYNTHETIC — span with verification.kind but no trace_id/span_id is dropped per spec §7.
    let session_id = "sess_otel_no_ids";
    let ev = ObservedEvent {
        event_id: "ev_otel_no_ids".into(),
        raw_event_id: "raw_otel_no_ids".into(),
        schema_version: "0.5.0".into(),
        session_id: session_id.into(),
        observed_at: Utc::now(),
        actor: Actor::System,
        kind: EventKind::OtelSpan,
        trace_id: None, // absent
        span_id: None,  // absent
        payload: json!({"telemetry": {"attributes": {"verification.kind": "build"}}}),
        parser_version: "otel@0.1.0".into(),
        ..Default::default()
    };

    let runs = extract_verification_runs(&[ev]);
    assert!(
        runs.is_empty(),
        "OTel span with verification.kind but no trace_id must be dropped"
    );
}
