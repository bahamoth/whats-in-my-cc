//! Slice-11 — extractor invariant tests (TDD red, Phase 1 commit 1).
//!
//! Tests the `extract_verification_runs` function against:
//! 1. The frozen real fixture `tests/fixtures/transcripts/real/verification_v01.jsonl`
//!    — 3 pairs from session aac68973: 2 passing `cargo test` + 1 passing
//!    `cargo build --release`. All 3 are "passed" status (no failed tests
//!    were observed in the real aac68973 transcript — see DEV-S11-06 in
//!    implementation-notes.html).
//! 2. A synthetic non-test Bash event (must produce zero runs).
//! 3. Determinism: two passes over the same events yield identical row IDs.

use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::{migrate, repo_observed};
use wimcc::ingest::{store, verification_run::extract_verification_runs};
use wimcc::live::NoopSink;

async fn load_fixture_events(path: &str) -> Vec<wimcc::model::observed::ObservedEvent> {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(&pool, std::path::Path::new(path), &NoopSink)
        .await
        .unwrap();
    // The real fixture only has one session
    let sessions = wimcc::db::repo_observed::list_sessions(&pool, 10)
        .await
        .unwrap();
    assert!(!sessions.is_empty(), "fixture produced no sessions");
    repo_observed::list_session(&pool, &sessions[0].session_id, 100_000)
        .await
        .unwrap()
}

#[tokio::test]
async fn extracts_verification_runs_from_real_bash_fixture() {
    // Real fixture: 3 pairs from aac68973 — 2 x cargo test, 1 x cargo build --release.
    // All 3 have is_error=false (passed). No real failed tests in this transcript
    // (DEV-S11-06: real transcript has no failing Bash test commands; all 3 are passed).
    let evs = load_fixture_events(
        "tests/fixtures/transcripts/real/verification_v01.jsonl",
    )
    .await;

    let runs = extract_verification_runs(&evs);

    assert_eq!(
        runs.len(),
        3,
        "expected 3 verification runs (2 test + 1 build), got {}; events count={}",
        runs.len(),
        evs.len()
    );

    let passed = runs.iter().filter(|r| r.status == "passed").count();
    let failed = runs.iter().filter(|r| r.status == "failed").count();
    assert_eq!(passed, 3, "all 3 real-fixture runs should be passed (no failures in aac68973)");
    assert_eq!(failed, 0, "no failed runs in real fixture");

    // Validate common fields on every run
    for r in &runs {
        assert!(!r.session_id.is_empty(), "session_id must not be empty");
        assert!(!r.trigger_event_id.is_empty(), "trigger_event_id must not be empty");
        assert!(!r.command.is_empty(), "command must not be empty");
        assert!(!r.command_kind.is_empty(), "command_kind must not be empty");
        assert!(
            ["bash", "hook", "otel"].contains(&r.source.as_str()),
            "source must be bash|hook|otel, got {:?}",
            r.source
        );
        assert_eq!(r.schema_version, "verification_run.v1");
        assert_eq!(r.parser_version, "verification_run@v1");
    }

    // Verify command kinds present
    let kinds: Vec<&str> = runs.iter().map(|r| r.command_kind.as_str()).collect();
    assert!(
        kinds.iter().any(|k| *k == "test_suite_rust"),
        "expected at least one test_suite_rust run; got {kinds:?}"
    );
    assert!(
        kinds.iter().any(|k| *k == "build"),
        "expected at least one build run; got {kinds:?}"
    );
}

#[tokio::test]
async fn produces_no_runs_for_non_test_bash() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    // Synthetic: git status tool call — must produce no verification runs
    let session_id = "s_vr_no_test";
    let tool_use_id = "toolu_git_status";

    let assistant = serde_json::json!({
        "type": "assistant",
        "sessionId": session_id,
        "uuid": "u_a_vr",
        "parentUuid": null,
        "timestamp": "2026-05-27T10:00:00Z",
        "cwd": "/tmp",
        "userType": "external",
        "entrypoint": "cli",
        "version": "2.1.146",
        "message": {
            "role": "assistant",
            "content": [{
                "type": "tool_use",
                "id": tool_use_id,
                "name": "Bash",
                "input": {"command": "git status"}
            }]
        }
    });
    let tool_result = serde_json::json!({
        "type": "user",
        "sessionId": session_id,
        "uuid": "u_u_vr",
        "parentUuid": "u_a_vr",
        "timestamp": "2026-05-27T10:00:01Z",
        "cwd": "/tmp",
        "userType": "external",
        "entrypoint": "cli",
        "message": {
            "role": "user",
            "content": [{
                "tool_use_id": tool_use_id,
                "type": "tool_result",
                "content": "On branch main"
            }]
        }
    });

    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "{assistant}").unwrap();
    writeln!(f, "{tool_result}").unwrap();

    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(&pool, f.path(), &NoopSink).await.unwrap();
    let evs = repo_observed::list_session(&pool, session_id, 1000)
        .await
        .unwrap();

    let runs = extract_verification_runs(&evs);
    assert!(
        runs.is_empty(),
        "non-test Bash commands must produce no verification runs; got {} runs",
        runs.len()
    );
}

#[tokio::test]
async fn deduplicates_by_trigger_event_id() {
    // Two calls to extract_verification_runs on the same events must produce
    // identical IDs (deterministic IDs keyed on trigger_event_id).
    let evs = load_fixture_events(
        "tests/fixtures/transcripts/real/verification_v01.jsonl",
    )
    .await;

    let runs1 = extract_verification_runs(&evs);
    let runs2 = extract_verification_runs(&evs);

    assert_eq!(runs1.len(), runs2.len(), "run counts must be deterministic");

    let ids1: Vec<&str> = runs1.iter().map(|r| r.verification_run_id.as_str()).collect();
    let ids2: Vec<&str> = runs2.iter().map(|r| r.verification_run_id.as_str()).collect();
    assert_eq!(ids1, ids2, "run IDs must be deterministic across two extractions");
}
