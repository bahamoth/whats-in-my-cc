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
async fn tsc_typecheck_is_not_detected_honest_gap() {
    // `tsc` is NOT on the Tier-1 allowlist and carries no test/spec keyword,
    // so it is intentionally not detected. Promoting tsc to Tier-1 is a future
    // change that needs its own fixture (DEV-S11-03). Document the gap here.
    let runs = load_runs().await;
    assert!(
        run_for_kind(&runs, "tsc").is_none(),
        "tsc type-check is an honest detection gap, not a guard"
    );
}

#[tokio::test]
async fn piped_to_nonpager_yields_unknown_status() {
    // Synthetic piped-to-grep case locking the status_basis=piped → unknown
    // rule at the extractor level. (Isolates the pipe-masking semantics on a
    // known tool; no genuine `npm test | grep` line exists in this user's
    // transcripts, so this case is explicitly synthetic.)
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
    store::ingest_file(&pool, f.path(), &NoopSink).await.unwrap();
    let evs = repo_observed::list_session(&pool, session, 1000).await.unwrap();
    let runs = extract_verification_runs(&evs);
    let m = runs
        .iter()
        .find(|r| r.command.contains("npm test"))
        .expect("npm test detected even when piped");
    assert_eq!(m.command_kind, "test_suite_js");
    assert_eq!(m.status_basis, "piped");
    assert_eq!(m.status, "unknown", "pipe masks exit → status unknown");
}
