//! Slice insight-surface-redesign #2 — real-fixture invariants for the
//! segment-split verification detector.
//!
//! Real-data anchoring (CLAUDE.md): the headline cases in
//! `tests/fixtures/transcripts/real/verification_npx_v01.jsonl` are GENUINE
//! captured lines from this project's transcripts, asserted individually —
//! NOT generalised from one line:
//!   - `cd .../webui\nnpx vitest run src/api/__tests__/client.endpoints.test.ts 2>&1 | tail -12`
//!     is a real line from session 653ea169 (the session the design spec §1.2
//!     calls out as reading verification 0%; is_error=false).
//!   - `npx tsc -b 2>&1 | tail -20` is a real line from session eb70234e
//!     (is_error=false) — documents the honest gap that `tsc` is not detected.
//!
//! The dry-run (`cargo test --no-run`) line is a SYNTHETIC curated case (no
//! genuine sample exists in this user's transcripts); its semantics are also
//! locked by the in-crate unit tests in `src/insight/verification_allowlist.rs`
//! and `src/ingest/verification_run.rs`.
//!
//! The former keyword-tier line (`./scripts/run_smoke_test.sh`) and its test
//! were removed with the Tier-2 keyword fallback (spec F2): the extractor now
//! only emits `"known_tool"` (deterministic allowlist), so a synthetic
//! keyword-only command is no longer detected.

use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::{migrate, repo_observed};
use wimcc::ingest::{store, verification_run::extract_verification_runs};
use wimcc::live::NoopSink;

const SESSION: &str = "npx0001-aaaa-bbbb-cccc-000000000001";
const FIXTURE: &str = "tests/fixtures/transcripts/real/verification_npx_v01.jsonl";

async fn load_runs() -> Vec<wimcc::ingest::verification_run::VerificationRunRecord> {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(&pool, std::path::Path::new(FIXTURE), &NoopSink)
        .await
        .unwrap();
    let evs = repo_observed::list_session(&pool, SESSION, 100_000)
        .await
        .unwrap();
    extract_verification_runs(&evs)
}

fn run_for_kind<'a>(
    runs: &'a [wimcc::ingest::verification_run::VerificationRunRecord],
    contains: &str,
) -> Option<&'a wimcc::ingest::verification_run::VerificationRunRecord> {
    runs.iter().find(|r| r.command.contains(contains))
}

#[tokio::test]
async fn cd_npx_vitest_is_detected_as_known_tool_test_suite_js() {
    // THE headline bug fix: `cd .../webui\nnpx vitest run … 2>&1 | tail -12`
    // was 0 runs before (the leading `cd` swallowed the command). Now the
    // newline-split picks the `npx vitest run …` segment.
    let runs = load_runs().await;
    let m = run_for_kind(&runs, "vitest").expect("vitest run must be detected");
    assert_eq!(m.command_kind, "test_suite_js");
    assert_eq!(m.detection_basis, "known_tool");
    // matched segment is the npx vitest segment (cd dropped); the redirect is
    // retained in `command`, the trailing `| tail` is a separate segment.
    assert_eq!(
        m.command,
        "npx vitest run src/api/__tests__/client.endpoints.test.ts 2>&1"
    );
    // `… 2>&1 | tail` is an output-capture idiom (tail is a pager) → exit basis.
    assert_eq!(m.status_basis, "exit");
    // Plan 6: no OTLP/hook/exit-code in this transcript-only fixture →
    // status is "unknown" (is_error=false is no longer used for pass/fail).
    // Content is just a tail fragment: no failure pattern, no "exit code:" text.
    assert_eq!(m.status, "unknown");
}

#[tokio::test]
async fn dry_run_no_run_is_excluded() {
    // Slice directive #6 (OVERRIDES the plan's "keep dry-run" decision):
    // `cargo test --no-run` compiles tests but does NOT run them, so it is not
    // a verification *run* and must be excluded.
    let runs = load_runs().await;
    assert!(
        run_for_kind(&runs, "--no-run").is_none(),
        "cargo test --no-run is a dry-run compile, not a verification run"
    );
}

#[tokio::test]
async fn tsc_build_is_now_detected_pattern_17() {
    // 종전에는 "정직한 갭"(미탐지)이었다 — B-2(2026-07-04)가 DEV-S11-03
    // 절차대로 실 fixture(verification_tsc_v01.jsonl, 3세션)와 함께 tsc를
    // Tier-1 패턴 17로 승격했다. 이 fixture의 `npx tsc -b …`(eb70234e 실
    // 라인)는 이제 build로 탐지된다. --noEmit 형태의 build_check 분기는
    // tests/verification_tsc.rs가 잠근다.
    let runs = load_runs().await;
    let r = run_for_kind(&runs, "tsc").expect("tsc run detected since pattern 17");
    assert_eq!(r.command_kind, "build");
    assert_eq!(r.detection_basis, "known_tool");
}

#[tokio::test]
async fn piped_to_nonpager_yields_unknown_status() {
    // Synthetic piped-to-grep case. The content "FAIL src/x.test.ts" carries NO
    // recognizable success/failure SUMMARY (looks_like_failure matches "FAILED",
    // "Tests failed", … — not a bare "FAIL <path>"), so the piped run stays
    // unknown. Since the 2026-06-11 relaxation, piped only stays unknown when no
    // summary survives the pipe (here it doesn't); a recognized summary upgrades
    // to estimated. status_basis stays "piped" either way.
    use std::io::Write;
    use tempfile::NamedTempFile;
    let session = "s_piped_unknown";
    let assistant = serde_json::json!({
        "type":"assistant","sessionId":session,"uuid":"a1","parentUuid":null,
        "timestamp":"2026-05-30T10:00:00Z","cwd":"/tmp","userType":"external",
        "entrypoint":"cli","version":"2.1.154",
        "message":{"role":"assistant","model":"claude-opus-4-8","content":[
            {"type":"tool_use","id":"toolu_p","name":"Bash",
             "input":{"command":"npm test | grep FAIL"}}]}
    });
    let result = serde_json::json!({
        "type":"user","sessionId":session,"uuid":"u1","parentUuid":"a1",
        "timestamp":"2026-05-30T10:00:05Z","cwd":"/tmp","userType":"external",
        "entrypoint":"cli",
        "message":{"role":"user","content":[
            {"tool_use_id":"toolu_p","type":"tool_result","is_error":false,
             "content":"FAIL src/x.test.ts"}]}
    });
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "{assistant}").unwrap();
    writeln!(f, "{result}").unwrap();

    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(&pool, f.path(), &NoopSink)
        .await
        .unwrap();
    let evs = repo_observed::list_session(&pool, session, 1000)
        .await
        .unwrap();
    let runs = extract_verification_runs(&evs);
    let m = runs
        .iter()
        .find(|r| r.command.contains("npm test"))
        .expect("npm test detected even when piped");
    assert_eq!(m.command_kind, "test_suite_js");
    assert_eq!(m.status_basis, "piped");
    assert_eq!(
        m.status, "unknown",
        "pipe masks exit AND no summary survived → status unknown"
    );
}
