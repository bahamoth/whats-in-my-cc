# Slice-3 OTel Receiver + Telemetry Facet Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Commit messages:** Do **not** add `Co-Authored-By: Claude...` (or any other Claude attribution) footers. The repository's pre-commit hook rejects commits containing them.

**Goal:** Add an OTLP/JSON traces receiver and the `telemetry` facet on `ObservedEvent` so OTel spans become first-class graph nodes alongside transcript-derived nodes.

**Architecture:** A new HTTP endpoint `POST /otel/v1/traces` (request gzip-decompressed by `tower-http`) parses OTLP/JSON, persists each span as one `raw_event` + one `observed_event` (with populated `telemetry` facet), and rebuilds the affected sessions' graphs. A new `EventKind::OtelSpan` produces graph nodes on the existing `OTel` lane. Idempotency is delivered by deterministic `(source_uri, source_line_no, payload_sha256)` keys per `(trace_id, span_id)`.

**Tech Stack:** Rust 1.88, axum 0.7, sqlx 0.8 (SQLite), tower-http 0.5 (with `decompression-gzip` feature), serde / serde_json. Webui: React 18, TypeScript 5, Vite 5, vitest 2.

**Spec:** `docs/superpowers/specs/2026-05-19-witmcc-slice3-otel-receiver-design.md`

---

## File Structure

| Path | Action | Responsibility |
|---|---|---|
| `src/model/meta.rs` | modify | `SCHEMA_VERSION` 0.1.0 → 0.2.0, add `PARSER_VERSION_OTEL`. |
| `migrations/20260519130000_0002_telemetry.sql` | create | Index on `observed_event(trace_id, span_id)`. |
| `src/model/observed.rs` | modify | Add `trace_id/span_id/parent_span_id/latency_ms` columns, `telemetry: Option<TelemetryFacet>` payload-side, `EventKind::OtelSpan`. |
| `src/db/repo_observed.rs` | modify | INSERT and row reader cover new columns. |
| `src/ingest/otel.rs` | create | OTLP/JSON parser → `Vec<SpanRecord>` + `Vec<RejectedSpan>`, ingest store. |
| `src/ingest/mod.rs` | modify | Re-export `otel`. |
| `src/api/otel.rs` | create | `POST /otel/v1/traces` handler. |
| `src/api/mod.rs` | modify | Route registration, `RequestDecompressionLayer`. |
| `src/api/dto.rs` | modify | `OtelIngestResponse` DTO. |
| `src/api/routes.rs:172-190` | modify | `observed_to_dto` carries telemetry onto the wire; `event_raw` returns the facet too. |
| `src/api/dto.rs` | modify | `RawEventResponse` gains `telemetry: Option<Value>`. |
| `src/db/repo_raw.rs` | modify | `RawForEventRow` carries `observed_payload` (TEXT). |
| `src/graph/build.rs` | modify | New case `EventKind::OtelSpan` → `otel_span` node + merge_keys. |
| `Cargo.toml` | modify | `tower-http` adds `decompression-gzip`. |
| `tests/fixtures/otel/single_span.json` | create | one root span with `session.id`. |
| `tests/fixtures/otel/parent_child.json` | create | two spans sharing `trace_id`. |
| `tests/fixtures/otel/multi_resource.json` | create | two resourceSpans → two sessions. |
| `tests/fixtures/otel/missing_session_id.json` | create | valid span, no `session.id`. |
| `tests/fixtures/otel/malformed_traceid.json` | create | hex-invalid `traceId`. |
| `tests/otel_ingest.rs` | create | End-to-end ingestion + dedup + graph + raw lookup. |
| `tests/api.rs` | modify | `meta.schema_version` assertion 0.1.0 → 0.2.0. |
| `webui/src/api/laneMapping.ts:11-19` | modify | `'otel_span'` → `'OTel'`. |
| `webui/src/api/__tests__/client.test.ts` | modify (optional) | small mapping coverage if absent. |
| `webui/src/components/SourcePanel.tsx` | modify | OTel `record_type` branch with Attributes summary. |
| `webui/src/components/__tests__/SourcePanel.test.tsx` | modify | OTel record rendering test. |
| `webui/src/components/__tests__/Timeline.test.tsx` | modify | OTel lane regression with otel_span node. |
| `README.md` | modify | OTel section + env hint. |
| `docs/implementation-notes.html` | modify | slice-3 deviations / commits section. |

---

## Branching

Work happens on `slice3-otel-receiver` branched from `main` (post slice-2 merge). The branch already exists when this plan starts.

---

## Task 1: Bump `SCHEMA_VERSION` + add OTel parser version

**Files:**
- Modify: `src/model/meta.rs`
- Modify: `tests/api.rs:51` (assertion that pins the version)

- [ ] **Step 1: Write a failing assertion**

Edit `tests/api.rs:51` — change the expectation to the new version:

```rust
assert_eq!(v["meta"]["schema_version"], "0.2.0");
```

- [ ] **Step 2: Run cargo test, confirm it fails**

```bash
cargo test --test api -- sessions_list_contains_sess_a
```
Expected: FAIL — `left: "0.1.0"`, `right: "0.2.0"`.

- [ ] **Step 3: Bump constants**

Edit `src/model/meta.rs`:

```rust
pub const SCHEMA_VERSION: &str = "0.2.0";
pub const PARSER_VERSION_TRANSCRIPT: &str = "transcript@0.1.0";
pub const PARSER_VERSION_OTEL: &str = "otel@0.1.0";
pub const COLLECTION_PROFILE: &str = "local_transcript_slice1";
```

