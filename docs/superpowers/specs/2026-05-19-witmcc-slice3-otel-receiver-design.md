# Slice-3 Design — OTel Receiver + Telemetry Facet

**Date:** 2026-05-19
**Branch:** `slice3-otel-receiver` (based on `main` post slice-2)
**Goal:** Stand up the OTLP/JSON traces receiver and add the telemetry facet to `ObservedEvent` so that OTel spans become first-class graph nodes alongside transcript-derived nodes.

---

## 1. Motivation

CLAUDE.md elevates "OTel-first" to a binding principle: `trace_id` / `span_id` must be first-class correlation keys, not bolt-on metadata. Slice-1 and slice-2 shipped the transcript path with `trace_id`/`span_id` columns reserved in the schema but no producer wiring them. Slice-3 proves the data path end-to-end:

- Accept OTLP/JSON traces over HTTP.
- Persist each span as an `ObservedEvent` with a populated `telemetry` facet.
- Render OTel spans on the existing six-lane timeline (OTel lane already declared in `webui/src/api/laneMapping.ts`).
- Reserve `merge_keys` for future transcript ↔ OTel correlation (transcript today emits no `trace_id`; correlation activates only when that changes).

AC-2 (≥90% trace linkage) remains unmet by slice-3 alone — that gate unlocks when a transcript producer also emits `trace_id`. Slice-3 commits to **proving the path**, not the linkage metric.

---

## 2. Scope

### In Scope

- **OTLP/JSON traces** receiver at `POST /otel/v1/traces` with gzip support.
- `ObservedEvent.telemetry` Rust facet populated for OTel-origin records.
- New `EventKind::OtelSpan` mapped to graph node kind `otel_span`, lane `OTel`.
- New `source_type` value `"otel"` in `raw_event` rows.
- `migrations/20260520xxxxxx_0002_telemetry.sql` only if needed (most columns already exist).
- Timeline / SourcePanel render OTel spans (lane mapping update plus attribute display).
- Hand-crafted OTLP/JSON fixtures and an integration test that posts them through the live HTTP server.
- `webui/src/components/Timeline.tsx` recognises the OTel lane (already defined in `LANES`, just needs `node_kind → lane` wiring).

### Out of Scope (deferred)

