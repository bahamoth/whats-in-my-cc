//! insight-redesign #4 — episode classifier drift bug fix (spec §6.4).
//!
//! A stream of >= DRIFT_THRESHOLD (8) consecutive read-only tool_calls must
//! trigger a Drift episode WITHOUT double-classifying the boundary event.
//! Pre-fix, the Exploration episode ended at events[i] and the Drift episode
//! started at events[i] — the same event landed in two episodes (513 shared
//! event_ids in real session 653ea169), producing zero-duration / negative-gap
//! rows and empty evidence.

use chrono::{TimeZone, Utc};
use std::collections::HashSet;
use witmcc::insight::episode::classifier::classify_session;
use witmcc::insight::episode::types::Phase;
use witmcc::model::observed::{Actor, EventKind, ObservedEvent};

/// Mirror of the helper in classifier.rs tests + episode_classifier_basic.rs:
/// strictly-increasing observed_at, unique event_id "ev_{i:03}".
fn ev(i: usize, actor: Actor, kind: EventKind, tool: Option<&str>) -> ObservedEvent {
    ObservedEvent {
        event_id: format!("ev_{i:03}"),
        raw_event_id: format!("raw_{i:03}"),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_drift".into(),
        observed_at: Utc.timestamp_opt(1_700_000_000 + i as i64, 0).unwrap(),
        actor,
        kind,
        tool_name: tool.map(String::from),
        parser_version: "test".into(),
        ..Default::default()
    }
}

/// Build: 1 user message + N consecutive read-only Read tool_calls.
/// With N >= DRIFT_THRESHOLD the classifier must emit a Drift episode.
fn read_only_stream(n_reads: usize) -> Vec<ObservedEvent> {
    let mut evs = vec![ev(0, Actor::User, EventKind::UserMessage, None)];
    for k in 1..=n_reads {
        evs.push(ev(k, Actor::Assistant, EventKind::ToolCall, Some("Read")));
    }
    evs
}

#[test]
fn drift_triggers_after_threshold() {
    // 9 reads (> DRIFT_THRESHOLD=8) must produce at least one Drift episode.
    let evs = read_only_stream(9);
    let eps = classify_session("sess_drift", &evs, &[]);
    assert!(
        eps.iter().any(|e| e.phase == Phase::Drift),
        "expected a Drift episode; got {:?}",
        eps.iter().map(|e| e.phase).collect::<Vec<_>>()
    );
}

#[test]
fn episodes_do_not_share_event_ids() {
    // Each event_id must belong to exactly one episode. We reconstruct each
    // episode's covered index range from start_event_id..=end_event_id and
    // assert the ranges are pairwise disjoint.
    let evs = read_only_stream(9);
    let eps = classify_session("sess_drift", &evs, &[]);

    // Map event_id -> stream index.
    let idx: std::collections::HashMap<&str, usize> = evs
        .iter()
        .enumerate()
        .map(|(i, e)| (e.event_id.as_str(), i))
        .collect();

    let mut seen: HashSet<usize> = HashSet::new();
    for e in &eps {
        let s = idx[e.start_event_id.as_str()];
        let t = idx[e.end_event_id.as_str()];
        assert!(s <= t, "episode start index {s} must be <= end index {t}");
        for i in s..=t {
            assert!(
                seen.insert(i),
                "event index {i} ({}) appears in more than one episode; \
                 episodes={:?}",
                evs[i].event_id,
                eps.iter()
                    .map(|x| (x.phase, x.start_event_id.clone(), x.end_event_id.clone()))
                    .collect::<Vec<_>>()
            );
        }
    }
    // Every event must be covered exactly once.
    assert_eq!(seen.len(), evs.len(), "every event must belong to one episode");
}

#[test]
fn no_zero_or_negative_duration_and_monotonic() {
    let evs = read_only_stream(9);
    let eps = classify_session("sess_drift", &evs, &[]);
    let mut prev_start: Option<chrono::DateTime<Utc>> = None;
    for e in &eps {
        assert!(
            e.ended_at >= e.started_at,
            "episode {:?} has ended_at {} < started_at {}",
            e.phase,
            e.ended_at,
            e.started_at
        );
        if let Some(p) = prev_start {
            assert!(
                e.started_at >= p,
                "episodes must be time-monotonic by started_at"
            );
        }
        prev_start = Some(e.started_at);
    }
}

#[test]
fn every_episode_has_non_empty_evidence() {
    let evs = read_only_stream(9);
    let eps = classify_session("sess_drift", &evs, &[]);
    for e in &eps {
        assert!(
            !e.evidence_node_ids.is_empty(),
            "episode {:?} ({}..{}) has empty evidence_node_ids",
            e.phase,
            e.start_event_id,
            e.end_event_id
        );
    }
    // The drift episode specifically must carry evidence (spec §6.4 ask).
    let drift = eps
        .iter()
        .find(|e| e.phase == Phase::Drift)
        .expect("a drift episode");
    assert!(
        !drift.evidence_node_ids.is_empty(),
        "drift episode must have non-empty evidence"
    );
}