(Leave `COLLECTION_PROFILE` unchanged — it's profile metadata, not schema versioning. A later slice will broaden it.)

- [ ] **Step 4: Run cargo test, confirm pass**

```bash
cargo test --test api -- sessions_list_contains_sess_a
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/model/meta.rs tests/api.rs
git commit -m "chore(meta): bump SCHEMA_VERSION 0.1.0 -> 0.2.0; add PARSER_VERSION_OTEL"
```

---

## Task 2: Migration 0002 — trace_id/span_id index

**Files:**
- Create: `migrations/20260519130000_0002_telemetry.sql`
- Test: `tests/migrate.rs` (use existing infra) — runs implicitly through `tests/api.rs::make_pool`.

- [ ] **Step 1: Add a failing assertion that the index exists**

Append to `tests/api.rs` (after the existing tests):

```rust
#[tokio::test]
async fn telemetry_index_exists() {
    let pool = make_pool().await;
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type='index' AND name='idx_obs_trace_span'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, 1);
}
```

- [ ] **Step 2: Run the test, confirm it fails**

```bash
cargo test --test api -- telemetry_index_exists
```
Expected: FAIL — assertion `1 == 0`.

- [ ] **Step 3: Add migration**

Create `migrations/20260519130000_0002_telemetry.sql`:

```sql
-- 0002_telemetry: slice-3 OTel span lookup index
CREATE INDEX IF NOT EXISTS idx_obs_trace_span
  ON observed_event(trace_id, span_id)
  WHERE trace_id IS NOT NULL;
```

- [ ] **Step 4: Re-run test, confirm pass**

```bash
cargo test --test api -- telemetry_index_exists
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add migrations/20260519130000_0002_telemetry.sql tests/api.rs
git commit -m "feat(db): migration 0002 — trace_id/span_id partial index"
```

---

## Task 3: ObservedEvent telemetry fields + `EventKind::OtelSpan` + repo round-trip

**Files:**
- Modify: `src/model/observed.rs`
- Modify: `src/db/repo_observed.rs`
- Modify: `src/api/routes.rs:172-190` (`observed_to_dto`)
- Test: `tests/repo_observed.rs` (create if not present)

- [ ] **Step 1: Write the failing round-trip test**

Create `tests/repo_observed.rs`:

```rust
use chrono::Utc;
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::{migrate, repo_observed, repo_raw};
use witmcc::model::observed::{Actor, EventKind, ObservedEvent, TelemetryFacet};

#[tokio::test]
async fn round_trip_preserves_telemetry_facet() {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();

    // Need a raw_event referenced by FK first.
    use witmcc::db::repo_runs;
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
        },
    )
    .await
    .unwrap();

    let event = ObservedEvent {
        event_id: "ev_test".into(),
        raw_event_id: "raw_test".into(),
        schema_version: "0.2.0".into(),
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
            scope_name: Some("witmcc.test".into()),
            scope_version: Some("0.1.0".into()),
        }),
        payload: serde_json::json!({"raw_span": {"name": "tool.invoke"}}),
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
    assert_eq!(got.trace_id.as_deref(), Some("5b8aa5a2d2c872e8321cf37308d69df2"));
    assert_eq!(got.span_id.as_deref(), Some("051581bf3cb55c13"));
    assert_eq!(got.parent_span_id.as_deref(), Some("0000000000000001"));
    assert_eq!(got.latency_ms, Some(123));
    let tel = got.telemetry.as_ref().expect("telemetry facet round-trips");
    assert_eq!(tel.span_name, "tool.invoke");
    assert_eq!(tel.span_kind.as_deref(), Some("client"));
    assert_eq!(tel.scope_name.as_deref(), Some("witmcc.test"));
}
```

- [ ] **Step 2: Run, confirm it fails to compile**

```bash
cargo test --test repo_observed -- round_trip_preserves_telemetry_facet
```
Expected: FAIL — `TelemetryFacet` and `EventKind::OtelSpan` not found, `ObservedEvent` missing fields.

- [ ] **Step 3: Add the model**

Replace `src/model/observed.rs` content:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Actor {
    User,
    Assistant,
    #[default]
    System,
    Hook,
    Tool,
}

impl Actor {
    pub fn as_str(&self) -> &'static str {
        match self {
            Actor::User => "user",
            Actor::Assistant => "assistant",
            Actor::System => "system",
            Actor::Hook => "hook",
            Actor::Tool => "tool",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    UserMessage,
    AssistantMessage,
    Thinking,
    ToolCall,
    ToolResult,
    HookEvent,
    SystemSummary,
    SessionState,
    FileHistorySnapshot,
    AttachmentMeta,
    OtelSpan,
    #[default]
    Unknown,
}

impl EventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventKind::UserMessage => "user_message",
            EventKind::AssistantMessage => "assistant_message",
            EventKind::Thinking => "thinking",
            EventKind::ToolCall => "tool_call",
            EventKind::ToolResult => "tool_result",
            EventKind::HookEvent => "hook_event",
            EventKind::SystemSummary => "system_summary",
            EventKind::SessionState => "session_state",
            EventKind::FileHistorySnapshot => "file_history_snapshot",
            EventKind::AttachmentMeta => "attachment_meta",
            EventKind::OtelSpan => "otel_span",
            EventKind::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelemetryFacet {
    pub span_name: String,
    pub span_kind: Option<String>,
    pub status_code: Option<String>,
    pub status_message: Option<String>,
    pub start_unix_nano: i64,
    pub end_unix_nano: i64,
    #[serde(default)]
    pub attributes: Value,
    #[serde(default)]
    pub resource: Value,
    pub scope_name: Option<String>,
    pub scope_version: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ObservedEvent {
    pub event_id: String,
    pub raw_event_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub event_uuid: Option<String>,
    pub parent_uuid: Option<String>,
    pub observed_at: DateTime<Utc>,
    pub actor: Actor,
    pub kind: EventKind,
    pub subkind: Option<String>,
    pub tool_use_id: Option<String>,
    pub tool_name: Option<String>,
    pub request_id: Option<String>,
    pub message_id: Option<String>,
    pub turn_id: Option<String>,
    pub source_tool_assistant_uuid: Option<String>,
    pub source_tool_use_id: Option<String>,
    pub is_sidechain: bool,
    pub is_meta: bool,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub user_type: Option<String>,
    pub entrypoint: Option<String>,
    pub cc_version: Option<String>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub parent_span_id: Option<String>,
    pub latency_ms: Option<i64>,
    pub telemetry: Option<TelemetryFacet>,
    pub payload: Value,
    pub parser_version: String,
}
```

- [ ] **Step 4: Update repo_observed INSERT to include new columns**

Replace `src/db/repo_observed.rs` `insert` function body:

```rust
pub async fn insert(pool: &SqlitePool, e: &ObservedEvent) -> Result<()> {
    sqlx::query(
        "INSERT INTO observed_event(
            event_id, raw_event_id, schema_version, session_id, event_uuid, parent_uuid,
            observed_at, actor, kind, subkind, tool_use_id, tool_name, request_id,
            message_id, turn_id, source_tool_assistant_uuid, source_tool_use_id,
            is_sidechain, is_meta, cwd, git_branch, user_type, entrypoint, cc_version,
            trace_id, span_id, parent_span_id, latency_ms,
            payload, parser_version)
         VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&e.event_id)
    .bind(&e.raw_event_id)
    .bind(&e.schema_version)
    .bind(&e.session_id)
    .bind(&e.event_uuid)
    .bind(&e.parent_uuid)
    .bind(e.observed_at.to_rfc3339())
    .bind(e.actor.as_str())
    .bind(e.kind.as_str())
    .bind(&e.subkind)
    .bind(&e.tool_use_id)
    .bind(&e.tool_name)
    .bind(&e.request_id)
    .bind(&e.message_id)
    .bind(&e.turn_id)
    .bind(&e.source_tool_assistant_uuid)
    .bind(&e.source_tool_use_id)
    .bind(e.is_sidechain as i64)
    .bind(e.is_meta as i64)
    .bind(&e.cwd)
    .bind(&e.git_branch)
    .bind(&e.user_type)
    .bind(&e.entrypoint)
    .bind(&e.cc_version)
    .bind(&e.trace_id)
    .bind(&e.span_id)
    .bind(&e.parent_span_id)
    .bind(e.latency_ms)
    .bind(merge_payload_with_telemetry(&e.payload, e.telemetry.as_ref()).to_string())
    .bind(&e.parser_version)
    .execute(pool)
    .await?;
    Ok(())
}

/// The `telemetry` facet rides inside the `payload` JSON under the key
/// `telemetry`. Keeps the DB schema unchanged while still letting consumers
/// see the facet alongside the source payload.
fn merge_payload_with_telemetry(
    payload: &serde_json::Value,
    telemetry: Option<&crate::model::observed::TelemetryFacet>,
) -> serde_json::Value {
    let mut out = if payload.is_object() {
        payload.clone()
    } else {
        serde_json::json!({ "value": payload })
    };
    if let Some(t) = telemetry {
        if let serde_json::Value::Object(map) = &mut out {
            map.insert(
                "telemetry".into(),
                serde_json::to_value(t).unwrap_or(serde_json::Value::Null),
            );
        }
    }
    out
}
```

Add to `row_to_observed` (replace function body, keeping the column extraction extended):

```rust
fn row_to_observed(r: sqlx::sqlite::SqliteRow) -> ObservedEvent {
    let actor: String = r.get("actor");
    let kind: String = r.get("kind");
    let payload_str: String = r.get("payload");
    let mut payload: serde_json::Value =
        serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);
    let telemetry = if let serde_json::Value::Object(map) = &mut payload {
        map.remove("telemetry")
            .and_then(|v| serde_json::from_value(v).ok())
    } else {
        None
    };
    ObservedEvent {
        event_id: r.get("event_id"),
        raw_event_id: r.get("raw_event_id"),
        schema_version: r.get("schema_version"),
        parser_version: r.get("parser_version"),
        session_id: r.get("session_id"),
        event_uuid: r.try_get("event_uuid").ok(),
        parent_uuid: r.try_get("parent_uuid").ok(),
        observed_at: chrono::DateTime::parse_from_rfc3339(&r.get::<String, _>("observed_at"))
            .unwrap()
            .with_timezone(&chrono::Utc),
        actor: match actor.as_str() {
            "user" => Actor::User,
            "assistant" => Actor::Assistant,
            "hook" => Actor::Hook,
            "tool" => Actor::Tool,
            _ => Actor::System,
        },
        kind: match kind.as_str() {
            "user_message" => EventKind::UserMessage,
            "assistant_message" => EventKind::AssistantMessage,
            "thinking" => EventKind::Thinking,
            "tool_call" => EventKind::ToolCall,
            "tool_result" => EventKind::ToolResult,
            "hook_event" => EventKind::HookEvent,
            "system_summary" => EventKind::SystemSummary,
            "session_state" => EventKind::SessionState,
            "file_history_snapshot" => EventKind::FileHistorySnapshot,
            "attachment_meta" => EventKind::AttachmentMeta,
            "otel_span" => EventKind::OtelSpan,
            _ => EventKind::Unknown,
        },
        subkind: r.try_get("subkind").ok(),
        tool_use_id: r.try_get("tool_use_id").ok(),
        tool_name: r.try_get("tool_name").ok(),
        request_id: r.try_get("request_id").ok(),
        message_id: r.try_get("message_id").ok(),
        turn_id: r.try_get("turn_id").ok(),
        source_tool_assistant_uuid: r.try_get("source_tool_assistant_uuid").ok(),
        source_tool_use_id: r.try_get("source_tool_use_id").ok(),
        is_sidechain: r.get::<i64, _>("is_sidechain") != 0,
        is_meta: r.get::<i64, _>("is_meta") != 0,
        cwd: r.try_get("cwd").ok(),
        git_branch: r.try_get("git_branch").ok(),
        user_type: r.try_get("user_type").ok(),
        entrypoint: r.try_get("entrypoint").ok(),
        cc_version: r.try_get("cc_version").ok(),
        trace_id: r.try_get("trace_id").ok(),
        span_id: r.try_get("span_id").ok(),
        parent_span_id: r.try_get("parent_span_id").ok(),
        latency_ms: r.try_get("latency_ms").ok(),
        telemetry,
        payload,
    }
}
```

- [ ] **Step 5: Extend the DTO projection so the wire carries telemetry**

Edit `src/api/routes.rs:172-190` — replace `observed_to_dto`:

```rust
fn observed_to_dto(e: &crate::model::observed::ObservedEvent) -> serde_json::Value {
    let telemetry = e
        .telemetry
        .as_ref()
        .map(|t| serde_json::to_value(t).unwrap_or(serde_json::Value::Null));
    json!({
        "event_id": e.event_id,
        "raw_event_id": e.raw_event_id,
        "session_id": e.session_id,
        "event_uuid": e.event_uuid,
        "parent_uuid": e.parent_uuid,
        "observed_at": e.observed_at.to_rfc3339(),
        "actor": e.actor.as_str(),
        "kind": e.kind.as_str(),
        "subkind": e.subkind,
        "tool_use_id": e.tool_use_id,
        "tool_name": e.tool_name,
        "turn_id": e.turn_id,
        "is_sidechain": e.is_sidechain,
        "is_meta": e.is_meta,
        "trace_id": e.trace_id,
        "span_id": e.span_id,
        "parent_span_id": e.parent_span_id,
        "latency_ms": e.latency_ms,
        "telemetry": telemetry,
        "payload": e.payload,
    })
}
```

- [ ] **Step 5b: Expose telemetry on the raw endpoint too**

The SourcePanel reads `record_type` + `record` + (new) `telemetry` from `/v1/events/:event_id/raw`. We need to:

1. Extend `RawForEventRow` to include the observed_event payload (which holds the `telemetry` key per Task 3 Step 4's merge function).
2. Have the handler split the facet out and surface it as a top-level field on the response.

Edit `src/db/repo_raw.rs` — extend `RawForEventRow` and the query:

```rust
pub struct RawForEventRow {
    pub event_id: String,
    pub session_id: String,
    pub kind: String,
    pub raw_event_id: String,
    pub source_type: String,
    pub source_uri: String,
    pub source_line_no: i64,
    pub captured_at: String,
    pub payload: Vec<u8>,
    pub observed_payload: String, // serialized observed_event.payload (JSON string)
}

pub async fn get_for_event_id(pool: &SqlitePool, event_id: &str) -> Result<Option<RawForEventRow>> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT o.event_id        AS event_id, \
                o.session_id      AS session_id, \
                o.kind            AS kind, \
                o.payload         AS observed_payload, \
                r.raw_event_id    AS raw_event_id, \
                r.source_type     AS source_type, \
                r.source_uri      AS source_uri, \
                r.source_line_no  AS source_line_no, \
                r.captured_at     AS captured_at, \
                r.payload         AS payload \
         FROM observed_event o \
         JOIN raw_event r ON r.raw_event_id = o.raw_event_id \
         WHERE o.event_id = ?",
    )
    .bind(event_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| RawForEventRow {
        event_id: r.get("event_id"),
        session_id: r.get("session_id"),
        kind: r.get("kind"),
        raw_event_id: r.get("raw_event_id"),
        source_type: r.get("source_type"),
        source_uri: r.get("source_uri"),
        source_line_no: r.get("source_line_no"),
        captured_at: r.get("captured_at"),
        payload: r.get("payload"),
        observed_payload: r.get("observed_payload"),
    }))
}
```

Edit `src/api/dto.rs` — extend `RawEventResponse` with a `telemetry` field. The struct currently looks like:

```rust
#[derive(Debug, Serialize)]
pub struct RawEventResponse {
    pub schema_version: String,
    pub event_id: String,
    pub session_id: String,
    pub source: RawSource,
    pub record: serde_json::Value,
    pub record_type: String,
    pub redaction_state: String,
}
```

Add a `telemetry` field at the end:

```rust
    pub telemetry: Option<serde_json::Value>,
