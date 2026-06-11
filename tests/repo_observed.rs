use chrono::Utc;
use wimcc::db::{migrate, repo_observed, repo_raw, repo_runs};
use wimcc::model::meta::{PARSER_VERSION_TRANSCRIPT, SCHEMA_VERSION};
use wimcc::model::observed::{Actor, EventKind, ObservedEvent, TelemetryFacet};

use sqlx::sqlite::SqlitePoolOptions;

#[tokio::test]
async fn insert_and_list_session_events() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let run_id = repo_runs::start(&pool).await.unwrap();
    let raw = repo_raw::NewRaw {
        raw_event_id: "raw1".into(),
        ingest_run_id: run_id,
        source_type: "claude_transcript".into(),
        source_uri: "/tmp/x.jsonl".into(),
        source_line_no: 1,
        source_byte_offset: 0,
        payload_sha256: "abc".into(),
        payload: b"{}".to_vec(),
        parse_error: None,
        captured_at: chrono::Utc::now(),
        redaction_state: "not_applicable".into(),
        redaction_manifest: None,
    };
    repo_raw::insert_dedup(&pool, &raw).await.unwrap();

    let e = ObservedEvent {
        event_id: "ev1".into(),
        raw_event_id: "raw1".into(),
        schema_version: SCHEMA_VERSION.into(),
        parser_version: PARSER_VERSION_TRANSCRIPT.into(),
        session_id: "sess".into(),
        observed_at: chrono::Utc::now(),
        actor: Actor::User,
        kind: EventKind::UserMessage,
        payload: serde_json::json!({"x": 1}),
        ..Default::default()
    };
    repo_observed::insert(&pool, &e).await.unwrap();

    let evs = repo_observed::list_session(&pool, "sess", 100)
        .await
        .unwrap();
    assert_eq!(evs.len(), 1);
    assert_eq!(evs[0].event_id, "ev1");
}

#[tokio::test]
async fn round_trip_preserves_telemetry_facet() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();

    // Need a raw_event referenced by FK first.
    let run_id = repo_runs::start(&pool).await.unwrap();
    repo_raw::insert_dedup(
        &pool,
        &repo_raw::NewRaw {
            raw_event_id: "raw_test".into(),
            ingest_run_id: run_id,
            source_type: "otel".into(),
            source_uri: "otel://traces/abc/spans/def".into(),
            source_line_no: 0,
            source_byte_offset: 0,
            payload_sha256: "deadbeef".into(),
            payload: b"{}".to_vec(),
            parse_error: None,
            captured_at: Utc::now(),
            redaction_state: "not_applicable".into(),
            redaction_manifest: None,
        },
    )
    .await
    .unwrap();

    let event = ObservedEvent {
        event_id: "ev_test".into(),
        raw_event_id: "raw_test".into(),
        schema_version: "0.5.0".into(),
        session_id: "sess-otel".into(),
        observed_at: Utc::now(),
        actor: Actor::Tool,
        kind: EventKind::OtelSpan,
        trace_id: Some("5b8aa5a2d2c872e8321cf37308d69df2".into()),
        span_id: Some("051581bf3cb55c13".into()),
        parent_span_id: Some("0000000000000001".into()),
        latency_ms: Some(123),
        telemetry: Some(TelemetryFacet {
            span_name: "tool.invoke".into(),
            span_kind: Some("client".into()),
            status_code: Some("ok".into()),
            status_message: None,
            start_unix_nano: 1_734_567_890_000_000_000,
            end_unix_nano: 1_734_567_890_123_000_000,
            attributes: serde_json::json!({"tool.name": "Bash"}),
            resource: serde_json::json!({"service.name": "claude-code"}),
            scope_name: Some("wimcc.test".into()),
            scope_version: Some("0.1.0".into()),
        }),
        // Tier 3-1: otel_span events no longer re-embed the span under
        // `payload.raw_span`; the span data is carried solely by the telemetry
        // facet (merged into the stored payload on insert, split back out on
        // read). The ingest path now stores an empty payload object.
        payload: serde_json::json!({}),
        parser_version: "otel@0.1.0".into(),
        ..Default::default()
    };

    repo_observed::insert(&pool, &event).await.unwrap();
    let rows = repo_observed::list_session(&pool, "sess-otel", 10)
        .await
        .unwrap();

    assert_eq!(rows.len(), 1);
    let got = &rows[0];
    assert_eq!(got.kind, EventKind::OtelSpan);
    assert_eq!(
        got.trace_id.as_deref(),
        Some("5b8aa5a2d2c872e8321cf37308d69df2")
    );
    assert_eq!(got.span_id.as_deref(), Some("051581bf3cb55c13"));
    assert_eq!(got.parent_span_id.as_deref(), Some("0000000000000001"));
    assert_eq!(got.latency_ms, Some(123));
    let tel = got.telemetry.as_ref().expect("telemetry facet round-trips");
    assert_eq!(tel.span_name, "tool.invoke");
    assert_eq!(tel.span_kind.as_deref(), Some("client"));
    assert_eq!(tel.scope_name.as_deref(), Some("wimcc.test"));
    // Tier 3-1: the read-back payload is the empty object we stored — telemetry
    // was split back into its own field, NOT left inside payload, and no
    // `raw_span` re-embed remains.
    assert_eq!(got.payload, serde_json::json!({}));
}
