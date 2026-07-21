//! B-2 (BACKLOG) — tsc 검증 감지 실 fixture invariant.
//!
//! Real-data anchoring (DEV-S11-03): `verification_tsc_v01.jsonl`의 세 쌍은
//! 전부 이 머신의 실 세션에서 동결한 GENUINE 라인이다 (표본 3세션, 코퍼스
//! 전수 스캔에서 tsc 계열 40여 건 관측 — make/bazel/tox/ruff/eslint/ctest는
//! 미관측이라 이번 확장에서 제외):
//!   1. 190a23db — `cd …/webui && npx tsc --noEmit 2>&1 | head -20; echo …`
//!      (실패: `…(15,48): error TS2345: …` 진단 출력, is_error=false)
//!   2. 178fae97 — 멀티라인 스크립트 내부 `pnpm tsc --noEmit 2>&1 | tail -5`
//!      (성공: tsc 자체 출력 없음 — 사용자 echo만 남음 → 구조적 unknown 유지)
//!   3. 01fe9550 — `pnpm exec tsc -b 2>&1 | grep -E … | head -30; echo …`
//!      (실패: `…(99,82): error TS2741: …`; grep은 pager가 아니므로 piped)
//!
//! tsc 진단 포맷 `file(line,col): error TSxxxx:`는 위 두 실패 표본이 잠근다
//! — `looks_like_failure`의 `": error TS"` 패턴의 근거.

use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::{migrate, repo_observed};
use wimcc::ingest::{store, verification_run::extract_verification_runs};
use wimcc::live::NoopSink;

const FIXTURE: &str = "tests/fixtures/transcripts/real/verification_tsc_v01.jsonl";

async fn runs_for(session: &str) -> Vec<wimcc::ingest::verification_run::VerificationRunRecord> {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(&pool, std::path::Path::new(FIXTURE), &NoopSink)
        .await
        .unwrap();
    let evs = repo_observed::list_session(&pool, session, 100_000)
        .await
        .unwrap();
    extract_verification_runs(&evs)
}

#[tokio::test]
async fn npx_tsc_noemit_failure_is_build_check_failed_estimated() {
    let runs = runs_for("190a23db-0c03-408d-b8aa-6981ce3a7605").await;
    assert_eq!(runs.len(), 1, "exactly the tsc run: {runs:?}");
    let r = &runs[0];
    assert!(r.command.contains("npx tsc --noEmit"));
    assert_eq!(r.command_kind, "build_check");
    assert_eq!(r.detection_basis, "known_tool");
    // `| head -20`은 pager 출력-캡처 관용구 — exit 기준 유지.
    assert_eq!(r.status_basis, "exit");
    // CC는 성공/pager-종단 실행에 exit 마커를 안 남긴다 — tsc 진단 포맷
    // `): error TS2345:`가 Tier-4 실패 추정을 발화시킨다.
    assert_eq!(r.status, "failed");
    assert_eq!(r.status_provenance.as_deref(), Some("estimated"));
}

#[tokio::test]
async fn pnpm_tsc_noemit_silent_success_stays_unknown() {
    let runs = runs_for("178fae97-fcf4-4ac6-9daf-62fcb2527af7").await;
    assert_eq!(runs.len(), 1, "exactly the tsc run: {runs:?}");
    let r = &runs[0];
    assert!(r.command.contains("pnpm tsc --noEmit"));
    assert_eq!(r.command_kind, "build_check");
    // tsc는 성공 시 아무것도 출력하지 않는다(공식 동작) — transcript-only로는
    // 성공을 확인할 수 없어 unknown이 정직하다(OTLP tool_result가 해소 클래스).
    assert_eq!(r.status, "unknown");
}

#[tokio::test]
async fn pnpm_exec_tsc_build_failure_is_piped_failed_estimated() {
    let runs = runs_for("01fe9550-58cb-43ab-b8e5-90285076e34a").await;
    assert_eq!(runs.len(), 1, "exactly the tsc run: {runs:?}");
    let r = &runs[0];
    assert!(r.command.contains("pnpm exec tsc -b"));
    // --noEmit 없는 tsc는 emit하는 컴파일 — build (cargo build 동형).
    assert_eq!(r.command_kind, "build");
    // `| grep`은 pager가 아니다 — exit code가 가려진다.
    assert_eq!(r.status_basis, "piped");
    assert_eq!(r.status, "failed");
    assert_eq!(r.status_provenance.as_deref(), Some("estimated"));
}