```

Edit `src/api/routes.rs` `event_raw` handler — after parsing the record bytes, also parse `row.observed_payload` and lift out `telemetry`:

```rust
    let observed_payload_value: serde_json::Value =
        serde_json::from_str(&row.observed_payload).unwrap_or(serde_json::Value::Null);
    let telemetry = match &observed_payload_value {
        serde_json::Value::Object(map) => map.get("telemetry").cloned(),
        _ => None,
    };
```

Then include `telemetry` in the `RawEventResponse { ... }` construction.

- [ ] **Step 6: Run all tests**

```bash
cargo test
```
Expected: PASS. The earlier `tests/api.rs` tests + the new `round_trip_preserves_telemetry_facet` should all pass.

- [ ] **Step 7: Commit**

```bash
git add src/model/observed.rs src/db/repo_observed.rs src/db/repo_raw.rs src/api/dto.rs src/api/routes.rs tests/repo_observed.rs
git commit -m "feat(model): TelemetryFacet + EventKind::OtelSpan + repo round-trip + raw endpoint telemetry"
```

---

## Task 4: OTLP/JSON parser

**Files:**
- Create: `src/ingest/otel.rs`
- Modify: `src/ingest/mod.rs`

- [ ] **Step 1: Write failing parser tests**

Append to the new file (we'll create it later, but stub the test first). Create `src/ingest/otel.rs`:

```rust
//! OTLP/JSON traces parser + ingest store.
//!
//! Slice-3 scope: traces signal only. Metrics/logs are future work.
//! The parser is intentionally permissive about unknown fields (source-preserving):
//! original span JSON is stored verbatim in `raw_event.payload`; we only extract
//! the fields the graph and UI care about.

use serde_json::Value;

