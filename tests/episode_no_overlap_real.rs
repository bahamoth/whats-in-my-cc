//! insight-redesign #4 — real-fixture invariant net for the episode classifier.
//!
//! For every frozen real transcript, the classifier output must:
//!   - partition events: no event_id appears in two episodes;
//!   - be time-monotonic by started_at with ended_at >= started_at;
//!   - carry non-empty evidence_node_ids on every episode.
//! These held vacuously pre-fix on the short fixtures but are the regression
//! net for the long-session double-classification bug (spec §6.4).

use std::collections::{HashMap, HashSet};

use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::{migrate, repo_observed, repo_verification_run};
use witmcc::ingest::store;
use witmcc::insight::episode::classifier::classify_session;
use witmcc::live::NoopSink;
use witmcc::model::observed::ObservedEvent;

async fn load(
    fixture: &str,
) -> (
    String,
    Vec<ObservedEvent>,
    Vec<witmcc::db::repo_verification_run::VerificationRunRow>,
) {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let path = format!("tests/fixtures/transcripts/real/{fixture}.jsonl");
    store::ingest_file(&pool, std::path::Path::new(&path), &NoopSink)
        .await
        .unwrap();
    let sessions = repo_observed::list_sessions(&pool, 10).await.unwrap();
    assert!(!sessions.is_empty(), "fixture {fixture} produced no sessions");
    let sid = sessions[0].session_id.clone();
    let evs = repo_observed::list_session(&pool, &sid, 100_000).await.unwrap();
    let runs = repo_verification_run::list_session(&pool, &sid)
        .await
        .unwrap_or_default();
    (sid, evs, runs)
}

fn assert_invariants(
    sid: &str,
    evs: &[ObservedEvent],
    runs: &[witmcc::db::repo_verification_run::VerificationRunRow],
) {
    let eps = classify_session(sid, evs, runs);
    assert!(!eps.is_empty(), "non-empty stream must yield episodes");

    let idx: HashMap<&str, usize> = evs
        .iter()
        .enumerate()
        .map(|(i, e)| (e.event_id.as_str(), i))
        .collect();

    let mut seen: HashSet<usize> = HashSet::new();
    let mut prev_start: Option<chrono::DateTime<chrono::Utc>> = None;
    for e in &eps {
        let s = idx[e.start_event_id.as_str()];
        let t = idx[e.end_event_id.as_str()];
        assert!(s <= t, "{sid}: start idx {s} > end idx {t}");
        for i in s..=t {
            assert!(seen.insert(i), "{sid}: event idx {i} in two episodes (overlap)");
        }
        assert!(
            e.ended_at >= e.started_at,
            "{sid}: negative/zero-duration episode {:?}",
            e.phase
        );
        if let Some(p) = prev_start {
            assert!(e.started_at >= p, "{sid}: episodes not monotonic by started_at");
        }
        prev_start = Some(e.started_at);
        assert!(!e.evidence_node_ids.is_empty(), "{sid}: empty evidence on {:?}", e.phase);
    }
    assert_eq!(seen.len(), evs.len(), "{sid}: events not fully partitioned");
}

#[tokio::test]
async fn verification_v01_invariants() {
    let (sid, evs, runs) = load("verification_v01").await;
    assert_invariants(&sid, &evs, &runs);
}

#[tokio::test]
async fn structured_patch_v01_invariants() {
    let (sid, evs, runs) = load("structured_patch_v01").await;
    assert_invariants(&sid, &evs, &runs);
}