- OTLP **metrics** and **logs** signals.
- OTLP/**protobuf** binary encoding.
- gRPC OTLP transport.
- Transcript ↔ OTel **automatic merge** (transcript today lacks `trace_id`; we only set merge_keys so a future slice that wires the producer gets merge for free).
- Span parent → child **graph edges** (`span_parent` edge kind) — keep nodes only in this slice.
- Live tail / file watcher (separate slice candidate).
- Token authentication, redaction, export bundles (M7).
- Findings engine (M5).
- AC-2 quantitative measurement.

---

## 3. Architecture

```
┌─────────────┐   POST /otel/v1/traces      ┌──────────────────────┐
│ OTel sender │ ─────────────────────────▶  │ src/api/otel.rs       │
│  (any SDK,  │  Content-Type:              │  - body limit 4 MB    │
│   gzip OK)  │    application/json         │  - gzip decode        │
└─────────────┘                              │  - parse OTLP JSON    │
                                             └──────────┬───────────┘
                                                        │ Vec<SpanRecord>
                                                        ▼
                                             ┌──────────────────────┐
                                             │ src/ingest/otel.rs    │
                                             │  span → ObservedEvent │
                                             │  span → RawEvent      │
                                             │  idempotent dedup     │
                                             │    by (trace_id,      │
                                             │        span_id,       │
                                             │        sha256(span))  │
                                             └──────────┬───────────┘
                                                        │
                                                        ▼
                                             ┌──────────────────────┐
                                             │ SQLite                │
                                             │   raw_event           │
                                             │   observed_event      │
                                             │   (graph rebuilt      │
                                             │    per session)       │
                                             └──────────────────────┘
```

The receiver is a thin HTTP adapter; all storage logic lives in `src/ingest/otel.rs` and reuses the existing `repo_raw` / `repo_observed` interfaces. Graph rebuild happens after each request batch finishes (per affected session).

---

## 4. API Surface

### `POST /otel/v1/traces`

**Request**

| Header             | Required | Notes |
|--------------------|----------|-------|
| `Content-Type`     | yes      | `application/json` |
| `Content-Encoding` | optional | `gzip` accepted. No other encodings. |
| body               | yes      | OTLP/JSON `ExportTraceServiceRequest` (≤ 4 MB after decompression) |

**Body shape (subset we parse)**

```jsonc
{
  "resourceSpans": [
    {
      "resource": {
        "attributes": [
          {"key": "service.name", "value": {"stringValue": "claude-code"}},
          {"key": "session.id",   "value": {"stringValue": "abc123"}}
        ]
      },
      "scopeSpans": [
        {
          "scope": {"name": "witmcc.test", "version": "0.1.0"},
          "spans": [
            {
              "traceId": "5b8aa5a2d2c872e8321cf37308d69df2",  // hex
              "spanId":  "051581bf3cb55c13",                  // hex
              "parentSpanId": null,
              "name":  "tool.invoke",
              "kind":  "SPAN_KIND_CLIENT",
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

**Response**

`200 OK` with envelope:

```json
{
  "meta": {"schema_version": "0.2", "generated_at": "<rfc3339>"},
  "data": {
    "accepted_spans": 17,
    "rejected_spans": 0,
    "sessions_touched": ["abc123", "def456"]
  }
}
```

**Error codes**

- `400 Bad Request` — JSON parse error, or body exceeds 4 MB after decompression.
- `413 Payload Too Large` — body exceeds limit before decompression.
- `415 Unsupported Media Type` — non-JSON content type, or encoding other than gzip.
- `500` — DB write failure (existing pattern).

Spans missing `traceId` or `spanId` are individually rejected (counted in `rejected_spans`) but do not fail the request.

### Existing endpoints (unchanged shape, expanded data)

- `GET /v1/sessions` — OTel-origin sessions appear when a span carries `session.id`.
- `GET /v1/sessions/:id` — events list now may include `otel_span` records.
- `GET /v1/sessions/:id/graph` — nodes list now may include `node_kind="otel_span"`.
- `GET /v1/events/:event_id/raw` — returns the original span JSON under `record`.

---

## 5. Data Model Changes

### 5.1 `ObservedEvent` (Rust struct, `src/model/observed.rs`)

Add fields (mirroring the DB columns that already exist since slice-1):

```rust
pub struct ObservedEvent {
    // ... existing fields ...
    pub trace_id:       Option<String>,
    pub span_id:        Option<String>,
    pub parent_span_id: Option<String>,
    pub latency_ms:     Option<i64>,
    pub telemetry:      Option<TelemetryFacet>,
}

pub struct TelemetryFacet {
    pub span_name:        String,
    pub span_kind:        Option<String>,   // "client" | "server" | ...
    pub status_code:      Option<String>,   // "ok" | "error" | "unset"
    pub status_message:   Option<String>,
    pub start_unix_nano:  i64,
    pub end_unix_nano:    i64,
    pub attributes:       serde_json::Value, // flat object: { "tool.name": "Bash", ... }
    pub resource:         serde_json::Value, // flat object from resource attributes
    pub scope_name:       Option<String>,
    pub scope_version:    Option<String>,
}
```

`telemetry` is `None` for transcript events and `Some` for OTel spans. The facet is stored inside the existing `payload` TEXT column as JSON; no DDL change required for the facet itself.

### 5.2 `EventKind`

Add variant `OtelSpan`, serialised as `"otel_span"`.

### 5.3 `Actor`

Reuse existing variants. OTel spans use `Actor::Tool` (most spans wrap tool/model calls) when `span_kind` is `client`, otherwise `Actor::System`. Pluggable later if needed.

### 5.4 Schema version

Bump `SCHEMA_VERSION` from `0.1` to `0.2`.

- New events emitted by ingest (transcript or OTel) carry `"0.2"`.
- Existing rows stay at `"0.1"` and remain readable; the new optional `telemetry` field is absent in their payload.

### 5.5 Migration

Existing `observed_event` columns already include `trace_id`, `span_id`, `parent_span_id`, `latency_ms`, `redaction_state` (see `migrations/20260519120000_0001_init.sql`). The only new index needed is on `trace_id` for span lookup:

```sql
-- migrations/20260520120000_0002_telemetry.sql
CREATE INDEX IF NOT EXISTS idx_obs_trace_span
  ON observed_event(trace_id, span_id)
  WHERE trace_id IS NOT NULL;
```

No data migration; old rows just have NULL `trace_id`.

### 5.6 Idempotency / `event_id` derivation

`event_id` for an OTel span is derived from `trace_id || ":" || span_id || ":" || sha256(canonical_span_json)`. Same `(trace_id, span_id)` re-ingested with identical payload → identical `event_id` → unique-insert no-op. Same `(trace_id, span_id)` with **different** payload → new `event_id` (new row); the unique constraint on `raw_event(source_uri, source_line_no, payload_sha256)` is bypassed because OTel uses synthetic `source_uri`. We accept the duplicate for now; deduplication via `(trace_id, span_id)` upsert is a follow-up.

### 5.7 RawEvent for OTel

| Column              | Value                                                                  |
|---------------------|------------------------------------------------------------------------|
| `source_type`       | `"otel"`                                                               |
| `source_uri`        | `otel://traces/<trace_id>/spans/<span_id>` (deterministic, per-span)   |
| `source_line_no`    | `0` (positional indexing has no meaning for HTTP batches)              |
| `source_byte_offset`| `0`                                                                    |
| `payload_sha256`    | sha256 of canonical span JSON                                          |
| `payload`           | canonical span JSON bytes                                              |
| `parse_error`       | `null` on success, error string on rejected spans                      |

The per-span `source_uri` combined with `payload_sha256` makes idempotency order-independent: re-POSTing the same span in any batch produces the same `(source_uri, source_line_no, payload_sha256)` triple and the `UNIQUE` constraint deduplicates.

---

## 6. Graph Mapping

### 6.1 Node materialisation (`src/graph/build.rs`)

Add a case to `compute`:

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

The `(trace_id, span_id)` pair drives the merge_keys. If a future slice produces a transcript event with the same pair, both events deduplicate onto the same node (existing dedup logic in `compute` handles that via `node_index_by_id`).

### 6.2 Edges

No new edge kind in slice-3. Spans are isolated nodes. (`span_parent` edge kind from `parent_span_id` is a follow-up — orthogonal to data path validation.)

### 6.3 Session selection

A span is included in a session's graph iff its `session_id` (extracted from `session.id` resource or span attribute) matches. Spans without `session.id` land in `observed_event` with empty `session_id` and are not surfaced through `/v1/sessions` (same convention as `file-history-snapshot`).

---

## 7. UI Changes (`webui/`)

### 7.1 Lane mapping

`webui/src/api/laneMapping.ts` already declares `OTel` lane. Add:

```ts
case 'otel_span': return 'OTel';
```

### 7.2 Timeline

`webui/src/components/Timeline.tsx` already renders all six declared lanes. No structural change. A small visual differentiator (e.g. dashed border for OTel markers) keeps them recognisable but is optional.

### 7.3 SourcePanel

`webui/src/components/SourcePanel.tsx` currently renders raw `record` as JSON. When `record_type === 'otel_span'`:

1. Show a small `Attributes` summary (top 10 key/value pairs from `telemetry.attributes`).
2. Below it, the full original span JSON in the existing `JsonView`.

### 7.4 SessionDetailPage MetaStrip

No change to layout; the existing `event_count` and `by_kind` summary already covers the new `otel_span` kind.

---

## 8. Error Handling & Edge Cases

| Case | Behaviour |
|------|-----------|
| Span missing `traceId` or `spanId` | Span rejected; counted in `rejected_spans`. Other spans in the same request still ingested. |
| `traceId` not 32 hex chars | Span rejected (`parse_error` recorded in `raw_event` if we still persist it; v1 just drops). |
| `spanId` not 16 hex chars | Same as above. |
| Span without `session.id` | Stored with empty `session_id`; not in `/v1/sessions`. |
| Span with `endTimeUnixNano < startTimeUnixNano` | Stored; `latency_ms` clamped to `0`. |
| Duplicate `(trace_id, span_id)` same payload | No-op via `payload_sha256` dedup. |
| Duplicate `(trace_id, span_id)` different payload | Both rows kept (separate `event_id`). Last-writer-wins on graph node via existing dedup. |
| Gzip body that decompresses past 4 MB | `400 Bad Request`. |
| Non-UTF-8 JSON | `400 Bad Request`. |

---

## 9. Test Strategy

### 9.1 Fixtures

`tests/fixtures/otel/` (new):

- `single_span.json` — one root span with `session.id`.
- `parent_child.json` — parent and child span sharing trace_id.
- `multi_resource.json` — two `resourceSpans` entries, two distinct sessions.
- `missing_session_id.json` — valid span, no `session.id`.
- `malformed_traceid.json` — span with `traceId="not-hex"`.

### 9.2 Unit tests

`src/ingest/otel.rs` (alongside module):

- Parses OTLP/JSON into `Vec<SpanRecord>`.
- Rejects malformed spans into a separate vec, never panics.
- Canonical JSON for sha256 is byte-stable across reorderings of attributes (sort keys).

### 9.3 Integration tests

`tests/otel_ingest.rs` (new):

- POST `single_span.json` → 200, `accepted_spans: 1`. Verify `/v1/sessions` lists the session and `/v1/sessions/:id` includes the span event.
- POST same body twice → second call is no-op (dedup via `payload_sha256`).
- POST `missing_session_id.json` → 200, `accepted_spans: 1`, session not in `/v1/sessions`.
- POST `malformed_traceid.json` → 200, `accepted_spans: 0, rejected_spans: 1`.
- POST gzip-encoded `parent_child.json` → 200, both spans ingested.
- Verify `/v1/sessions/:id/graph` returns two `otel_span` nodes.

### 9.4 UI tests (`webui/`)

- `laneMapping.test.ts`: `laneForNodeKind('otel_span') === 'OTel'`.
- `SourcePanel.test.tsx`: rendering `record_type='otel_span'` shows Attributes section and span JSON.
- `Timeline.test.tsx`: regression — six lanes still visible; otel_span lane carries a marker when the graph contains one.

### 9.5 Acceptance smoke

- Start `witmcc serve`, post the single-span fixture with `curl --data-binary @single_span.json`, open the UI, see the session in the list, click in, see the span on the OTel lane, click it, see attributes.

---

## 10. Routing & Wiring

`src/api/mod.rs`:

```rust
let router = Router::new()
    // ... existing routes ...
    .route("/otel/v1/traces", post(otel::ingest_traces))
    // ... layers ...
```

`host_allowlist` middleware applies. `loopback` bind applies. No new auth surface.

`src/api/otel.rs` handler:

```rust
pub async fn ingest_traces(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Envelope<IngestResponse>>, (StatusCode, Json<Value>)> {
    let json = decode_body(&headers, body)?;       // gzip + size guard
    let parsed = parse_otlp_json(&json)?;          // returns (accepted, rejected)
    let result = ingest::otel::store(&pool, parsed).await?;
    Ok(Json(Envelope::wrap(result)))
}
```

---

## 11. Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| OTLP/JSON evolves; we silently drop attributes we don't recognise | Source-preserving: original span JSON kept in `raw_event.payload`. Parser only extracts known keys. |
| Spans with sensitive attribute values (API keys etc.) get persisted | Redaction is M7. Document the gap in implementation-notes and warn in README. |
| Large spans (>4 MB single span) get rejected | Acceptable for slice-3; raise the limit if real workloads need more. |
| OTel SDKs default to protobuf, not JSON | Document required exporter env: `OTEL_EXPORTER_OTLP_TRACES_PROTOCOL=http/json`. |
| Trace-id collision across unrelated sessions | merge_keys include `session_id` + `trace_id` + `span_id`. Different sessions ⇒ different node_id. |
| `(trace_id, span_id)` duplicate with different payload | Both rows kept; graph dedup wins. Note as known gap; deduplication upsert is a follow-up. |

---

## 12. Migration Path

1. `cargo install` consumers run new `witmcc serve` — auto-migration applies `0002_telemetry.sql` (index only).
2. Existing transcript rows continue working at `schema_version=0.1`.
3. New events (transcript or OTel) ingest at `0.2`.
4. UI is forward-compatible because it ignores unknown fields.

---

## 13. Build / Dev Workflow

No new build steps. The receiver is in the same binary; the UI build path remains `just webui-build && cargo build`.

Local OTel emit example (for manual smoke testing):

```bash
curl -X POST http://127.0.0.1:7878/otel/v1/traces \
  -H 'Content-Type: application/json' \
  --data-binary @tests/fixtures/otel/single_span.json
```

---

## 14. Acceptance Criteria for Slice-3

1. `POST /otel/v1/traces` accepts a valid OTLP/JSON traces request and returns `200` with `accepted_spans > 0`.
2. The session attached to the span appears in `GET /v1/sessions` after ingest.
3. The span appears in `GET /v1/sessions/:id` with `kind="otel_span"` and a populated `telemetry` facet inside `payload`.
4. The graph for that session contains an `otel_span` node with `merge_keys.trace_id` and `merge_keys.span_id` set.
5. Timeline UI shows a marker on the OTel lane for that span; clicking it opens the SourcePanel with attribute summary + raw span JSON.
6. All existing cargo tests (currently 31) and webui vitest tests (currently 19) keep passing.
7. New OTel integration tests (≥5) added and passing.

---

## 15. Open Decisions (resolved for this slice)

| Decision | Choice | Rationale |
|---|---|---|
| Wire format | JSON only | Avoid `prost`/`opentelemetry-proto` dep; local single user can configure SDK. |
| Signals | Traces only | Metrics are time-series (different model); logs add scope without insight gain in slice-3. |
| Receiver path | `/otel/v1/traces` | Distinct from `/v1/...` Pull API; matches `OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:7878/otel`. |
| Auth | loopback + host allowlist only | Token auth is M7. |
| trace_id 1급 | Yes — promoted to top-level `correlation_keys` | CLAUDE.md principle. Schema version bump 0.1 → 0.2. |
| Span merge with transcript | Not in slice-3 | Transcript producer doesn't emit trace_id yet. merge_keys laid down for free future merge. |
| Span parent edges | Not in slice-3 | Orthogonal; data path validation first. |

---

## 16. Follow-up slices unblocked by this work

- **Slice candidate: transcript trace producer** — wrap Claude Code launches with an OTel-emitting hook so transcript events carry `trace_id`. Activates merge automatically.
- **Slice candidate: span parent edges** — derive `span_parent` edge kind from `parent_span_id`.
- **Slice candidate: OTel metrics** — add `/otel/v1/metrics`, decide whether metrics become `MetricSample` records or aggregate elsewhere.
- **Slice candidate: protobuf encoding** — once a real SDK in our flow needs it.
- **Slice candidate: token auth** — required before redaction / export bundles ship.