#[derive(Debug, Clone)]
pub struct SpanRecord {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub name: String,
    pub kind: Option<String>,
    pub status_code: Option<String>,
    pub status_message: Option<String>,
    pub start_unix_nano: i64,
    pub end_unix_nano: i64,
    pub attributes: Value,
    pub resource: Value,
    pub scope_name: Option<String>,
    pub scope_version: Option<String>,
    pub raw: Value,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RejectedSpan {
    pub reason: String,
    pub raw: Value,
}

#[derive(Debug, Default)]
pub struct ParseResult {
    pub spans: Vec<SpanRecord>,
    pub rejected: Vec<RejectedSpan>,
}

pub fn parse_otlp_json(body: &Value) -> ParseResult {
    let mut out = ParseResult::default();
    let Some(rs_arr) = body.get("resourceSpans").and_then(|v| v.as_array()) else {
        return out;
    };
    for rs in rs_arr {
        let resource = flatten_kv(rs.get("resource").and_then(|r| r.get("attributes")));
        let resource_session = string_from(&resource, "session.id");
        let scope_spans = rs
            .get("scopeSpans")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for ss in scope_spans {
            let scope_name = ss
                .get("scope")
                .and_then(|s| s.get("name"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let scope_version = ss
                .get("scope")
                .and_then(|s| s.get("version"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let spans = ss
                .get("spans")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            for span in spans {
                match extract_span(
                    &span,
                    &resource,
                    resource_session.as_deref(),
                    scope_name.clone(),
                    scope_version.clone(),
                ) {
                    Ok(rec) => out.spans.push(rec),
                    Err(reason) => out.rejected.push(RejectedSpan {
                        reason,
                        raw: span.clone(),
                    }),
                }
            }
        }
    }
    out
}

fn extract_span(
    span: &Value,
    resource: &Value,
    resource_session: Option<&str>,
    scope_name: Option<String>,
    scope_version: Option<String>,
) -> Result<SpanRecord, String> {
    let trace_id = span
        .get("traceId")
        .and_then(|v| v.as_str())
        .ok_or("missing traceId")?
        .to_string();
    if !is_hex_of_len(&trace_id, 32) {
        return Err("malformed traceId".into());
    }
    let span_id = span
        .get("spanId")
        .and_then(|v| v.as_str())
        .ok_or("missing spanId")?
        .to_string();
    if !is_hex_of_len(&span_id, 16) {
        return Err("malformed spanId".into());
    }
    let parent_span_id = span
        .get("parentSpanId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);
    let name = span
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let kind = span
        .get("kind")
        .and_then(|v| v.as_str())
        .map(normalize_kind)
        .map(String::from);
    let status_code = span
        .get("status")
        .and_then(|s| s.get("code"))
        .and_then(|v| v.as_str())
        .map(normalize_status)
        .map(String::from);
    let status_message = span
        .get("status")
        .and_then(|s| s.get("message"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let start = parse_unix_nano(span.get("startTimeUnixNano"))?;
    let end = parse_unix_nano(span.get("endTimeUnixNano"))?;
    let attrs = flatten_kv(span.get("attributes"));
    let span_session = string_from(&attrs, "session.id");
    let session_id = span_session.or_else(|| resource_session.map(String::from));

    Ok(SpanRecord {
        trace_id,
        span_id,
        parent_span_id,
        name,
        kind,
        status_code,
        status_message,
        start_unix_nano: start,
        end_unix_nano: end,
        attributes: attrs,
        resource: resource.clone(),
        scope_name,
        scope_version,
        raw: span.clone(),
        session_id,
    })
}

fn is_hex_of_len(s: &str, n: usize) -> bool {
    s.len() == n && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn flatten_kv(attrs: Option<&Value>) -> Value {
    let mut out = serde_json::Map::new();
    let Some(arr) = attrs.and_then(|v| v.as_array()) else {
        return Value::Object(out);
    };
    for kv in arr {
        let Some(k) = kv.get("key").and_then(|v| v.as_str()) else {
            continue;
        };
        let v = kv.get("value");
        let value = match v {
            Some(o) if o.is_object() => {
                if let Some(s) = o.get("stringValue") {
                    s.clone()
                } else if let Some(b) = o.get("boolValue") {
                    b.clone()
                } else if let Some(i) = o.get("intValue") {
                    // OTLP encodes int as string; coerce.
                    match i.as_str().and_then(|s| s.parse::<i64>().ok()) {
                        Some(n) => Value::Number(n.into()),
                        None => i.clone(),
                    }
                } else if let Some(d) = o.get("doubleValue") {
                    d.clone()
                } else if let Some(a) = o.get("arrayValue") {
                    a.clone()
                } else {
                    o.clone()
                }
            }
            _ => Value::Null,
        };
        out.insert(k.into(), value);
    }
    Value::Object(out)
}

fn string_from(obj: &Value, key: &str) -> Option<String> {
    obj.get(key)
        .and_then(|v| v.as_str())
        .map(String::from)
}

fn normalize_kind(s: &str) -> &str {
    // Accept full enum strings ("SPAN_KIND_CLIENT") or short forms ("client").
    match s {
        "SPAN_KIND_INTERNAL" | "internal" => "internal",
        "SPAN_KIND_SERVER" | "server" => "server",
        "SPAN_KIND_CLIENT" | "client" => "client",
        "SPAN_KIND_PRODUCER" | "producer" => "producer",
        "SPAN_KIND_CONSUMER" | "consumer" => "consumer",
        _ => "unspecified",
    }
}

fn normalize_status(s: &str) -> &str {
    match s {
        "STATUS_CODE_OK" | "ok" => "ok",
        "STATUS_CODE_ERROR" | "error" => "error",
        _ => "unset",
    }
}

fn parse_unix_nano(v: Option<&Value>) -> Result<i64, String> {
    match v {
        Some(Value::String(s)) => s.parse::<i64>().map_err(|_| "bad unix_nano".into()),
        Some(Value::Number(n)) => n.as_i64().ok_or_else(|| "bad unix_nano".into()),
        None => Ok(0),
        _ => Err("bad unix_nano".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn single_span_fixture() -> Value {
        json!({
            "resourceSpans": [{
                "resource": {"attributes": [
                    {"key": "service.name", "value": {"stringValue": "claude-code"}},
                    {"key": "session.id",   "value": {"stringValue": "sess-otel-A"}}
                ]},
                "scopeSpans": [{
                    "scope": {"name": "witmcc.test", "version": "0.1.0"},
                    "spans": [{
                        "traceId": "5b8aa5a2d2c872e8321cf37308d69df2",
                        "spanId":  "051581bf3cb55c13",
                        "name":    "tool.invoke",
                        "kind":    "SPAN_KIND_CLIENT",
                        "startTimeUnixNano": "1734567890000000000",
                        "endTimeUnixNano":   "1734567890123000000",
                        "attributes": [
                            {"key": "tool.name", "value": {"stringValue": "Bash"}}
                        ],
                        "status": {"code": "STATUS_CODE_OK"}
                    }]
                }]
            }]
        })
    }

    #[test]
    fn parses_single_span() {
        let res = parse_otlp_json(&single_span_fixture());
        assert_eq!(res.spans.len(), 1);
        assert!(res.rejected.is_empty());
        let s = &res.spans[0];
        assert_eq!(s.trace_id, "5b8aa5a2d2c872e8321cf37308d69df2");
        assert_eq!(s.span_id, "051581bf3cb55c13");
        assert_eq!(s.name, "tool.invoke");
        assert_eq!(s.kind.as_deref(), Some("client"));
        assert_eq!(s.status_code.as_deref(), Some("ok"));
        assert_eq!(s.session_id.as_deref(), Some("sess-otel-A"));
        assert_eq!(s.scope_name.as_deref(), Some("witmcc.test"));
        assert_eq!(
            s.attributes.get("tool.name").and_then(|v| v.as_str()),
            Some("Bash")
        );
        assert_eq!(s.start_unix_nano, 1_734_567_890_000_000_000);
        assert_eq!(s.end_unix_nano, 1_734_567_890_123_000_000);
    }

    #[test]
    fn rejects_missing_trace_id() {
        let mut fx = single_span_fixture();
        fx["resourceSpans"][0]["scopeSpans"][0]["spans"][0]
            .as_object_mut()
            .unwrap()
            .remove("traceId");
        let res = parse_otlp_json(&fx);
        assert!(res.spans.is_empty());
        assert_eq!(res.rejected.len(), 1);
        assert!(res.rejected[0].reason.contains("traceId"));
    }

    #[test]
    fn rejects_malformed_hex_trace_id() {
        let mut fx = single_span_fixture();
        fx["resourceSpans"][0]["scopeSpans"][0]["spans"][0]["traceId"] =
            serde_json::Value::String("not-hex".into());
        let res = parse_otlp_json(&fx);
        assert!(res.spans.is_empty());
        assert_eq!(res.rejected.len(), 1);
        assert!(res.rejected[0].reason.contains("malformed traceId"));
    }

    #[test]
    fn span_session_attribute_overrides_resource() {
        let mut fx = single_span_fixture();
        let attrs = fx["resourceSpans"][0]["scopeSpans"][0]["spans"][0]["attributes"]
            .as_array_mut()
            .unwrap();
        attrs.push(json!({"key": "session.id", "value": {"stringValue": "from-span"}}));
        let res = parse_otlp_json(&fx);
        assert_eq!(res.spans[0].session_id.as_deref(), Some("from-span"));
    }

    #[test]
    fn missing_session_id_results_in_none() {
        let mut fx = single_span_fixture();
        fx["resourceSpans"][0]["resource"]["attributes"]
            .as_array_mut()
            .unwrap()
            .retain(|kv| kv["key"] != "session.id");
        let res = parse_otlp_json(&fx);
        assert_eq!(res.spans[0].session_id, None);
    }
}
```

Then register the module — modify `src/ingest/mod.rs`:

```rust
pub mod mapping;
pub mod otel;
pub mod store;
pub mod transcript;
```

- [ ] **Step 2: Run, confirm tests pass**

```bash
cargo test --lib ingest::otel
```
Expected: 5 unit tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/ingest/otel.rs src/ingest/mod.rs
git commit -m "feat(ingest): OTLP/JSON parser — extract spans + flatten attributes"
```

---

## Task 5: OTel fixtures

**Files:**
- Create: `tests/fixtures/otel/single_span.json`
- Create: `tests/fixtures/otel/parent_child.json`
- Create: `tests/fixtures/otel/multi_resource.json`
- Create: `tests/fixtures/otel/missing_session_id.json`
- Create: `tests/fixtures/otel/malformed_traceid.json`

- [ ] **Step 1: Create the directory**

```bash
mkdir -p tests/fixtures/otel
```

- [ ] **Step 2: Author single_span.json**

Create `tests/fixtures/otel/single_span.json`:

```json
{
  "resourceSpans": [
    {
      "resource": {
        "attributes": [
          {"key": "service.name", "value": {"stringValue": "claude-code"}},
          {"key": "session.id",   "value": {"stringValue": "sess-otel-A"}}
        ]
      },
      "scopeSpans": [
        {
          "scope": {"name": "witmcc.smoke", "version": "0.1.0"},
          "spans": [
            {
              "traceId": "5b8aa5a2d2c872e8321cf37308d69df2",
              "spanId":  "051581bf3cb55c13",
              "name":    "tool.invoke",
              "kind":    "SPAN_KIND_CLIENT",
              "startTimeUnixNano": "1734567890000000000",
              "endTimeUnixNano":   "1734567890123000000",
              "attributes": [
                {"key": "tool.name", "value": {"stringValue": "Bash"}}
              ],
              "status": {"code": "STATUS_CODE_OK"}
            }
          ]
        }
      ]
    }
  ]
}
```

- [ ] **Step 3: Author parent_child.json**

Create `tests/fixtures/otel/parent_child.json`:

```json
{
  "resourceSpans": [
    {
      "resource": {
        "attributes": [
          {"key": "session.id", "value": {"stringValue": "sess-otel-B"}}
        ]
      },
      "scopeSpans": [
        {
          "scope": {"name": "witmcc.smoke"},
          "spans": [
            {
              "traceId": "7c8aa5a2d2c872e8321cf37308d69df0",
              "spanId":  "aaaaaaaaaaaaaaaa",
              "name":    "session.turn",
              "startTimeUnixNano": "1734567890000000000",
              "endTimeUnixNano":   "1734567891000000000"
            },
            {
              "traceId": "7c8aa5a2d2c872e8321cf37308d69df0",
              "spanId":  "bbbbbbbbbbbbbbbb",
              "parentSpanId": "aaaaaaaaaaaaaaaa",
              "name":    "tool.invoke",
              "startTimeUnixNano": "1734567890100000000",
              "endTimeUnixNano":   "1734567890900000000"
            }
          ]
        }
      ]
    }
  ]
}
```

- [ ] **Step 4: Author multi_resource.json**

Create `tests/fixtures/otel/multi_resource.json`:

```json
{
  "resourceSpans": [
    {
      "resource": {"attributes": [
        {"key": "session.id", "value": {"stringValue": "sess-multi-1"}}
      ]},
      "scopeSpans": [{"spans": [{
        "traceId": "11111111111111111111111111111111",
        "spanId":  "1111111111111111",
        "name":    "first",
        "startTimeUnixNano": "1",
        "endTimeUnixNano":   "2"
      }]}]
    },
    {
      "resource": {"attributes": [
        {"key": "session.id", "value": {"stringValue": "sess-multi-2"}}
      ]},
      "scopeSpans": [{"spans": [{
        "traceId": "22222222222222222222222222222222",
        "spanId":  "2222222222222222",
        "name":    "second",
        "startTimeUnixNano": "1",
        "endTimeUnixNano":   "2"
      }]}]
    }
  ]
}
```

- [ ] **Step 5: Author missing_session_id.json**

Create `tests/fixtures/otel/missing_session_id.json`:

```json
{
  "resourceSpans": [
    {
      "resource": {"attributes": [
        {"key": "service.name", "value": {"stringValue": "claude-code"}}
      ]},
      "scopeSpans": [{"spans": [{
        "traceId": "99999999999999999999999999999999",
        "spanId":  "9999999999999999",
        "name":    "orphan",
        "startTimeUnixNano": "1",
        "endTimeUnixNano":   "2"
      }]}]
    }
  ]
}
```

- [ ] **Step 6: Author malformed_traceid.json**

Create `tests/fixtures/otel/malformed_traceid.json`:

```json
{
  "resourceSpans": [
    {
      "resource": {"attributes": []},
      "scopeSpans": [{"spans": [{
        "traceId": "not-hex",
        "spanId":  "0000000000000001",
        "name":    "broken",
        "startTimeUnixNano": "1",
        "endTimeUnixNano":   "2"
      }]}]
    }
  ]
}
```

- [ ] **Step 7: Smoke-test that fixtures parse**

Append to `src/ingest/otel.rs` (inside `mod tests`):

```rust
#[test]
fn fixtures_parse_with_expected_counts() {
    let cases = &[
        ("tests/fixtures/otel/single_span.json", 1usize, 0usize),
        ("tests/fixtures/otel/parent_child.json", 2, 0),
        ("tests/fixtures/otel/multi_resource.json", 2, 0),
        ("tests/fixtures/otel/missing_session_id.json", 1, 0),
        ("tests/fixtures/otel/malformed_traceid.json", 0, 1),
    ];
    for (path, ok, rej) in cases {
        let body: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let res = parse_otlp_json(&body);
        assert_eq!(res.spans.len(), *ok, "{path} accepted");
        assert_eq!(res.rejected.len(), *rej, "{path} rejected");
    }
}
```

Run:

```bash
cargo test --lib ingest::otel::tests::fixtures_parse_with_expected_counts
```
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add tests/fixtures/otel/ src/ingest/otel.rs
git commit -m "test(otel): JSON fixtures (single, parent/child, multi, missing-sid, malformed)"
```

---

## Task 6: OTel ingest store

**Files:**
- Modify: `src/ingest/otel.rs`
- Test: `tests/otel_ingest.rs` (created in this task)

- [ ] **Step 1: Write the failing integration test**

Create `tests/otel_ingest.rs`:

```rust
use chrono::Utc;
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::migrate;
use witmcc::ingest::otel;

async fn make_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

fn fixture(path: &str) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[tokio::test]
async fn store_single_span_inserts_one_observed_event() {
    let pool = make_pool().await;
    let body = fixture("tests/fixtures/otel/single_span.json");
    let parsed = otel::parse_otlp_json(&body);
    let res = otel::store(&pool, parsed, Utc::now()).await.unwrap();
    assert_eq!(res.accepted_spans, 1);
    assert_eq!(res.rejected_spans, 0);
    assert_eq!(res.sessions_touched, vec!["sess-otel-A".to_string()]);

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM observed_event WHERE kind = 'otel_span'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count.0, 1);

    let row: (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT trace_id, span_id FROM observed_event WHERE kind = 'otel_span' LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0.as_deref(), Some("5b8aa5a2d2c872e8321cf37308d69df2"));
    assert_eq!(row.1.as_deref(), Some("051581bf3cb55c13"));
}

#[tokio::test]
async fn store_is_idempotent() {
    let pool = make_pool().await;
    let body = fixture("tests/fixtures/otel/single_span.json");
    let r1 = otel::store(&pool, otel::parse_otlp_json(&body), Utc::now())
        .await
        .unwrap();
    let r2 = otel::store(&pool, otel::parse_otlp_json(&body), Utc::now())
        .await
        .unwrap();
    assert_eq!(r1.accepted_spans, 1);
    assert_eq!(r2.accepted_spans, 0);
    assert_eq!(r2.duplicate_spans, 1);

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM observed_event WHERE kind = 'otel_span'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count.0, 1);
}
```

- [ ] **Step 2: Run, confirm it fails**

```bash
cargo test --test otel_ingest
```
Expected: FAIL — `otel::store` doesn't exist.

- [ ] **Step 3: Implement the store**

Append to `src/ingest/otel.rs` (after the existing types):

```rust
use crate::db::{repo_observed, repo_raw, repo_runs};
use crate::error::Result;
use crate::ids::MonotonicUlidGen;
use crate::model::meta::{PARSER_VERSION_OTEL, SCHEMA_VERSION};
use crate::model::observed::{Actor, EventKind, ObservedEvent, TelemetryFacet};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::collections::BTreeSet;

#[derive(Debug, Default, Serialize)]
pub struct IngestResult {
    pub accepted_spans: u64,
    pub rejected_spans: u64,
    pub duplicate_spans: u64,
    pub sessions_touched: Vec<String>,
}

pub async fn store(
    pool: &SqlitePool,
    parsed: ParseResult,
    received_at: DateTime<Utc>,
) -> Result<IngestResult> {
    let mut gen = MonotonicUlidGen::new();
    let run_id = repo_runs::start(pool).await?;
    let mut result = IngestResult {
        rejected_spans: parsed.rejected.len() as u64,
        ..Default::default()
    };
    let mut touched: BTreeSet<String> = BTreeSet::new();

    for span in parsed.spans {
        // Canonical JSON for hashing (sort keys so re-POST is byte-stable).
        let canonical = canonical_json(&span.raw);
        let canonical_bytes = canonical.as_bytes().to_vec();
        let payload_sha = hex::encode(Sha256::digest(&canonical_bytes));
        let source_uri = format!("otel://traces/{}/spans/{}", span.trace_id, span.span_id);
        let raw_id = gen.generate();

        let inserted = repo_raw::insert_dedup(
            pool,
            &repo_raw::NewRaw {
                raw_event_id: raw_id.clone(),
                ingest_run_id: run_id.clone(),
                source_type: "otel".into(),
                source_uri,
                source_line_no: 0,
                source_byte_offset: 0,
                payload_sha256: payload_sha,
                payload: canonical_bytes,
                parse_error: None,
                captured_at: received_at,
            },
        )
        .await?;
        if !inserted {
            result.duplicate_spans += 1;
            continue;
        }

        let observed_at = unix_nano_to_utc(span.start_unix_nano).unwrap_or(received_at);
        let latency_ms = if span.end_unix_nano >= span.start_unix_nano {
            Some(((span.end_unix_nano - span.start_unix_nano) / 1_000_000) as i64)
        } else {
            Some(0)
        };
        let actor = match span.kind.as_deref() {
            Some("client") => Actor::Tool,
            _ => Actor::System,
        };
        let session_id = span.session_id.clone().unwrap_or_default();
        let telemetry = TelemetryFacet {
            span_name: span.name.clone(),
            span_kind: span.kind.clone(),
            status_code: span.status_code.clone(),
            status_message: span.status_message.clone(),
            start_unix_nano: span.start_unix_nano,
            end_unix_nano: span.end_unix_nano,
            attributes: span.attributes.clone(),
            resource: span.resource.clone(),
            scope_name: span.scope_name.clone(),
            scope_version: span.scope_version.clone(),
        };

        let event = ObservedEvent {
            event_id: gen.generate(),
            raw_event_id: raw_id,
            schema_version: SCHEMA_VERSION.into(),
            session_id: session_id.clone(),
            observed_at,
            actor,
            kind: EventKind::OtelSpan,
            tool_name: span
                .attributes
                .get("tool.name")
                .and_then(|v| v.as_str())
                .map(String::from),
            trace_id: Some(span.trace_id.clone()),
            span_id: Some(span.span_id.clone()),
            parent_span_id: span.parent_span_id.clone(),
            latency_ms,
            telemetry: Some(telemetry),
            payload: serde_json::json!({"raw_span": span.raw}),
            parser_version: PARSER_VERSION_OTEL.into(),
            ..Default::default()
        };
        repo_observed::insert(pool, &event).await?;

        result.accepted_spans += 1;
        if !session_id.is_empty() {
            touched.insert(session_id);
        }
    }

    repo_runs::finish(
        pool,
        &run_id,
        "ok",
        serde_json::to_value(&result).unwrap_or(serde_json::Value::Null),
    )
    .await?;

    result.sessions_touched = touched.into_iter().collect();
    Ok(result)
}

fn canonical_json(value: &Value) -> String {
    // Recursively sort object keys so byte representation is stable.
    fn norm(v: &Value) -> Value {
        match v {
            Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                let mut out = serde_json::Map::new();
                for k in keys {
                    out.insert(k.clone(), norm(&map[k]));
                }
                Value::Object(out)
            }
            Value::Array(arr) => Value::Array(arr.iter().map(norm).collect()),
            _ => v.clone(),
        }
    }
    norm(value).to_string()
}

fn unix_nano_to_utc(nano: i64) -> Option<DateTime<Utc>> {
    if nano <= 0 {
        return None;
    }
    let secs = nano / 1_000_000_000;
    let nsec = (nano % 1_000_000_000) as u32;
    chrono::DateTime::<Utc>::from_timestamp(secs, nsec)
}
```

- [ ] **Step 4: Run, confirm tests pass**

```bash
cargo test --test otel_ingest
```
Expected: PASS (both tests).

- [ ] **Step 5: Commit**

```bash
git add src/ingest/otel.rs tests/otel_ingest.rs
git commit -m "feat(ingest): otel::store — insert raw + observed with telemetry facet"
```

---

## Task 7: Graph builder — `otel_span` node

**Files:**
- Modify: `src/graph/build.rs:34-58` (`compute` match arm)
- Test: `tests/otel_ingest.rs` (add a graph assertion)

- [ ] **Step 1: Add a failing test**

Append to `tests/otel_ingest.rs`:

```rust
use witmcc::graph::build;

#[tokio::test]
async fn graph_has_otel_span_node_after_ingest() {
    let pool = make_pool().await;
    let body = fixture("tests/fixtures/otel/parent_child.json");
    otel::store(&pool, otel::parse_otlp_json(&body), Utc::now())
        .await
        .unwrap();
    let (n, e) = build::rebuild_session(&pool, "sess-otel-B").await.unwrap();
    assert_eq!(n, 2, "two otel_span nodes from parent_child fixture");
    assert_eq!(e, 0, "no edges emitted in slice-3");

    let row: (String,) = sqlx::query_as(
        "SELECT node_kind FROM graph_node WHERE session_id = ? ORDER BY started_at LIMIT 1",
    )
    .bind("sess-otel-B")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "otel_span");
}
```

- [ ] **Step 2: Run, confirm it fails**

```bash
cargo test --test otel_ingest -- graph_has_otel_span_node_after_ingest
```
Expected: FAIL — `EventKind::OtelSpan` falls through the `_ => continue` arm.

- [ ] **Step 3: Add the match arm**

Edit `src/graph/build.rs` — inside the `match e.kind` block inside `compute`, before the catch-all `_ => continue`:

```rust
            EventKind::OtelSpan => (
                "otel_span",
                json!({
                    "session_id": session_id,
                    "trace_id":   e.trace_id,
                    "span_id":    e.span_id,
                }),
            ),
```

- [ ] **Step 4: Run, confirm it passes**

```bash
cargo test --test otel_ingest -- graph_has_otel_span_node_after_ingest
```
Expected: PASS.

- [ ] **Step 5: Full suite sanity check**

```bash
cargo test
```
Expected: PASS (all existing + new tests).

- [ ] **Step 6: Commit**

```bash
git add src/graph/build.rs tests/otel_ingest.rs
git commit -m "feat(graph): materialize otel_span node with (trace_id, span_id) merge_keys"
```

---

## Task 8: HTTP handler — `POST /otel/v1/traces`

**Files:**
- Modify: `Cargo.toml` (add `decompression-gzip` feature)
- Create: `src/api/otel.rs`
- Modify: `src/api/dto.rs` (DTO)
- Modify: `src/api/mod.rs` (route + decompression layer)
- Test: `tests/otel_ingest.rs` (HTTP path test)

- [ ] **Step 1: Add `tower-http` feature**

Edit `Cargo.toml` — update the `tower-http` line:

```toml
tower-http         = { version = "0.5",  features = ["trace", "decompression-gzip"] }
```

- [ ] **Step 2: Write the failing HTTP test**

Append to `tests/otel_ingest.rs`:

```rust
use axum_test::TestServer;

async fn http_setup() -> TestServer {
    let pool = make_pool().await;
    let app = witmcc::api::router(pool);
    TestServer::new(app).unwrap()
}

#[tokio::test]
async fn post_traces_returns_accepted_count() {
    let s = http_setup().await;
    let body = fixture("tests/fixtures/otel/single_span.json");
    let resp = s
        .post("/otel/v1/traces")
        .json(&body)
        .await;
    resp.assert_status_ok();
    let v: serde_json::Value = resp.json();
    assert_eq!(v["meta"]["schema_version"], "0.2.0");
    assert_eq!(v["data"]["accepted_spans"], 1);
    assert_eq!(v["data"]["rejected_spans"], 0);
    assert_eq!(v["data"]["sessions_touched"][0], "sess-otel-A");
}

#[tokio::test]
async fn post_traces_with_malformed_trace_id_rejects_span() {
    let s = http_setup().await;
    let body = fixture("tests/fixtures/otel/malformed_traceid.json");
    let resp = s.post("/otel/v1/traces").json(&body).await;
    resp.assert_status_ok();
    let v: serde_json::Value = resp.json();
    assert_eq!(v["data"]["accepted_spans"], 0);
    assert_eq!(v["data"]["rejected_spans"], 1);
}

#[tokio::test]
async fn post_traces_with_non_json_body_is_400() {
    let s = http_setup().await;
    let resp = s
        .post("/otel/v1/traces")
        .add_header("content-type", "application/json")
        .text("not json")
        .await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
}
```

- [ ] **Step 3: Run, confirm it fails**

```bash
cargo test --test otel_ingest -- post_traces_returns_accepted_count
```
Expected: FAIL — `404 Not Found` (no route).

- [ ] **Step 4: Add the DTO**

Append to `src/api/dto.rs`:

```rust
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct OtelIngestResponse {
    pub accepted_spans: u64,
    pub rejected_spans: u64,
    pub duplicate_spans: u64,
    pub sessions_touched: Vec<String>,
}
```

(If `use serde::Serialize` is already imported at the top, do not duplicate the line. The file currently imports `serde::Serialize` near the top — check before re-adding.)

- [ ] **Step 5: Add the handler**

Create `src/api/otel.rs`:

```rust
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde_json::json;
use sqlx::SqlitePool;

use crate::api::dto::OtelIngestResponse;
use crate::ingest::otel;
use crate::model::meta::{Envelope, ResponseMeta};

const MAX_DECOMPRESSED_BYTES: usize = 4 * 1024 * 1024;

pub async fn ingest_traces(
    State(pool): State<SqlitePool>,
    body: axum::body::Bytes,
) -> Result<Json<Envelope<OtelIngestResponse>>, (StatusCode, Json<serde_json::Value>)> {
    if body.len() > MAX_DECOMPRESSED_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({
                "type": "about:blank",
                "title": "PAYLOAD_TOO_LARGE",
                "detail": format!("body exceeds {} bytes", MAX_DECOMPRESSED_BYTES),
            })),
        ));
    }
    let value: serde_json::Value = serde_json::from_slice(&body).map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "type": "about:blank",
                "title": "BAD_REQUEST",
                "detail": format!("json parse error: {err}"),
            })),
        )
    })?;
    let parsed = otel::parse_otlp_json(&value);
    let result = otel::store(&pool, parsed, chrono::Utc::now())
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "type": "about:blank",
                    "title": "DB_FAILURE",
                    "detail": format!("{err}"),
                })),
            )
        })?;

    Ok(Json(Envelope {
        meta: ResponseMeta::now(),
        data: OtelIngestResponse {
            accepted_spans: result.accepted_spans,
            rejected_spans: result.rejected_spans,
            duplicate_spans: result.duplicate_spans,
            sessions_touched: result.sessions_touched,
        },
    }))
}
```

- [ ] **Step 6: Wire the route + decompression layer + body limit**

Replace `src/api/mod.rs`:

```rust
pub mod dto;
pub mod middleware;
pub mod otel;
pub mod routes;
pub mod static_assets;

