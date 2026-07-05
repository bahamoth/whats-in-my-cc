//! GET /v1/sessions/:id/usage returns the token-usage aggregate envelope.
use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::api::{router, AppState};
use wimcc::db::repo_usage_facet::UsageFacetRow;
use wimcc::db::{migrate, repo_observed, repo_raw, repo_runs, repo_usage_facet};
use wimcc::ingest::store;
use wimcc::live::NoopSink;
use wimcc::model::observed::{EventKind, ObservedEvent};

async fn empty_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn usage_endpoint_returns_aggregate() {
    let pool = empty_pool().await;
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/transcripts/real/verification_v01.jsonl"),
        &NoopSink,
    )
    .await
    .unwrap();

    let state = AppState::new_for_tests(pool);
    let server = TestServer::new(router(state)).unwrap();
    let r = server
        .get("/v1/sessions/aac68973-729e-4014-a02b-28a556f5ff29/usage")
        .await;
    r.assert_status_ok();
    let body = r.json::<Value>();
    let data = &body["data"];
    assert!(data["assistant_events"].as_i64().unwrap() > 0);
    assert!(data["cache_read_input_tokens"].as_i64().unwrap() > 0);
    assert!(data["billed_tokens"].as_i64().unwrap() > 0);
    // cache_hit_ratio is removed (F1); consumers compute from token components.
    assert!(data.get("cache_hit_ratio").is_none());
}

#[tokio::test]
async fn usage_endpoint_returns_public_pricing_estimate() {
    let pool = empty_pool().await;
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/transcripts/real/verification_v01.jsonl"),
        &NoopSink,
    )
    .await
    .unwrap();

    let state = AppState::new_for_tests(pool);
    let server = TestServer::new(router(state)).unwrap();
    let r = server
        .get("/v1/sessions/aac68973-729e-4014-a02b-28a556f5ff29/usage")
        .await;
    r.assert_status_ok();
    let body = r.json::<Value>();
    let data = &body["data"];

    // Fixture is all claude-opus-4-7 (priced) with non-zero tokens → positive.
    assert!(
        data["estimated_cost_usd"].as_f64().unwrap() > 0.0,
        "real fixture should yield a positive public-pricing estimate"
    );
    // Never presented as actual billing.
    assert_eq!(
        data["cost_basis"].as_str().unwrap(),
        "estimate_public_pricing"
    );
    // Pricing is a periodic-refresh public-rate estimate; the version IS the
    // update date (YYYY-MM-DD) — no arbitrary v-numbering — so staleness is
    // visible in the API/UI.
    // 버전 리터럴 고정 대신: (a) 형식 pricing_estimate@YYYY-MM-DD, (b) 엔드포인트가
    // 소스 상수(pricing.json의 version)를 그대로 노출 — 갱신 시 수기 수정 불필요.
    let version = data["pricing_version"].as_str().unwrap();
    assert_eq!(version, wimcc::insight::pricing::pricing_version());
    let date = version
        .strip_prefix("pricing_estimate@")
        .expect("pricing_version must start with pricing_estimate@");
    assert!(
        chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok(),
        "pricing_version date must be YYYY-MM-DD, got {version}"
    );
    // claude-opus-4-7 is in the table → nothing unpriced for this fixture.
    assert!(data["models_without_pricing"]
        .as_array()
        .unwrap()
        .is_empty());

    // Per-model detail carries the token split + per-model cost.
    let by_model = data["by_model"].as_array().unwrap();
    assert!(!by_model.is_empty());
    let m0 = &by_model[0];
    assert_eq!(m0["model"].as_str().unwrap(), "claude-opus-4-7");
    assert!(m0["priced"].as_bool().unwrap());
    assert!(m0["cache_read_input_tokens"].as_i64().unwrap() > 0);
    assert!(m0["estimated_cost_usd"].as_f64().unwrap() > 0.0);

    // §2.2 — per-model 적용 단가 노출. fixture 모델 claude-opus-4-7 = 5/6.25/0.5/25.
    let rates = &m0["rates"];
    assert!(
        (rates["input_per_mtok"].as_f64().unwrap() - 5.0).abs() < 1e-9,
        "opus-4-7 input rate"
    );
    assert!((rates["cache_creation_per_mtok"].as_f64().unwrap() - 6.25).abs() < 1e-9);
    assert!((rates["cache_read_per_mtok"].as_f64().unwrap() - 0.5).abs() < 1e-9);
    assert!((rates["output_per_mtok"].as_f64().unwrap() - 25.0).abs() < 1e-9);
}

