//! DetectorConfig 파일 로딩 wiring (2026-07-03).
//!
//! `pipeline.rs`가 `DetectorConfig::default()` 하드코딩("Plan 4에서 파일 로드로
//! 교체")이라 detector 임계값이 코드 고정 — 튜닝 불가하던 격차를 닫는다.
//! 파일 위치는 token과 같은 컨벤션: `$WIMCC_CONFIG_DIR/detectors.toml`
//! (기본 `~/.config/wimcc/detectors.toml`). 파일/섹션/키 결측 → 코드 기본값.
//! 03 spec: rule_pack은 코드 기본이면 null, "TOML rule pack 로드 시 그 id" —
//! 최상위 `id` 키가 pack id이며 로드된 패스의 모든 signal provenance에 찍힌다.
//!
//! 주의: `WIMCC_CONFIG_DIR`을 설정하므로 전용 테스트 바이너리로 프로세스
//! 격리하고(각 tests/*.rs = 별도 프로세스), 세 국면을 한 test fn에서 순차
//! 실행해 env 경합을 배제한다.

use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::{migrate, repo_signal};
use wimcc::ingest::store;

/// 같은 파일을 같은 범위로 2회 Read하는 합성 transcript — 코드 기본값
/// (min_reads=2)에서 re_read가 발화한다. payload 형태는 mapping.rs가 잠근
/// 실 구조(tool_use block → input.file_path)를 따른다.
fn re_read_transcript() -> String {
    let rec = |line: &str| format!("{line}\n");
    let mut s = String::new();
    s += &rec(
        r#"{"type":"user","uuid":"u1","parentUuid":null,"sessionId":"sess-cfg","timestamp":"2026-07-03T01:00:00Z","cwd":"/tmp","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"promptId":"p1","message":{"role":"user","content":"look at the file"}}"#,
    );
    s += &rec(
        r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","sessionId":"sess-cfg","timestamp":"2026-07-03T01:00:01Z","cwd":"/tmp","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"requestId":"req_1","message":{"id":"msg_1","model":"claude-opus-4-7","type":"message","role":"assistant","stop_reason":"tool_use","content":[{"type":"tool_use","id":"toolu_r1","name":"Read","input":{"file_path":"/tmp/x.rs"}}]}}"#,
    );
    s += &rec(
        r#"{"type":"user","uuid":"u2","parentUuid":"a1","sessionId":"sess-cfg","timestamp":"2026-07-03T01:00:02Z","cwd":"/tmp","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"promptId":"p1","sourceToolAssistantUUID":"a1","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_r1","content":"fn main(){}","is_error":false}]}}"#,
    );
    s += &rec(
        r#"{"type":"assistant","uuid":"a2","parentUuid":"u2","sessionId":"sess-cfg","timestamp":"2026-07-03T01:00:03Z","cwd":"/tmp","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"requestId":"req_2","message":{"id":"msg_2","model":"claude-opus-4-7","type":"message","role":"assistant","stop_reason":"tool_use","content":[{"type":"tool_use","id":"toolu_r2","name":"Read","input":{"file_path":"/tmp/x.rs"}}]}}"#,
    );
    s += &rec(
        r#"{"type":"user","uuid":"u3","parentUuid":"a2","sessionId":"sess-cfg","timestamp":"2026-07-03T01:00:04Z","cwd":"/tmp","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"promptId":"p1","sourceToolAssistantUUID":"a2","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_r2","content":"fn main(){}","is_error":false}]}}"#,
    );
    s
}

async fn ingest_fresh(transcript_path: &std::path::Path) -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(&pool, transcript_path, &wimcc::live::NoopSink)
        .await
        .unwrap();
    pool
}

async fn re_read_signals(pool: &sqlx::SqlitePool) -> Vec<repo_signal::SignalRow> {
    repo_signal::list_by_session(pool, "sess-cfg")
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.detector == "re_read")
        .collect()
}

#[tokio::test]
async fn detector_config_file_drives_pipeline() {
    let config_dir = tempfile::tempdir().unwrap();
    // Hermetic: 개발 머신의 실제 ~/.config/wimcc/detectors.toml을 읽지 않도록
    // 시작부터 격리 디렉토리를 가리킨다.
    std::env::set_var("WIMCC_CONFIG_DIR", config_dir.path());

    let transcript = config_dir.path().join("re_read_session.jsonl");
    std::fs::write(&transcript, re_read_transcript()).unwrap();

    // Phase 1 — 파일 없음 → 코드 기본값(min_reads=2)으로 re_read 발화.
    let pool = ingest_fresh(&transcript).await;
    let fired = re_read_signals(&pool).await;
    assert_eq!(fired.len(), 1, "baseline: re_read must fire with defaults");
    let prov: serde_json::Value = serde_json::from_str(&fired[0].provenance).unwrap();
    assert!(
        prov["rule_pack"].is_null(),
        "no config file → rule_pack null (code-default), got {prov}"
    );

    // Phase 2 — detectors.toml이 min_reads를 99로 올림 → 발화하지 않아야 한다.
    let cfg_path = config_dir.path().join("detectors.toml");
    std::fs::write(
        &cfg_path,
        "id = \"test-pack@2026-07\"\n[detector.re_read]\nmin_reads = 99\n",
    )
    .unwrap();
    let pool = ingest_fresh(&transcript).await;
    assert!(
        re_read_signals(&pool).await.is_empty(),
        "min_reads=99 from file must suppress re_read"
    );

    // Phase 3 — id만 있는 pack (파라미터 결측 → 기본값): re_read는 발화하고
    // provenance.rule_pack에 pack id가 찍힌다 (03 spec).
    std::fs::write(&cfg_path, "id = \"test-pack@2026-07\"\n").unwrap();
    let pool = ingest_fresh(&transcript).await;
    let fired = re_read_signals(&pool).await;
    assert_eq!(fired.len(), 1, "id-only pack keeps code defaults");
    let prov: serde_json::Value = serde_json::from_str(&fired[0].provenance).unwrap();
    assert_eq!(
        prov["rule_pack"], "test-pack@2026-07",
        "loaded pack id must be stamped into provenance.rule_pack"
    );
}