use axum::{
    extract::DefaultBodyLimit, middleware as axum_mw, routing::get, routing::post, Router,
};
use sqlx::SqlitePool;
use tower_http::decompression::RequestDecompressionLayer;

const MAX_REQUEST_BODY: usize = 4 * 1024 * 1024;

pub fn router(pool: SqlitePool) -> Router {
    Router::new()
        .route("/v1/health", get(routes::health))
        .route("/v1/sessions", get(routes::list_sessions))
        .route("/v1/sessions/:id", get(routes::session_detail))
        .route("/v1/sessions/:id/graph", get(routes::session_graph))
        .route("/v1/events/:event_id/raw", get(routes::event_raw))
        .route("/otel/v1/traces", post(otel::ingest_traces))
        .fallback(static_assets::spa_handler)
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BODY))
        .layer(RequestDecompressionLayer::new().gzip(true))
        .layer(axum_mw::from_fn(middleware::host_allowlist))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(pool)
}
```

`DefaultBodyLimit::max(4 MB)` overrides axum's 2 MB default for all routes; the GET routes don't care, and the OTel POST handler still does its own >4 MB guard against decompressed bombs.

- [ ] **Step 7: Run the HTTP tests**

```bash
cargo test --test otel_ingest -- post_traces_
```
Expected: PASS (3 tests: accepted_count, malformed, non_json).

- [ ] **Step 8: Run the entire suite**

```bash
cargo test
```
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml Cargo.lock src/api/otel.rs src/api/dto.rs src/api/mod.rs tests/otel_ingest.rs
git commit -m "feat(api): POST /otel/v1/traces — receive OTLP/JSON spans (gzip-aware)"
```