/// F1 — `turns` → `assistant_events` 개명, `user_turns` 추가, `cache_hit_ratio` 삭제.
/// usage_facet 3행(= assistant_events 3) + user_message 이벤트 2종 turn_id(distinct 2)
/// → `assistant_events == 3`, `user_turns == 2`, `turns` 및 `cache_hit_ratio` 부재.
#[tokio::test]
async fn usage_reports_assistant_events_and_user_turns_separately() {
    let pool = empty_pool().await;

    // FK 체인: ingest_run → raw_event → observed_event.
    let run_id = repo_runs::start(&pool).await.unwrap();

    // observed_event를 위한 raw_event 시드 헬퍼.
    let seed_raw = |raw_id: &str| repo_raw::NewRaw {
        raw_event_id: raw_id.to_string(),
        ingest_run_id: run_id.clone(),
        source_type: "claude_transcript".into(),
        source_uri: "/tmp/f1_test.jsonl".into(),
        source_line_no: 0,
        source_byte_offset: 0,
        payload_sha256: format!("sha_{raw_id}"),
        payload: b"{}".to_vec(),
        parse_error: None,
        captured_at: chrono::Utc::now(),
        redaction_state: "not_applicable".into(),
        redaction_manifest: None,
    };

    // user_message 이벤트 3개: turn_id "t1", "t1", "t2" → distinct 2.
    for (event_id, turn_id) in [("ue_f1_1", "t1"), ("ue_f1_2", "t1"), ("ue_f1_3", "t2")] {
        let raw_id = format!("raw_{event_id}");
        repo_raw::insert_dedup(&pool, &seed_raw(&raw_id))
            .await
            .unwrap();
        let e = ObservedEvent {
            event_id: event_id.into(),
            raw_event_id: raw_id,
            schema_version: "observed_event.v1".into(),
            session_id: "sess_f1".into(),
            observed_at: chrono::Utc::now(),
            kind: EventKind::UserMessage,
            turn_id: Some(turn_id.into()),
            parser_version: "test@v0".into(),
            ..Default::default()
        };
        repo_observed::insert(&pool, &e).await.unwrap();
    }

    // usage_facet 3행 — 모두 같은 세션 "sess_f1".
    // usage_facet.raw_event_id는 별도 FK가 없으므로 직접 삽입 가능.
    let make_uf = |raw_event_id: &str| UsageFacetRow {
        raw_event_id: raw_event_id.into(),
        schema_version: "usage_facet.v1".into(),
        session_id: "sess_f1".into(),
        model: Some("claude-opus-4-8".into()),
        input_tokens: 10,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 0,
        output_tokens: 5,
        observed_at: "2026-06-08T10:00:00Z".into(),
        parser_version: "usage_facet@v1".into(),
    };
    repo_usage_facet::insert(&pool, &make_uf("uf_f1_1"))
        .await
        .unwrap();
    repo_usage_facet::insert(&pool, &make_uf("uf_f1_2"))
        .await
        .unwrap();
    repo_usage_facet::insert(&pool, &make_uf("uf_f1_3"))
        .await
        .unwrap();

    let state = AppState::new_for_tests(pool);
    let server = TestServer::new(router(state)).unwrap();
    let r = server.get("/v1/sessions/sess_f1/usage").await;
    r.assert_status_ok();
    let body = r.json::<Value>();
    let data = &body["data"];

    assert_eq!(data["assistant_events"].as_i64().unwrap(), 3);
    assert_eq!(data["user_turns"].as_i64().unwrap(), 2);
    assert!(data.get("turns").is_none(), "거짓 'turns' 라벨 제거됨");
    assert!(
        data.get("cache_hit_ratio").is_none(),
        "window-고정 rate 제거됨"
    );
}

/// §2.2 — 가격표에 없는 모델은 `rates: null` + `priced: false`.
#[tokio::test]
async fn usage_rates_null_for_unpriced_model() {
    let pool = empty_pool().await;
    let run_id = repo_runs::start(&pool).await.unwrap();
    repo_raw::insert_dedup(
        &pool,
        &repo_raw::NewRaw {
            raw_event_id: "raw_unpriced_1".into(),
            ingest_run_id: run_id.clone(),
            source_type: "claude_transcript".into(),
            source_uri: "/tmp/unpriced_test.jsonl".into(),
            source_line_no: 0,
            source_byte_offset: 0,
            payload_sha256: "sha_raw_unpriced_1".into(),
            payload: b"{}".to_vec(),
            parse_error: None,
            captured_at: chrono::Utc::now(),
            redaction_state: "not_applicable".into(),
            redaction_manifest: None,
        },
    )
    .await
    .unwrap();
    repo_usage_facet::insert(
        &pool,
        &UsageFacetRow {
            raw_event_id: "raw_unpriced_1".into(),
            schema_version: "usage_facet.v1".into(),
            session_id: "sess_unpriced".into(),
            model: Some("some-future-model-x".into()),
            input_tokens: 10,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
            output_tokens: 5,
            observed_at: "2026-07-04T10:00:00Z".into(),
            parser_version: "usage_facet@v1".into(),
        },
    )
    .await
    .unwrap();

    let state = AppState::new_for_tests(pool);
    let server = TestServer::new(router(state)).unwrap();
    let r = server.get("/v1/sessions/sess_unpriced/usage").await;
    r.assert_status_ok();
    let body = r.json::<Value>();
    let m0 = &body["data"]["by_model"][0];
    assert_eq!(m0["model"].as_str().unwrap(), "some-future-model-x");
    assert!(!m0["priced"].as_bool().unwrap());
    assert!(
        m0["rates"].is_null(),
        "unpriced model must carry rates: null"
    );
}
