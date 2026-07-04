//! §1.2 (docs/specs/2026-07-04-session-detail-improvements.md) — events 4축 필터.
//! 잠그는 계약: 축별 정확성 / 축 AND·CSV OR / 커서 페이징 결합(경계 누락·중복
//! 없음) / matched_count / around×필터 400 / 미지 값 400.

use axum_test::TestServer;
use chrono::{DateTime, TimeZone, Utc};
use serde_json::json;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

use wimcc::db::{migrate, repo_observed, repo_raw, repo_runs, repo_signal, repo_verification_run};
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

const SESS: &str = "sess-filter";

/// 시드(모두 초 간격, i = 0..):
///  - user_message ×8: 4건 human("deploy i" 2건 + "chat i" 2건), 2건 command
///    ("<command-name>/model</command-name>"), 2건 notification("<task-notification>…")
///  - assistant_message ×4: model "claude-fable-5" 2건 / "claude-haiku-4-5-20251001" 2건
///  - tool_call ×6: Bash 3, Edit 3 (tool_name 컬럼+payload 동시 설정)
///  - tool_result ×6: is_error true 2 / false 4, content "thread panicked" 1건 포함
///  - metric_sample ×6
///  - signal 1행: evidence_refs = 첫 tool_call·둘째 tool_result의 event_id
///  - verification_run 2행: trigger=마지막 Bash tool_call, status failed →
///    5분 뒤 passed (최신 승리 계약은 repo 단위테스트가 잠금 — 여기선 passed로 조회)
async fn seed_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let run_id = repo_runs::start(&pool).await.unwrap();
    let base: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 7, 5, 0, 0, 0).unwrap();

    let specs: Vec<(EventKind, serde_json::Value, Option<&str>)> = vec![
        // (kind, payload, tool_name 컬럼)
        (
            EventKind::UserMessage,
            json!({"content": "deploy the fix"}),
            None,
        ),
        (
            EventKind::UserMessage,
            json!({"content": "deploy again"}),
            None,
        ),
        (EventKind::UserMessage, json!({"content": "chat one"}), None),
        (EventKind::UserMessage, json!({"content": "chat two"}), None),
        (
            EventKind::UserMessage,
            json!({"content": "<command-name>/model</command-name>"}),
            None,
        ),
        (
            EventKind::UserMessage,
            json!({"content": "<command-name>/help</command-name>"}),
            None,
        ),
        (
            EventKind::UserMessage,
            json!({"content": "<task-notification>done A</task-notification>"}),
            None,
        ),
        (
            EventKind::UserMessage,
            json!({"content": "<task-notification>done B</task-notification>"}),
            None,
        ),
        (
            EventKind::AssistantMessage,
            json!({"text": "working", "model": "claude-fable-5"}),
            None,
        ),
        (
            EventKind::AssistantMessage,
            json!({"text": "done", "model": "claude-fable-5"}),
            None,
        ),
        (
            EventKind::AssistantMessage,
            json!({"text": "hm", "model": "claude-haiku-4-5-20251001"}),
            None,
        ),
        (
            EventKind::AssistantMessage,
            json!({"text": "ok", "model": "claude-haiku-4-5-20251001"}),
            None,
        ),
        (
            EventKind::ToolCall,
            json!({"tool_name": "Bash", "input": {"command": "cargo test"}}),
            Some("Bash"),
        ),
        (
            EventKind::ToolCall,
            json!({"tool_name": "Bash", "input": {"command": "ls"}}),
            Some("Bash"),
        ),
        (
            EventKind::ToolCall,
            json!({"tool_name": "Bash", "input": {"command": "cargo build"}}),
            Some("Bash"),
        ),
        (
            EventKind::ToolCall,
            json!({"tool_name": "Edit", "input": {"file_path": "a.rs"}}),
            Some("Edit"),
        ),
        (
            EventKind::ToolCall,
            json!({"tool_name": "Edit", "input": {"file_path": "b.rs"}}),
            Some("Edit"),
        ),
        (
            EventKind::ToolCall,
            json!({"tool_name": "Edit", "input": {"file_path": "c.rs"}}),
            Some("Edit"),
        ),
        (
            EventKind::ToolResult,
            json!({"tool_result": {"is_error": true, "content": "thread panicked"}}),
            None,
        ),
        (
            EventKind::ToolResult,
            json!({"tool_result": {"is_error": true, "content": "exit 1"}}),
            None,
        ),
        (
            EventKind::ToolResult,
            json!({"tool_result": {"is_error": false, "content": "ok"}}),
            None,
        ),
        (
            EventKind::ToolResult,
            json!({"tool_result": {"is_error": false, "content": "ok"}}),
            None,
        ),
        (
            EventKind::ToolResult,
            json!({"tool_result": {"is_error": false, "content": "ok"}}),
            None,
        ),
        (
            EventKind::ToolResult,
            json!({"tool_result": {"is_error": false, "content": "ok"}}),
            None,
        ),
    ];

    let total = specs.len() + 6; // + metric_sample x6
    let mut event_ids: Vec<String> = Vec::with_capacity(total);
    for i in 0..total {
        let event_id = format!("01K{i:023}");
        let raw_id = format!("raw_{i:06}");
        event_ids.push(event_id.clone());

        repo_raw::insert_dedup(
            &pool,
            &repo_raw::NewRaw {
                raw_event_id: raw_id.clone(),
                ingest_run_id: run_id.clone(),
                source_type: "test".into(),
                source_uri: format!("test://{i}"),
                source_line_no: i as i64,
                source_byte_offset: 0,
                payload_sha256: format!("sha_{i:06}"),
                payload: b"{}".to_vec(),
                parse_error: None,
                captured_at: chrono::Utc::now(),
                redaction_state: "not_applicable".into(),
                redaction_manifest: None,
            },
        )
        .await
        .unwrap();

        let (kind, payload, tool_name) = if i < specs.len() {
            let (k, p, t) = &specs[i];
            (*k, p.clone(), t.map(str::to_string))
        } else {
            (EventKind::MetricSample, json!({}), None)
        };

        let ev = ObservedEvent {
            event_id,
            raw_event_id: raw_id,
            schema_version: "0.5.0".into(),
            session_id: SESS.into(),
            observed_at: base + chrono::Duration::seconds(i as i64),
            actor: Actor::User,
            kind,
            tool_name,
            payload,
            parser_version: "test".into(),
            ..Default::default()
        };
        repo_observed::insert(&pool, &ev).await.unwrap();
    }

    // signal: evidence = first Bash tool_call (idx 12) + second tool_result (idx 19).
    repo_signal::insert(
        &pool,
        &repo_signal::SignalRow {
            signal_id: "sig_filter_1".into(),
            schema_version: "signal.v1".into(),
            session_id: SESS.into(),
            detector: "test_detector".into(),
            subkind: None,
            summary: "test signal".into(),
            evidence_refs: format!(
                r#"["{}",{{"event_id":"{}"}}]"#,
                event_ids[12], event_ids[19]
            ),
            facts: "{}".into(),
            provenance: "{}".into(),
            created_at: base.to_rfc3339(),
        },
    )
    .await
    .unwrap();

    // verification_run: trigger = last Bash tool_call (idx 14) — failed then passed
    // (latest started_at wins; repo unit test locks the "latest wins" contract).
    let trigger = event_ids[14].clone();
    repo_verification_run::insert(
        &pool,
        &repo_verification_run::VerificationRunRow {
            verification_run_id: "vr_filter_1".into(),
            schema_version: "verification_run@v1".into(),
            session_id: SESS.into(),
            source: "bash".into(),
            command: "cargo build".into(),
            command_kind: "test_suite_rust".into(),
            trigger_event_id: trigger.clone(),
            status: "failed".into(),
            started_at: (base + chrono::Duration::seconds(100)).to_rfc3339(),
            raw_event_id: "raw_vr_1".into(),
            parser_version: "test".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    repo_verification_run::insert(
        &pool,
        &repo_verification_run::VerificationRunRow {
            verification_run_id: "vr_filter_2".into(),
            schema_version: "verification_run@v1".into(),
            session_id: SESS.into(),
            source: "bash".into(),
            command: "cargo build".into(),
            command_kind: "test_suite_rust".into(),
            trigger_event_id: trigger,
            status: "passed".into(),
            started_at: (base + chrono::Duration::seconds(400)).to_rfc3339(),
            raw_event_id: "raw_vr_2".into(),
            parser_version: "test".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    pool
}

async fn setup() -> TestServer {
    let pool = seed_pool().await;
    let app = wimcc::api::router(wimcc::api::AppState::new_for_tests(pool));
    TestServer::new(app).unwrap()
}

async fn get(server: &TestServer, qs: &str) -> serde_json::Value {
    server
        .get(&format!("/v1/sessions/{SESS}/events{qs}"))
        .await
        .json::<serde_json::Value>()
}

#[tokio::test]
async fn filter_axes_and_matched_count() {
    let server = setup().await;
    // origin=human → human user_message 4건만
    let v = get(&server, "?origin=human").await;
    assert_eq!(v["data"]["events"].as_array().unwrap().len(), 4);
    assert_eq!(v["data"]["matched_count"], serde_json::json!(4));
    // origin CSV OR
    let v = get(&server, "?origin=command,notification").await;
    assert_eq!(v["data"]["matched_count"], serde_json::json!(4));
    // error=true
    let v = get(&server, "?error=true").await;
    assert_eq!(v["data"]["matched_count"], serde_json::json!(2));
    // signal=true → evidence 2건
    let v = get(&server, "?signal=true").await;
    assert_eq!(v["data"]["matched_count"], serde_json::json!(2));
    // verification=passed → trigger tool_call 1건
    let v = get(&server, "?verification=passed").await;
    assert_eq!(v["data"]["matched_count"], serde_json::json!(1));
    let v = get(&server, "?verification=failed").await;
    assert_eq!(v["data"]["matched_count"], serde_json::json!(0));
    // tool·model·q·role
    assert_eq!(
        get(&server, "?tool=Bash").await["data"]["matched_count"],
        serde_json::json!(3)
    );
    assert_eq!(
        get(&server, "?model=claude-fable-5").await["data"]["matched_count"],
        serde_json::json!(2)
    );
    assert_eq!(
        get(&server, "?q=PANICKED").await["data"]["matched_count"],
        serde_json::json!(1)
    );
    assert_eq!(
        get(&server, "?role=assistant").await["data"]["matched_count"],
        serde_json::json!(4)
    );
    // AND 조합: q=deploy && origin=human → 2건
    assert_eq!(
        get(&server, "?q=deploy&origin=human").await["data"]["matched_count"],
        serde_json::json!(2)
    );
    // tool CSV OR — 두 번째 토큰(Edit)도 실제로 매칭돼야 한다(SQL 푸시다운
    // 경로 end-to-end; Task 2 리뷰 커버리지 지시).
    let v = get(&server, "?tool=Bash,Edit").await;
    assert_eq!(v["data"]["matched_count"], serde_json::json!(6));
    let events = v["data"]["events"].as_array().unwrap();
    assert!(
        events.iter().any(|e| e["tool_name"] == "Edit"),
        "CSV second token (Edit) must match end-to-end"
    );
    // kind 축이 신규 필터 경로를 타도 정확한 이벤트 + matched_count를 낸다.
    let v = get(&server, "?kind=tool_result").await;
    assert_eq!(v["data"]["matched_count"], serde_json::json!(6));
    let events = v["data"]["events"].as_array().unwrap();
    assert_eq!(events.len(), 6);
    assert!(events.iter().all(|e| e["kind"] == "tool_result"));
    // 필터 없으면 matched_count 자체가 없다
    let v = get(&server, "").await;
    assert!(v["data"].get("matched_count").is_none());
}

#[tokio::test]
async fn filtered_pagination_no_gap_no_dup_and_tip() {
    let server = setup().await;
    // user_message 8건을 limit=3으로 뒤로 페이징: 3+3+2, 합집합 8, 교집합 0
    let p1 = get(&server, "?role=user&limit=3").await;
    let e1 = p1["data"]["events"].as_array().unwrap().clone();
    assert_eq!(e1.len(), 3);
    assert!(p1["data"]["next_cursor"].is_null(), "무커서 필터 창은 tip");
    let prev = p1["data"]["prev_cursor"].as_str().unwrap().to_string();
    let p2 = get(
        &server,
        &format!("?role=user&limit=3&before={}", urlencoding::encode(&prev)),
    )
    .await;
    let e2 = p2["data"]["events"].as_array().unwrap().clone();
    assert_eq!(e2.len(), 3);
    let prev2 = p2["data"]["prev_cursor"].as_str().unwrap().to_string();
    let p3 = get(
        &server,
        &format!("?role=user&limit=3&before={}", urlencoding::encode(&prev2)),
    )
    .await;
    let e3 = p3["data"]["events"].as_array().unwrap().clone();
    assert_eq!(e3.len(), 2);
    let mut ids: Vec<String> = [&e1, &e2, &e3]
        .into_iter()
        .flatten()
        .map(|e| e["event_id"].as_str().unwrap().to_string())
        .collect();
    ids.sort();
    let n = ids.len();
    ids.dedup();
    assert_eq!(n, 8, "no dup, no gap");
    assert_eq!(ids.len(), 8, "no dup, no gap");
    // after 전진이 limit 미만 → tip(null)
    let tail = e1.last().unwrap();
    let after = format!(
        "{}|{}",
        tail["observed_at"].as_str().unwrap(),
        tail["event_id"].as_str().unwrap()
    );
    let p4 = get(
        &server,
        &format!("?role=user&limit=5&after={}", urlencoding::encode(&after)),
    )
    .await;
    assert!(p4["data"]["next_cursor"].is_null());
}

#[tokio::test]
async fn filter_param_errors() {
    let server = setup().await;
    // around×필터 → 400 (기존 kind 규칙과 동일 문구 계열)
    let r = server
        .get(&format!("/v1/sessions/{SESS}/events?origin=human&around=X"))
        .await;
    r.assert_status(axum::http::StatusCode::BAD_REQUEST);
    // 미지 origin/role/verification 값 → 400
    for qs in [
        "?origin=alien",
        "?role=bot",
        "?verification=flaky",
        "?error=yes",
    ] {
        let r = server.get(&format!("/v1/sessions/{SESS}/events{qs}")).await;
        r.assert_status(axum::http::StatusCode::BAD_REQUEST);
    }
}