---

## Task 9: End-to-end integration coverage (gzip + missing session)

**Files:**
- Modify: `tests/otel_ingest.rs`

- [ ] **Step 1: Add gzip + missing-session tests**

Append to `tests/otel_ingest.rs`:

```rust
use std::io::Write;

fn gzip(bytes: &[u8]) -> Vec<u8> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(bytes).unwrap();
    enc.finish().unwrap()
}

#[tokio::test]
async fn post_traces_gzip_body_is_decompressed() {
    let s = http_setup().await;
    let body = fixture("tests/fixtures/otel/parent_child.json");
    let bytes = serde_json::to_vec(&body).unwrap();
    let gz = gzip(&bytes);
    let resp = s
        .post("/otel/v1/traces")
        .add_header("content-type", "application/json")
        .add_header("content-encoding", "gzip")
        .bytes(gz.into())
        .await;
    resp.assert_status_ok();
    let v: serde_json::Value = resp.json();
    assert_eq!(v["data"]["accepted_spans"], 2);
}

#[tokio::test]
async fn post_traces_without_session_id_skips_session_listing() {
    let s = http_setup().await;
    let body = fixture("tests/fixtures/otel/missing_session_id.json");
    let resp = s.post("/otel/v1/traces").json(&body).await;
    resp.assert_status_ok();
    let v: serde_json::Value = resp.json();
    assert_eq!(v["data"]["accepted_spans"], 1);
    assert!(
        v["data"]["sessions_touched"].as_array().unwrap().is_empty(),
        "no session.id means no session listed"
    );
    let listed: serde_json::Value = s.get("/v1/sessions").await.json();
    let arr = listed["data"].as_array().unwrap();
    assert!(arr.iter().all(|s| s["session_id"] != ""));
}
```

