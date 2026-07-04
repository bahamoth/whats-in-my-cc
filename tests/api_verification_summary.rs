//! 대시보드 검증 탭 — `GET /v1/verification/summary` (2026-07-04 전면 개편).
//!
//! 결정론 정의를 잠근다:
//! - recovered: failed run과 같은 (session, command_kind)에 더 늦은 passed 존재
//! - rhythm pct: (started_at − session.first) / (last − first) × 100, 소수 1자리
//! - coverage: 세션에 파싱 가능한 started_at의 passed run이 하나라도 있으면
//!   그 세션 hunk 전부 covered(기존 covered_diff_hunk_ids의 보수적 의미와 동일)

use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::api::{router, AppState};
use wimcc::db::migrate;

async fn pool() -> sqlx::SqlitePool {
    let p = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&p).await.unwrap();
    p
}

async fn seed_session_events(pool: &sqlx::SqlitePool, sid: &str, first: &str, last: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO ingest_run (run_id, started_at, status) VALUES ('run_vs', datetime('now'), 'done')",
    )
    .execute(pool)
    .await
    .unwrap();
    for (i, at) in [first, last].iter().enumerate() {
        let raw_id = format!("raw_{sid}_{i}");
        sqlx::query(
            "INSERT INTO raw_event (raw_event_id, ingest_run_id, source_type, source_uri, source_line_no, source_byte_offset, payload_sha256, payload, captured_at)
             VALUES (?, 'run_vs', 'claude_transcript', 'vs.jsonl', 0, 0, ?, '{}', datetime('now'))",
        )
        .bind(&raw_id)
        .bind(format!("sha_{raw_id}"))
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO observed_event (event_id, raw_event_id, schema_version, session_id, observed_at, actor, kind, payload, parser_version)
             VALUES (?, ?, 'observed_event.v1', ?, ?, 'assistant', 'assistant_message', '{}', 'test')",
        )
        .bind(format!("ev_{sid}_{i}"))
        .bind(&raw_id)
        .bind(sid)
        .bind(at)
        .execute(pool)
        .await
        .unwrap();
    }
}

