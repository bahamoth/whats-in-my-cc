//! B-4 (2026-07-04) — `POST /v1/export-bundles`: owner-only local export.
//!
//! read-only 원칙의 유일한 write 예외(PRD·05 §06). 계약(§09-3 사용자 결정,
//! 2026-07-04): 기본은 redacted normalized evidence만, raw payload는
//! explicit opt-in(`include_raw: true`). 번들은 로컬 파일 + sha256 —
//! 데이터가 로컬을 떠나는 경로가 아니라 owner가 반출을 선택하는 행위다.
//!
//! WIMCC_CONFIG_DIR을 설정하므로 전용 테스트 바이너리(프로세스 격리 —
//! detector_config_file.rs 관행).

use axum_test::TestServer;
use serde_json::{json, Value};
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::migrate;
use wimcc::ingest::store;

/// 이 바이너리의 모든 테스트가 공유하는 config dir — 테스트가 병렬로 돌므로
/// per-test env 설정은 경합한다(번들 파일명이 타임스탬프라 공유는 안전).
static CONFIG_DIR: once_cell::sync::Lazy<tempfile::TempDir> =
    once_cell::sync::Lazy::new(|| tempfile::tempdir().unwrap());

async fn make_server() -> (TestServer, sqlx::SqlitePool) {
    std::env::set_var("WIMCC_CONFIG_DIR", CONFIG_DIR.path());
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/transcripts/minimal_session.jsonl"),
        &wimcc::live::NoopSink,
    )
    .await
    .unwrap();
    let state = wimcc::api::AppState::new_for_tests(pool.clone());
    let server = TestServer::new(wimcc::api::router(state)).unwrap();
    (server, pool)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

#[tokio::test]
async fn export_bundle_writes_redacted_file_with_hash() {
    let (server, _pool) = make_server().await;
    let r = server
        .post("/v1/export-bundles")
        .json(&json!({"kind": "session", "id": "sess-A"}))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    let d = &body["data"];
    let path = d["bundle_path"].as_str().unwrap();
    let bytes = std::fs::read(path).expect("bundle file must exist");
    assert_eq!(
        d["sha256"].as_str().unwrap(),
        sha256_hex(&bytes),
        "sha256 must match the written file"
    );
    let bundle: Value = serde_json::from_str(std::str::from_utf8(&bytes).unwrap()).unwrap();
    // redacted normalized evidence 기본 — observed 이벤트는 있고 raw는 없다.
    assert!(bundle["events"].as_array().unwrap().len() >= 6, "{bundle}");
    assert!(bundle.get("raw_events").is_none(), "raw must be opt-in");
    assert_eq!(bundle["session"]["session_id"], "sess-A");
    assert!(bundle["session"]["metrics"]["tool_call_total"].is_i64());
    assert!(bundle["signals"].is_array());
    // meta가 반출 계약을 선언한다.
    assert_eq!(bundle["meta"]["include_raw"], false);
}

#[tokio::test]
async fn export_bundle_includes_raw_only_on_opt_in() {
    let (server, _pool) = make_server().await;
    let r = server
        .post("/v1/export-bundles")
        .json(&json!({"kind": "session", "id": "sess-A", "include_raw": true}))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    let path = body["data"]["bundle_path"].as_str().unwrap();
    let bundle: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    // minimal fixture는 raw 5라인 → observed 6이벤트(한 라인이 text+tool_call).
    assert!(
        bundle["raw_events"].as_array().unwrap().len() >= 5,
        "opt-in이면 raw record가 실린다: {bundle}"
    );
    assert_eq!(bundle["meta"]["include_raw"], true);
}

#[tokio::test]
async fn export_bundle_unknown_session_is_404() {
    let (server, _pool) = make_server().await;
    let r = server
        .post("/v1/export-bundles")
        .json(&json!({"kind": "session", "id": "nope"}))
        .await;
    r.assert_status_not_found();
}

#[tokio::test]
async fn export_bundle_rejects_unknown_kind() {
    let (server, _pool) = make_server().await;
    let r = server
        .post("/v1/export-bundles")
        .json(&json!({"kind": "signal", "id": "x"}))
        .await;
    r.assert_status_bad_request();
}

#[tokio::test]
async fn export_bundle_records_audit_row() {
    let (server, pool) = make_server().await;
    server
        .post("/v1/export-bundles")
        .json(&json!({"kind": "session", "id": "sess-A"}))
        .await
        .assert_status_ok();
    let rows = wimcc::db::repo_audit::list_recent(&pool, 10).await.unwrap();
    assert!(
        rows.iter().any(|r| r.event == "export_bundle_created"),
        "export는 audit에 남는다"
    );
}