- [ ] **Step 2: Add `flate2` as dev-dep**

Edit `Cargo.toml` `[dev-dependencies]` section — add:

```toml
flate2             = "1"
```

- [ ] **Step 3: Run the gzip test**

```bash
cargo test --test otel_ingest -- post_traces_gzip_body_is_decompressed
```
Expected: PASS.

- [ ] **Step 4: Run missing session test**

```bash
cargo test --test otel_ingest -- post_traces_without_session_id_skips_session_listing
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock tests/otel_ingest.rs
git commit -m "test(otel): gzip body decompression + missing session.id behaviour"
```

---

## Task 10: Webui — lane mapping for `otel_span`

**Files:**
- Modify: `webui/src/api/laneMapping.ts`
- Test: `webui/src/api/__tests__/laneMapping.test.ts` (create)

- [ ] **Step 1: Write the failing test**

Create `webui/src/api/__tests__/laneMapping.test.ts`:

```typescript
import { describe, expect, it } from 'vitest';
import { laneForNodeKind } from '../laneMapping';

describe('laneForNodeKind', () => {
  it('maps tool_call to Action', () => {
    expect(laneForNodeKind('tool_call')).toBe('Action');
  });

  it('maps otel_span to OTel', () => {
    expect(laneForNodeKind('otel_span')).toBe('OTel');
  });

  it('returns null for unknown kinds', () => {
    expect(laneForNodeKind('unknown_kind')).toBeNull();
  });
});
```

- [ ] **Step 2: Run, confirm it fails**

```bash
cd webui && npm run test -- --run laneMapping
```
Expected: FAIL — `expected 'OTel' got null`.

- [ ] **Step 3: Add the case**

Edit `webui/src/api/laneMapping.ts`:

```ts
export const LANES = [
  'Intent',
  'Context',
  'Action',
  'State',
  'OTel',
  'Quality',
] as const;
export type Lane = (typeof LANES)[number];

export function laneForNodeKind(kind: string): Lane | null {
  switch (kind) {
    case 'user_message':            return 'Intent';
    case 'assistant_message':       return 'Context';
    case 'tool_call':               return 'Action';
    case 'tool_result':             return 'Action'; // merged into tool_call, but defensive
    case 'file_history_snapshot':   return 'State';
    case 'otel_span':               return 'OTel';
    default:                        return null;
  }
}
```

- [ ] **Step 4: Run, confirm it passes**

```bash
cd webui && npm run test -- --run laneMapping
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add webui/src/api/laneMapping.ts webui/src/api/__tests__/laneMapping.test.ts
git commit -m "feat(webui): laneMapping otel_span -> OTel"
```

---

## Task 11: Webui — SourcePanel OTel rendering

**Files:**
- Modify: `webui/src/components/SourcePanel.tsx`
- Modify: `webui/src/components/SourcePanel.module.css`
- Modify: `webui/src/api/types.ts` (add optional `telemetry` to `RawEventResponse` type)
- Modify: `webui/src/components/__tests__/SourcePanel.test.tsx`

`SourcePanel` (see `webui/src/components/SourcePanel.tsx`) takes a single prop `eventId: string | null`, runs `getEventRaw` internally, and renders the response. Existing tests mock `globalThis.fetch`. We extend the response shape (server side already does this in Task 3 Step 5b) and render an Attributes table when `record_type === 'otel_span'`.

- [ ] **Step 1: Extend the TypeScript response type**

Edit `webui/src/api/types.ts` — find `RawEventResponse` and append an optional `telemetry` field:

```ts
export type RawEventResponse = {
  schema_version: string;
  event_id: string;
  session_id: string;
  source: {
    kind: string;
    file_path: string;
    line_no: number;
    ingested_at: string;
  };
  record: unknown;
  record_type: string;
  redaction_state: string;
  telemetry?: {
    span_name?: string;
    span_kind?: string | null;
    status_code?: string | null;
    status_message?: string | null;
    attributes?: Record<string, unknown>;
    resource?: Record<string, unknown>;
  } | null;
};
```