#[allow(clippy::too_many_arguments)]
async fn seed_run(
    pool: &sqlx::SqlitePool,
    id: &str,
    sid: &str,
    kind: &str,
    status: &str,
    status_basis: &str,
    started_at: &str,
) {
    sqlx::query(
        "INSERT INTO verification_run (verification_run_id, session_id, source, command, command_kind, trigger_event_id, status, status_basis, started_at, raw_event_id, parser_version)
         VALUES (?, ?, 'bash', 'cmd', ?, 'ev_trigger', ?, ?, ?, 'raw_x', 'test')",
    )
    .bind(id)
    .bind(sid)
    .bind(kind)
    .bind(status)
    .bind(status_basis)
    .bind(started_at)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_hunks(pool: &sqlx::SqlitePool, sid: &str, n: usize) {
    for i in 0..n {
        sqlx::query(
            "INSERT INTO diff_hunk (diff_hunk_id, schema_version, session_id, file_path, change_type, introduced_by_event_id, patch_preview, lines_added, lines_removed)
             VALUES (?, 'diff_hunk.v1', ?, 'src/x.rs', 'modify', 'ev_x', '@@', 1, 0)",
        )
        .bind(format!("dh_{sid}_{i}"))
        .bind(sid)
        .execute(pool)
        .await
        .unwrap();
    }
}

/// A: failed→passed(test, 복구) + build passed, hunk 3 (전부 covered)
/// B: lint unknown(piped) + test failed(방치) + build_check not_executed, hunk 1 (covered 0)
async fn seeded() -> sqlx::SqlitePool {
    let p = pool().await;
    seed_session_events(
        &p,
        "sess_va",
        "2026-06-10T00:00:00+00:00",
        "2026-06-10T10:00:00+00:00",
    )
    .await;
    seed_run(
        &p,
        "vr_a1",
        "sess_va",
        "test_suite_rust",
        "failed",
        "exit",
        "2026-06-10T02:30:00+00:00",
    )
    .await;
    seed_run(
        &p,
        "vr_a2",
        "sess_va",
        "test_suite_rust",
        "passed",
        "exit",
        "2026-06-10T05:00:00+00:00",
    )
    .await;
    seed_run(
        &p,
        "vr_a3",
        "sess_va",
        "build",
        "passed",
        "exit",
        "2026-06-10T07:30:00+00:00",
    )
    .await;
    seed_hunks(&p, "sess_va", 3).await;
    seed_session_events(
        &p,
        "sess_vb",
        "2026-06-11T00:00:00+00:00",
        "2026-06-11T01:00:00+00:00",
    )
    .await;
    seed_run(
        &p,
        "vr_b1",
        "sess_vb",
        "lint",
        "unknown",
        "piped",
        "2026-06-11T00:30:00+00:00",
    )
    .await;
    seed_run(
        &p,
        "vr_b2",
        "sess_vb",
        "test_suite_js",
        "failed",
        "exit",
        "2026-06-11T00:45:00+00:00",
    )
    .await;
    seed_run(
        &p,
        "vr_b3",
        "sess_vb",
        "build_check",
        "not_executed",
        "exit",
        "2026-06-11T00:50:00+00:00",
    )
    .await;
    seed_hunks(&p, "sess_vb", 1).await;
    p
}

#[tokio::test]
async fn summary_aggregates_kinds_failures_rhythm_coverage() {
    let server = TestServer::new(router(AppState::new_for_tests(seeded().await))).unwrap();
    let r = server.get("/v1/verification/summary").await;
    r.assert_status_ok();
    let v: Value = r.json();
    let d = &v["data"];

    assert_eq!(d["total"], 6);
    assert_eq!(d["measured"], 4); // passed 2 + failed 2
    assert_eq!(d["passed"], 2);
    assert_eq!(d["failed"], 2);
    assert_eq!(d["unknown"], 1);
    assert_eq!(d["unknown_piped"], 1);
    assert_eq!(d["unknown_other"], 0);
    assert_eq!(d["not_executed"], 1);

    // kind 매핑: test_suite_* → test, build|build_check → build. 정렬: 총수 desc, kind asc.
    let kinds = d["by_kind"].as_array().unwrap();
    assert_eq!(kinds[0]["kind"], "test");
    assert_eq!(kinds[0]["passed"], 1);
    assert_eq!(kinds[0]["failed"], 2);
    assert_eq!(kinds[1]["kind"], "build");
    assert_eq!(kinds[1]["passed"], 1);
    assert_eq!(kinds[1]["not_executed"], 1);
    assert_eq!(kinds[2]["kind"], "lint");
    assert_eq!(kinds[2]["unknown"], 1);

    assert_eq!(d["failures"]["recovered"], 1); // vr_a1 → 이후 vr_a2 passed
    assert_eq!(d["failures"]["abandoned"], 1); // vr_b2

    // rhythm: run 수 동률(3:3) → session_id asc. pct = 시간 위치.
    let rhythm = d["rhythm"].as_array().unwrap();
    assert_eq!(rhythm[0]["session_id"], "sess_va");
    assert_eq!(rhythm[0]["guards"], 3);
    assert_eq!(rhythm[0]["passed"], 2);
    let pcts: Vec<f64> = rhythm[0]["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["pct"].as_f64().unwrap())
        .collect();
    assert_eq!(pcts, vec![25.0, 50.0, 75.0]);
    assert_eq!(rhythm[0]["runs"][0]["status"], "failed");

    // coverage: A는 passed run 존재 → hunk 3 전부, B는 passed 없음 → 0.
    assert_eq!(d["coverage"]["covered"], 3);
    assert_eq!(d["coverage"]["total"], 4);
    let by = d["coverage"]["by_session"].as_array().unwrap();
    assert_eq!(by[0]["session_id"], "sess_va");
    assert_eq!(by[0]["covered"], 3);
    assert_eq!(by[1]["session_id"], "sess_vb");
    assert_eq!(by[1]["covered"], 0);
}

#[tokio::test]
async fn summary_respects_from_window() {
    let server = TestServer::new(router(AppState::new_for_tests(seeded().await))).unwrap();
    let r = server
        .get("/v1/verification/summary?from=2026-06-11T00:00:00%2B00:00")
        .await;
    r.assert_status_ok();
    let v: Value = r.json();
    assert_eq!(v["data"]["total"], 3);
    assert_eq!(v["data"]["coverage"]["total"], 1);
}

#[tokio::test]
async fn summary_rejects_bad_time() {
    let server = TestServer::new(router(AppState::new_for_tests(pool().await))).unwrap();
    let r = server.get("/v1/verification/summary?from=yesterday").await;
    r.assert_status(axum::http::StatusCode::BAD_REQUEST);
}
