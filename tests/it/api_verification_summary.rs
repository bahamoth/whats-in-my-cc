//! 대시보드 검증 탭 — `GET /v1/verification/summary` (2026-07-04 전면 개편).
//!
//! 결정론 정의를 잠근다:
//! - recovered: failed run과 같은 (session, command_kind)에 더 늦은 passed 존재
//! - rhythm pct: (started_at − session.first) / (last − first) × 100, 소수 1자리
//! - coverage(정밀, 2026-07-04 2차): hunk는 도입 이벤트의 observed_at 이후에
//!   passed run이 존재할 때만 covered. 도입 시점을 알 수 없는 hunk는 커버로
//!   치지 않는다(검증 안 된 변경을 숨기지 않는 방향의 보수).

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

/// hunk를 도입 이벤트(관측 시각 포함)와 함께 시드한다 — 정밀 커버리지의
/// 시간 선행 조인이 실제 observed_event를 읽는 것을 검증하기 위해.
async fn seed_hunk_at(pool: &sqlx::SqlitePool, sid: &str, idx: usize, observed_at: &str) {
    let ev_id = format!("ev_hunk_{sid}_{idx}");
    sqlx::query(
        "INSERT INTO raw_event (raw_event_id, ingest_run_id, source_type, source_uri, source_line_no, source_byte_offset, payload_sha256, payload, captured_at)
         VALUES (?, 'run_vs', 'claude_transcript', 'vs.jsonl', 0, 0, ?, '{}', datetime('now'))",
    )
    .bind(format!("raw_hunk_{sid}_{idx}"))
    .bind(format!("sha_hunk_{sid}_{idx}"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO observed_event (event_id, raw_event_id, schema_version, session_id, observed_at, actor, kind, payload, parser_version)
         VALUES (?, ?, 'observed_event.v1', ?, ?, 'assistant', 'tool_result', '{}', 'test')",
    )
    .bind(&ev_id)
    .bind(format!("raw_hunk_{sid}_{idx}"))
    .bind(sid)
    .bind(observed_at)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO diff_hunk (diff_hunk_id, schema_version, session_id, file_path, change_type, introduced_by_event_id, patch_preview, lines_added, lines_removed)
         VALUES (?, 'diff_hunk.v1', ?, 'src/x.rs', 'modify', ?, '@@', 1, 0)",
    )
    .bind(format!("dh_{sid}_{idx}"))
    .bind(sid)
    .bind(&ev_id)
    .execute(pool)
    .await
    .unwrap();
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
    // A: 마지막 passed run은 07:30. 01:00·05:30 도입 hunk는 covered,
    // 09:00 도입 hunk는 이후 passed가 없어 uncovered.
    seed_hunk_at(&p, "sess_va", 0, "2026-06-10T01:00:00+00:00").await;
    seed_hunk_at(&p, "sess_va", 1, "2026-06-10T05:30:00+00:00").await;
    seed_hunk_at(&p, "sess_va", 2, "2026-06-10T09:00:00+00:00").await;
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
    // B: passed run이 없으므로 도입 시점과 무관하게 uncovered.
    seed_hunk_at(&p, "sess_vb", 0, "2026-06-11T00:10:00+00:00").await;
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

    // coverage(정밀): A는 도입 01:00·05:30 hunk만 이후 passed(05:00은 아님 —
    // 05:30 hunk를 커버하는 것은 07:30 build passed) → 2/3. B는 passed 없음 → 0/1.
    assert_eq!(d["coverage"]["covered"], 2);
    assert_eq!(d["coverage"]["total"], 4);
    let by = d["coverage"]["by_session"].as_array().unwrap();
    assert_eq!(by[0]["session_id"], "sess_va");
    assert_eq!(by[0]["covered"], 2);
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

#[tokio::test]
async fn summary_session_scope_aggregates_single_session() {
    let server = TestServer::new(router(AppState::new_for_tests(seeded().await))).unwrap();
    let r = server
        .get("/v1/verification/summary?session_id=sess_va")
        .await;
    r.assert_status_ok();
    let v: Value = r.json();
    let d = &v["data"];
    // sess_va만: failed→passed(test) + build passed = 3 runs, hunk 2/3 covered.
    assert_eq!(d["total"], 3);
    assert_eq!(d["passed"], 2);
    assert_eq!(d["failed"], 1);
    assert_eq!(d["failures"]["recovered"], 1);
    assert_eq!(d["failures"]["abandoned"], 0);
    let rhythm = d["rhythm"].as_array().unwrap();
    assert_eq!(rhythm.len(), 1);
    assert_eq!(rhythm[0]["session_id"], "sess_va");
    assert_eq!(d["coverage"]["covered"], 2);
    assert_eq!(d["coverage"]["total"], 3);
}

#[tokio::test]
async fn summary_session_scope_rejects_window_params() {
    // session_id×project/from/to 결합은 계약상 미지원(400) — kind×around와 같은 스타일.
    let server = TestServer::new(router(AppState::new_for_tests(pool().await))).unwrap();
    for q in [
        "/v1/verification/summary?session_id=s&project=p",
        "/v1/verification/summary?session_id=s&from=2026-06-10T00:00:00%2B00:00",
        "/v1/verification/summary?session_id=s&to=2026-06-10T00:00:00%2B00:00",
    ] {
        let r = server.get(q).await;
        r.assert_status(axum::http::StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn summary_session_scope_unknown_session_is_empty() {
    let server = TestServer::new(router(AppState::new_for_tests(seeded().await))).unwrap();
    let r = server.get("/v1/verification/summary?session_id=nope").await;
    r.assert_status_ok();
    let v: Value = r.json();
    assert_eq!(v["data"]["total"], 0);
    assert_eq!(v["data"]["coverage"]["total"], 0);
    assert_eq!(v["data"]["rhythm"].as_array().unwrap().len(), 0);
}