(Read the file first — if the existing type doesn't fully match the snippet above, preserve existing fields and only add the optional `telemetry` block.)

- [ ] **Step 2: Write the failing test**

Append to `webui/src/components/__tests__/SourcePanel.test.tsx` (reuse the `envelope` helper already defined in the file):

```tsx
  it('renders Attributes section when record_type is otel_span', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope({
      schema_version: '0.2.0',
      event_id: 'ev_o',
      session_id: 'sess-otel-A',
      source: {
        kind: 'otel',
        file_path: 'otel://traces/abc/spans/def',
        line_no: 0,
        ingested_at: '2026-05-19T00:00:00Z',
      },
      record: {
        traceId: '5b8aa5a2d2c872e8321cf37308d69df2',
        spanId: '051581bf3cb55c13',
        name: 'tool.invoke',
      },
      record_type: 'otel_span',
      redaction_state: 'none',
      telemetry: {
        span_name: 'tool.invoke',
        span_kind: 'client',
        status_code: 'ok',
        attributes: { 'tool.name': 'Bash', 'session.id': 'sess-otel-A' },
      },
    }));
    render(<SourcePanel eventId="ev_o" />);
    await waitFor(() => expect(screen.getByText('Attributes')).toBeInTheDocument());
    expect(screen.getByText('tool.name')).toBeInTheDocument();
    expect(screen.getByText('Bash')).toBeInTheDocument();
  });
```

- [ ] **Step 3: Run, confirm it fails**

```bash
cd webui && npm run test -- --run SourcePanel
```
Expected: FAIL — `Attributes` text not in DOM.

- [ ] **Step 4: Extend `SourcePanel.tsx`**

Replace the `state.kind === 'ok'` branch of `webui/src/components/SourcePanel.tsx` with:

```tsx
      {state.kind === 'ok' && (
        <>
          <header className={styles.header}>
            <span className={styles.type}>{state.data.record_type}</span>
            <span className={styles.source}>
              <span>{state.data.source.file_path}</span>
              <span>:{state.data.source.line_no}</span>
            </span>
          </header>
          {state.data.record_type === 'otel_span' && state.data.telemetry?.attributes && (
            <section className={styles.attributes} aria-labelledby="otel-attrs-heading">
              <h4 id="otel-attrs-heading">Attributes</h4>
              <table>
                <tbody>
                  {Object.entries(state.data.telemetry.attributes).map(([k, v]) => (
                    <tr key={k}>
                      <td className={styles.attrKey}>{k}</td>
                      <td className={styles.attrValue}>{String(v)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
          )}
          <div className={styles.body}>
            <JsonView data={state.data.record} />
          </div>
        </>
      )}
```

- [ ] **Step 5: Add CSS**

Append to `webui/src/components/SourcePanel.module.css`:

```css
.attributes { margin: 0 0 12px; padding: 8px 10px; border: 1px solid rgba(160,180,220,.18); border-radius: 8px; }
.attributes h4 { margin: 0 0 6px; font-size: 12px; letter-spacing: .08em; text-transform: uppercase; color: var(--muted, #9aa8bf); }
.attributes table { width: 100%; border-collapse: collapse; font-size: 12px; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
.attrKey { color: var(--muted, #9aa8bf); padding: 2px 8px 2px 0; vertical-align: top; white-space: nowrap; }
.attrValue { padding: 2px 0; word-break: break-all; color: var(--ink, #edf3ff); }
```

- [ ] **Step 6: Run, confirm it passes**

```bash
cd webui && npm run test -- --run SourcePanel
```
Expected: PASS (all four tests including the new OTel one).

- [ ] **Step 7: Run the full webui test suite**

```bash
cd webui && npm test -- --run
```
Expected: All passing.

- [ ] **Step 8: Commit**

```bash
git add webui/src/api/types.ts webui/src/components/SourcePanel.tsx webui/src/components/SourcePanel.module.css webui/src/components/__tests__/SourcePanel.test.tsx
git commit -m "feat(webui): SourcePanel renders OTel Attributes section for otel_span records"
```

---

## Task 12: Webui — Timeline OTel lane regression test

**Files:**
- Modify: `webui/src/components/__tests__/Timeline.test.tsx`

The Timeline component already renders all six `LANES` and tags each marker with `data-node-id`. With Task 10's lane mapping in place, an `otel_span` node naturally lands on the `OTel` lane. This task is a pure regression test — no production-code change.

- [ ] **Step 1: Add the failing regression test**

Append to `webui/src/components/__tests__/Timeline.test.tsx`:

```tsx
  it('renders an otel_span node marker (regression for slice-3)', () => {
    const graph = {
      nodes: [
        {
          node_id: 'nd_o_1',
          schema_version: '0.2.0',
          session_id: 's',
          node_kind: 'otel_span',
          started_at: '2026-05-19T00:00:00Z',
          ended_at: '2026-05-19T00:00:01Z',
          merge_keys: { trace_id: 't', span_id: 's' },
          source_event_ids: ['ev_o_1'],
          source_uris: [],
          payload: {},
        },
      ],
      edges: [],
    };
    render(<Timeline graph={graph} selectedNodeId={null} onSelect={() => {}} />);
    const marker = document.querySelector('[data-node-id="nd_o_1"]');
    expect(marker).not.toBeNull();
    // OTel lane label is present.
    expect(screen.getByText('OTel')).toBeInTheDocument();
    // The "no OTel observed" placeholder must NOT appear when an OTel node exists.
    expect(screen.queryByText(/no OTel observed/i)).toBeNull();
  });
```

(Reuse the existing top-of-file imports — `render`, `screen` from `@testing-library/react`, `describe`/`it`/`expect` from `vitest`, and the `Timeline` import. If the file already has a graph factory helper, use it; otherwise the inline object above works.)

- [ ] **Step 2: Run, confirm it passes**

```bash
cd webui && npm run test -- --run Timeline
```
Expected: PASS (depends on Task 10's lane mapping being in place — if Task 10 was skipped, the marker would land in no lane and `posByNodeId` wouldn't have it; FAIL would indicate Task 10 hasn't landed).

- [ ] **Step 3: Run the full webui suite**

```bash
cd webui && npm test -- --run
```
Expected: All passing.

- [ ] **Step 4: Commit**

```bash
git add webui/src/components/__tests__/Timeline.test.tsx
git commit -m "test(webui): regression — otel_span node renders on OTel lane"
```

---

## Task 13: README + implementation-notes update

**Files:**
- Modify: `README.md`
- Modify: `docs/implementation-notes.html`

- [ ] **Step 1: Update README**

Add a new section after the existing "Web UI" section in `README.md` (use the actual surrounding markup; insert near the API examples):

````markdown
### OTel Traces Receiver (slice-3)

`POST /otel/v1/traces` accepts OTLP/JSON traces. gzip-encoded request bodies are
decompressed automatically. Set your exporter to JSON:

```bash
export OTEL_EXPORTER_OTLP_TRACES_PROTOCOL=http/json
export OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:7878/otel
```

Manual smoke test:

```bash
curl -X POST http://127.0.0.1:7878/otel/v1/traces \
  -H 'Content-Type: application/json' \
  --data-binary @tests/fixtures/otel/single_span.json
```

Notes:
- traces signal only — metrics/logs are future slices.
- Spans without `session.id` are stored but excluded from `/v1/sessions`.
- No redaction yet — do not send spans containing secrets.
````

- [ ] **Step 2: Update implementation-notes.html**

Open `docs/implementation-notes.html` and:

1. Add new section IDs to the sidebar `localnav` block:

```html
<a href="#slice3-overview">Overview (slice-3)</a>
<a href="#slice3-deviations">Intentional Deviations (slice-3)</a>
<a href="#slice3-commits">Commit Reference (slice-3)</a>
```

2. Add a new section just before the `<footer class="footer">`:

```html
<section id="slice3-overview">
  <h2><span class="num">07</span>Slice-3 Overview</h2>
  <p><code>slice3-otel-receiver</code> branch — OTLP/JSON traces receiver
  + <code>telemetry</code> facet on ObservedEvent. CLAUDE.md "OTel-first" 원칙을
  schema에 박는 작업.</p>
  <div class="callout good">
    <strong>전체 상태:</strong> slice-3 완료. POST /otel/v1/traces 정상 동작.
    SCHEMA_VERSION 0.1.0 → 0.2.0.
  </div>
</section>

<section id="slice3-deviations">
  <h2><span class="num">08</span>Slice-3 Intentional Deviations</h2>
  <div class="dev-item">
    <div class="dev-head">
      <span class="dev-id">DEV-S3-01</span>
      <strong>OTLP/JSON only (no protobuf)</strong>
      <span class="pill blue">api</span>
    </div>
    <div class="dev-body">
      <p>OTLP/HTTP은 protobuf와 JSON 둘 다 지원하지만 slice-3는 JSON만 받는다.
      SDK 측에서 <code>OTEL_EXPORTER_OTLP_TRACES_PROTOCOL=http/json</code> 설정 필요.</p>
    </div>
  </div>

  <div class="dev-item">
    <div class="dev-head">
      <span class="dev-id">DEV-S3-02</span>
      <strong>Traces only — no metrics/logs</strong>
      <span class="pill blue">api</span>
    </div>
    <div class="dev-body">
      <p>metrics는 time-series 모델이라 ObservedEvent에 직접 매핑되지 않고,
      logs는 traces가 가진 인과 정보 없이 양만 늘린다. 두 signal은 후속 슬라이스.</p>
    </div>
  </div>

  <div class="dev-item">
    <div class="dev-head">
      <span class="dev-id">DEV-S3-03</span>
      <strong>Telemetry facet stored inside <code>payload</code> JSON</strong>
      <span class="pill violet">model</span>
    </div>
    <div class="dev-body">
      <p>DB schema 변경 회피를 위해 facet은 <code>payload.telemetry</code> 키 아래
      포함된다. <code>trace_id</code>/<code>span_id</code>/<code>parent_span_id</code>/
      <code>latency_ms</code>는 top-level 컬럼 사용 — 인덱스 가능.</p>
    </div>
  </div>

  <div class="dev-item">
    <div class="dev-head">
      <span class="dev-id">DEV-S3-04</span>
      <strong>No transcript ↔ OTel merge in slice-3</strong>
      <span class="pill amber">graph</span>
    </div>
    <div class="dev-body">
      <p>Transcript producer가 아직 <code>trace_id</code>를 emit하지 않으므로
      merge가 자동 발생하지 않는다. graph_node의 merge_keys에
      <code>trace_id+span_id</code>는 깔려 있어 producer 추가 시 자동 동작.</p>
    </div>
  </div>

  <div class="dev-item">
    <div class="dev-head">
      <span class="dev-id">DEV-S3-05</span>
      <strong>No span_parent edges</strong>
      <span class="pill amber">graph</span>
    </div>
    <div class="dev-body">
      <p><code>parent_span_id</code>는 column에 저장되지만 <code>span_parent</code>
      edge kind는 아직 emit하지 않는다. 데이터 경로 우선 검증 후 후속 슬라이스에서 추가.</p>
    </div>
  </div>
</section>

<section id="slice3-commits">
  <h2><span class="num">09</span>Slice-3 Commit Reference</h2>
  <p>커밋 SHA는 PR 머지 후 채워 넣는다.</p>
</section>
```

3. Also update the `<header class="hero">` paragraph at the top to mention slice-3 if appropriate.

- [ ] **Step 3: Commit**

```bash
git add README.md docs/implementation-notes.html
git commit -m "docs(slice-3): README OTel section + implementation notes deviations"
```

---

## Final Verification

After all 13 tasks land, run the full suite from repo root:

```bash
just webui-build
cargo test
cd webui && npm test -- --run && cd ..
```

Expected: all green. The previous slice's 31 cargo + 19 vitest tests stay passing; new tests added by this slice raise both totals.

Manual smoke:

```bash
witmcc serve --bind 127.0.0.1 --port 7878 --db-path /tmp/witmcc-slice3.db --auto-migrate &
sleep 1
curl -s -X POST http://127.0.0.1:7878/otel/v1/traces \
  -H 'Content-Type: application/json' \
  --data-binary @tests/fixtures/otel/single_span.json | jq
curl -s http://127.0.0.1:7878/v1/sessions | jq
curl -s http://127.0.0.1:7878/v1/sessions/sess-otel-A/graph | jq
kill %1
```

Expected: `accepted_spans: 1`, sess-otel-A listed, graph contains one `otel_span` node.

Browser smoke:
1. `witmcc serve` (with the same DB).
2. Open `http://127.0.0.1:7878/`.
3. Click into `sess-otel-A`.
4. Confirm the OTel lane shows a marker, clicking it opens the SourcePanel with the Attributes section visible.

---

## Self-Review Checklist

This plan was self-reviewed against the spec:

- **Spec §3 Architecture** — covered by Tasks 4, 6, 8.
- **Spec §4 API Surface** — POST handler in Task 8; response shape exercised in Task 8/9.
- **Spec §5 Data Model** — Tasks 1, 2, 3 cover schema_version bump, migration index, and ObservedEvent extension. `payload.telemetry` strategy documented in DEV-S3-03.
- **Spec §6 Graph Mapping** — Task 7 adds the `otel_span` node kind with `(trace_id, span_id)` merge_keys.
- **Spec §7 UI Changes** — Tasks 10–12.
- **Spec §8 Error Handling** — Malformed trace_id in Task 4 + 8; missing session.id in Task 9; gzip oversize implicit via `MAX_DECOMPRESSED_BYTES` constant in Task 8.
- **Spec §9 Test Strategy** — Fixtures in Task 5; ingest tests in Tasks 6–9; UI tests in Tasks 10–12.
- **Spec §14 Acceptance** — all seven criteria covered by tests added across Tasks 6, 7, 8, 11, 12.
