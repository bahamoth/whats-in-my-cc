//! Real-data anchoring: 동결된 disposition_v01.jsonl(실 세션 7라인)이
//! disposition 분류·outcome tool_use_error 단계·hook attachment 승격을 잠근다.
//!
//! fixture 구성 (모두 실 payload, 세션별):
//! - 0daafa6e: 사용자 거부 tool_result
//! - 5864d6c7: backgrounded · stale-read tool_use_error · hook_cancelled attachment
//! - 6a254a2a: hook 차단 tool_result · hook_system_message attachment
//! - ed82aee9: 병렬 호출 취소 tool_use_error

use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::{migrate, repo_observed};
use wimcc::ingest::store;
use wimcc::insight::disposition::{classify_disposition, Disposition};
use wimcc::insight::outcome::{resolve_outcome, OutcomeProvenance, OutcomeStatus};
use wimcc::live::NoopSink;
use wimcc::model::observed::{EventKind, ObservedEvent};

const FIXTURE: &str = "tests/fixtures/transcripts/real/disposition_v01.jsonl";

async fn ingested_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(&pool, std::path::Path::new(FIXTURE), &NoopSink)
        .await
        .expect("ingest");
    pool
}

fn result_content<'a>(events: &'a [ObservedEvent], tid: &str) -> &'a str {
    events
        .iter()
        .find(|e| e.kind == EventKind::ToolResult && e.tool_use_id.as_deref() == Some(tid))
        .and_then(|e| e.payload.pointer("/tool_result/content"))
        .and_then(|v| v.as_str())
        .expect("tool_result content")
}

#[tokio::test]
async fn real_payloads_classify_into_dispositions() {
    let pool = ingested_pool().await;

    let s0 = repo_observed::list_session(&pool, "0daafa6e-7a95-4c11-b76d-230208456833", 1000)
        .await
        .unwrap();
    assert_eq!(
        classify_disposition(result_content(&s0, "toolu_01UsUT7hPoE9xweaChCvxyQs")),
        Some(Disposition::UserRejected)
    );

    let s6 = repo_observed::list_session(&pool, "6a254a2a-bd02-4b85-a0c1-bd179c72b808", 1000)
        .await
        .unwrap();
    assert_eq!(
        classify_disposition(result_content(&s6, "toolu_01XbRb5UC7BwENgEfN1nmjXH")),
        Some(Disposition::PolicyDenied)
    );

    let s5 = repo_observed::list_session(&pool, "5864d6c7-a009-4bb5-9b09-e97c4f7f82fe", 1000)
        .await
        .unwrap();
    assert_eq!(
        classify_disposition(result_content(&s5, "toolu_01DZteT1xu2c1ExdPnG5TXdz")),
        Some(Disposition::Backgrounded)
    );

    let se = repo_observed::list_session(&pool, "ed82aee9-62a7-4cfe-bf9a-565379765b1e", 1000)
        .await
        .unwrap();
    assert_eq!(
        classify_disposition(result_content(&se, "toolu_017mQohn36gCWrEciv4bhkTk")),
        Some(Disposition::Cancelled)
    );
}

#[tokio::test]
async fn real_tool_use_error_is_measured_failure_but_cancelled_is_not() {
    let pool = ingested_pool().await;

    // stale-read Edit 실패: <tool_use_error> 래퍼 → Failed(Measured).
    let s5 = repo_observed::list_session(&pool, "5864d6c7-a009-4bb5-9b09-e97c4f7f82fe", 1000)
        .await
        .unwrap();
    let stale_tid = "toolu_015hqfG2AQGcRFCHkRhoYUcg";
    assert_eq!(classify_disposition(result_content(&s5, stale_tid)), None);
    let o = resolve_outcome(&s5, stale_tid);
    assert_eq!(o.status, OutcomeStatus::Failed);
    assert_eq!(o.provenance, OutcomeProvenance::Measured);

    // 병렬 호출 취소: 같은 래퍼지만 실행 실패가 아님 → Unknown 유지.
    let se = repo_observed::list_session(&pool, "ed82aee9-62a7-4cfe-bf9a-565379765b1e", 1000)
        .await
        .unwrap();
    let o = resolve_outcome(&se, "toolu_017mQohn36gCWrEciv4bhkTk");
    assert_eq!(o.status, OutcomeStatus::Unknown);
}

#[tokio::test]
async fn hook_attachments_are_promoted_to_hook_events() {
    let pool = ingested_pool().await;

    // hook_system_message: 차단 규칙 이름+안내문을 담은 attachment → hook_event 승격.
    let s6 = repo_observed::list_session(&pool, "6a254a2a-bd02-4b85-a0c1-bd179c72b808", 1000)
        .await
        .unwrap();
    let sysmsg = s6
        .iter()
        .find(|e| e.subkind.as_deref() == Some("hook_system_message"))
        .expect("hook_system_message event");
    assert_eq!(sysmsg.kind, EventKind::HookEvent);

    let s5 = repo_observed::list_session(&pool, "5864d6c7-a009-4bb5-9b09-e97c4f7f82fe", 1000)
        .await
        .unwrap();
    let cancelled = s5
        .iter()
        .find(|e| e.subkind.as_deref() == Some("hook_cancelled"))
        .expect("hook_cancelled event");
    assert_eq!(cancelled.kind, EventKind::HookEvent);
}
