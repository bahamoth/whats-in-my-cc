# Slice-6 Design — OTel Metrics + Logs Receiver, Doctor, Real-payload Anchoring

**Date:** 2026-05-20
**Branch:** `slice6-otel-metrics-logs` (based on `main` post slice-5)
**Goal:** Make every Claude Code OTel signal — **metrics, logs, traces** — land in witmcc with a single `claude` invocation against the same loopback endpoint. Lock the data model on **real** Claude Code payloads rather than hand-written fixtures, and ship `witmcc doctor` so users can see which collectors are actually wired.

---

## 1. Motivation

Slice-3 shipped `/otel/v1/traces` and proved the trace path on hand-written fixtures. Two gaps surfaced when we re-read the official Claude Code monitoring docs (`https://code.claude.com/docs/en/monitoring-usage`):

1. **Claude Code's primary OTel signals are metrics + logs, not traces.** Tool-call counts, token usage, cost, prompt/tool events — all of these ship via `OTEL_METRICS_EXPORTER` / `OTEL_LOGS_EXPORTER`. Traces are **beta** and require an extra `CLAUDE_CODE_ENHANCED_TELEMETRY_BETA=1` flag.
2. **No fixture in the repo came from a real Claude Code emission.** Every `tests/fixtures/otel/*.json` was synthesised by hand. Schema changes downstream (Findings engine, AC-2 linkage) cannot rely on those as ground truth.

Slice-6 closes both gaps. It does **not** invent a separate capture tool — under CLAUDE.md's *source-preserving* principle, raw bodies are already a permanent receiver responsibility (`raw_event` table). The slice extends that to two new signals, then anchors normalisation on what `claude` actually emits.

AC-2 (≥90% trace linkage) still won't be met by slice-6 alone — that needs a transcript producer emitting `trace_id`. But after this slice every signal a user can enable is **received and persisted**, and the operator can prove it via `witmcc doctor`.

---

## 2. Scope

### In Scope

- **Two-stage receiver** for OTLP/JSON metrics + logs:
  - **Stage 1** — `POST /otel/v1/metrics`, `POST /otel/v1/logs` accept body, gzip-decode, persist to `raw_event` with new `source_type` values, **no normalisation, no graph node**. Goal: a real `claude` session immediately rounds-trips into the DB.
  - **Stage 2** — `MetricSample` / `LogRecord` records, `EventKind::MetricSample` / `EventKind::LogRecord`, graph nodes, lane wiring. Driven by the real fixtures captured between the two stages.
- **Real fixture anchoring**: after Stage 1 lands, user runs one `claude` session against witmcc; one payload per signal is frozen under `tests/fixtures/otel/real/{signal}_v{NN}.json` and becomes the basis for Stage 2 parser/test work.
- **`witmcc doctor`** — read-only diagnostic CLI that reports environment variables, hook settings.json presence, server reachability, and last-ingest timestamps per source. **No file mutation.**
- `webui` updates: a Metrics and a Logs sub-lane (or shared `OTel` lane with sub-marker) wired through `laneMapping.ts`.
- Implementation-notes section for slice-6.

### Out of Scope (deferred)

- OTLP/**protobuf** binary encoding (`http/json` only — same constraint as slice-3).
- gRPC OTLP transport.
- Transcript ↔ otel-logs **automatic dedup** — log records carry `trace_id`/`span_id`; transcript today does not. Cross-source dedup policy is stated in §11 but implementation is a later slice.
- Aggregating metrics into rollups. Each `NumberDataPoint` / `HistogramDataPoint` becomes one `MetricSample` row. Time-series UI is a later slice.
- Configuration write of any kind to `~/.claude/settings.json`, `~/.zshrc`, etc. CLAUDE.md non-goal stays inviolate — `witmcc doctor` only **reads and reports**.
- Token authentication, redaction, retention cleanup (M7).
- Findings engine (M5).
- Live-tail of transcript JSONL files (separate slice).

---

## 3. Architecture

```
                   ┌─────────────────────────────────────┐
                   │ Claude Code  (CLAUDE_CODE_ENABLE_   │
                   │   TELEMETRY=1 + http/json + OTLP    │
                   │   endpoint = http://127.0.0.1:7878) │
                   └────────────────┬────────────────────┘
                                    │ POST /otel/v1/{metrics,logs,traces}
                                    │ Content-Encoding: gzip (typical)
                                    ▼
        ┌──────────────────────────────────────────────────────┐
        │ src/api/otel.rs                                       │
        │  - existing: ingest_traces                            │
        │  - new:      ingest_metrics, ingest_logs              │
        │  - shared:   gzip decode, body limit, content-type    │
        └────────┬────────────────────┬─────────────────────────┘
                 │                     │
                 │ Stage 1             │ Stage 2 (after real fixtures land)
                 ▼                     ▼
        ┌─────────────────┐   ┌──────────────────────────────┐
        │ raw_event       │   │ src/ingest/otel_metrics.rs   │
        │  source_type =  │   │ src/ingest/otel_logs.rs      │
        │   "otel-metrics"│   │  metric/log → ObservedEvent   │
        │   "otel-logs"   │   │  metric/log → graph node      │
        │  parser_version │   └──────────────────────────────┘
        │  = "otel-m@s6"  │                  │
        │  = "otel-l@s6"  │                  ▼
        └─────────────────┘   ┌──────────────────────────────┐
                              │ observed_event +              │
                              │ graph rebuild (per session)   │
                              └──────────────────────────────┘
```

Stage 1 is a strict subset of Stage 2 — once Stage 2 lands, the same Stage-1 raw insertion is preserved (source-preserving principle) and the normaliser reads from `raw_event` to populate `observed_event`. There is no separate "capture mode" toggle: Stage 1 is permanent receiver behaviour.

---

## 4. API Surface

### 4.1 `POST /otel/v1/metrics` (new)

| Header             | Required | Notes |
|--------------------|----------|-------|
| `Content-Type`     | yes      | `application/json` |
| `Content-Encoding` | optional | `gzip` accepted |
| body               | yes      | OTLP/JSON `ExportMetricsServiceRequest` (≤ 4 MB after decompression) |

Body shape (excerpt — full OTLP spec applies):

```jsonc
{
  "resourceMetrics": [
    {
      "resource": { "attributes": [ {"key": "service.name", "value": {"stringValue":"claude-code"}}, ... ] },
      "scopeMetrics": [
        {
          "scope": { "name": "com.anthropic.claude_code", "version": "..." },
          "metrics": [
            {
              "name": "claude_code.session.count",
              "unit": "1",
              "sum": {
                "isMonotonic": true,
                "aggregationTemporality": 1,
                "dataPoints": [
                  {"timeUnixNano":"...", "asInt":"1", "attributes":[ ... ]}
                ]
              }
            },
            // gauge, histogram, exponentialHistogram, summary variants
          ]
        }
      ]
    }
  ]
}
```

**Stage 1 response** (`200 OK`):

```json
{
  "meta": {"schema_version":"0.5","generated_at":"<rfc3339>"},
  "data": {"accepted_resource_metrics": 1, "stored_raw_rows": 1}
}
```

**Stage 2 response** adds:

```json
{
  "data": {
    "accepted_resource_metrics": 1,
    "stored_raw_rows": 1,
    "accepted_data_points": 12,
    "rejected_data_points": 0,
    "sessions_touched": ["abc123"]
  }
}
```

### 4.2 `POST /otel/v1/logs` (new)

Symmetric to metrics. Body is OTLP/JSON `ExportLogsServiceRequest`. Stage 2 response surfaces `accepted_log_records` and `sessions_touched`.

### 4.3 Reuse — `POST /otel/v1/traces`

No shape change. The existing handler stays. `witmcc doctor` reports it alongside the new endpoints.

### 4.4 Errors

Identical envelope to slice-3:

- `400` — JSON parse / body limit / non-UTF-8.
- `413` — body before decompression > limit.
- `415` — unsupported content type / encoding.
- `500` — DB write failure.

Individual data points / log records with broken shape are counted in `rejected_*` but do not fail the request (Stage 2 only — Stage 1 is "all or nothing" at the resource-message level).

---

## 5. Data Model

### 5.1 Stage 1 — `raw_event` only

| Column              | Metrics                                            | Logs                                            |
|---------------------|----------------------------------------------------|-------------------------------------------------|
| `source_type`       | `"otel-metrics"`                                   | `"otel-logs"`                                   |
| `source_uri`        | `otel-metrics://post/<sha256(body)[..16]>`         | `otel-logs://post/<sha256(body)[..16]>`         |
| `source_line_no`    | `0`                                                | `0`                                             |
| `payload_sha256`    | sha256 of canonical request JSON                   | same                                            |
| `payload`           | canonical JSON bytes of full request               | same                                            |
| `parser_version`    | `"otel-metrics@0.5"`                               | `"otel-logs@0.5"`                               |

Stage 1 deliberately stores **the whole `ExportXxxServiceRequest` as one row**, not per-data-point. The minimum receiver does not look inside; per-point splitting is Stage 2's job.

### 5.2 Stage 2 — `ObservedEvent` extensions

```rust
pub enum EventKind {
    // ... existing ...
    MetricSample,
    LogRecord,
}

pub struct MetricFacet {
    pub instrument_name: String,
    pub instrument_kind: String,   // "sum" | "gauge" | "histogram" | "exponentialHistogram" | "summary"
    pub unit:            Option<String>,
    pub temporality:     Option<String>, // "cumulative" | "delta"
    pub is_monotonic:    Option<bool>,
    pub value_int:       Option<i64>,
    pub value_float:     Option<f64>,
    pub histogram:       Option<serde_json::Value>, // raw datapoint for histograms
    pub attributes:      serde_json::Value,         // flat object
    pub resource:        serde_json::Value,
    pub scope_name:      Option<String>,
    pub scope_version:   Option<String>,
    pub time_unix_nano:  i64,
    pub start_time_unix_nano: Option<i64>,
}

pub struct LogFacet {
    pub severity_number: Option<i32>,
    pub severity_text:   Option<String>,
    pub body:            serde_json::Value, // OTLP "body" is AnyValue; keep raw
    pub event_name:      Option<String>,    // attribute "event.name"
    pub attributes:      serde_json::Value,
    pub resource:        serde_json::Value,
    pub scope_name:      Option<String>,
    pub scope_version:   Option<String>,
    pub time_unix_nano:  i64,
    pub observed_time_unix_nano: Option<i64>,
}
```

`ObservedEvent` already has `trace_id` / `span_id` / `latency_ms` columns from slice-1. `LogRecord` populates those when the log carries `traceId`/`spanId`. `MetricSample` typically does not.

The facet ends up inside `observed_event.payload` (the existing TEXT-JSON column). **No DDL** required for the facet itself. SQL indexes added below.

### 5.3 Migrations

`migrations/20260520180000_0004_otel_metrics_logs.sql`:

```sql
CREATE INDEX IF NOT EXISTS idx_obs_metric_instrument
  ON observed_event(json_extract(payload, '$.instrument_name'))
  WHERE kind = 'metric_sample';

CREATE INDEX IF NOT EXISTS idx_obs_log_event_name
  ON observed_event(json_extract(payload, '$.event_name'))
  WHERE kind = 'log_record';

CREATE INDEX IF NOT EXISTS idx_raw_source_type_session
  ON raw_event(source_type, session_id);
```

No data backfill — Stage 2 reparses any rows Stage 1 already collected by streaming through `raw_event WHERE source_type IN ('otel-metrics','otel-logs') AND parser_version NOT LIKE '…@normalized'` (decision: keep the reparse loop idempotent so re-running Stage 2 doesn't double-insert; rely on existing `(source_uri, source_line_no, payload_sha256)` unique).

### 5.4 Schema version

Bump `SCHEMA_VERSION` from `0.4` to `0.5`.

- Stage 1 alone is sufficient to ship `0.5` because it adds new `source_type` values that consumers need to know about.
- Stage 2 keeps `0.5` (additive change in `EventKind`).

### 5.5 `event_id` derivation

| Record         | `event_id` formula                                                   |
|----------------|----------------------------------------------------------------------|
| MetricSample   | `metric:<resource_sha8>:<instrument>:<time_unix_nano>:<attr_sha8>`   |
| LogRecord      | `log:<resource_sha8>:<time_unix_nano>:<body_sha8>`                   |

Both are deterministic: replaying the same payload yields the same `event_id` → idempotent. `attr_sha8` = first 8 hex of sha256 of canonical-JSON attribute object (sorted keys).

### 5.6 `session_id` extraction

Both metrics and logs ship `session.id` as a resource attribute when `CLAUDE_CODE_ENABLE_TELEMETRY=1` is set (verified from official docs; concrete attribute name confirmed during real-payload capture). If absent, the record stores with empty `session_id` and is invisible in `/v1/sessions` (same convention as slice-3).

---

## 6. Graph Mapping

### 6.1 Node kinds

```rust
EventKind::MetricSample => ("metric_sample", json!({
    "session_id":      session_id,
    "instrument_name": e.metric.instrument_name,
    "time_unix_nano":  e.metric.time_unix_nano,
})),
EventKind::LogRecord => ("log_record", json!({
    "session_id":     session_id,
    "time_unix_nano": e.log.time_unix_nano,
    "trace_id":       e.trace_id,   // optional, helps later linkage
    "span_id":        e.span_id,
})),
```

### 6.2 Edges

None in slice-6. Same posture as slice-3 / slice-5. Edge kinds (`metric_attribution`, `span_log`) are follow-ups.

### 6.3 Lane mapping (`webui/src/api/laneMapping.ts`)

The OTel lane already exists (slice-3). We add two more **node-kind → same lane** mappings rather than introducing new lanes:

```ts
case 'metric_sample':
case 'log_record':
case 'otel_span':
  return 'OTel';
```

The Timeline marker for `metric_sample` uses a small diamond shape, `log_record` uses a triangle, `otel_span` keeps its existing dashed-border circle — that's the only visual differentiation. Sub-lanes would mean 10 lanes; we already have 8 and the screen is tight.

### 6.4 SourcePanel

For `metric_sample`: instrument name + value + first 10 attributes. For `log_record`: severity badge + event name + first 10 attributes + body JSON.

---

## 7. `witmcc doctor` Command

### 7.1 CLI

```
witmcc doctor [--json] [--server http://127.0.0.1:7878]
```

- Default output: human-readable colourised report (~30 lines).
- `--json` emits structured output for tooling.
- `--server` lets the user point at a non-default port; default `WITMCC_SERVER` env or `http://127.0.0.1:7878`.

### 7.2 Checks (read-only)

1. **Environment variables** — read `CLAUDE_CODE_ENABLE_TELEMETRY`, `CLAUDE_CODE_ENHANCED_TELEMETRY_BETA`, `OTEL_METRICS_EXPORTER`, `OTEL_LOGS_EXPORTER`, `OTEL_TRACES_EXPORTER`, `OTEL_EXPORTER_OTLP_PROTOCOL`, `OTEL_EXPORTER_OTLP_ENDPOINT`. Report value + status (good / wrong-value / unset).
2. **Hook settings** — read `~/.claude/settings.json` (best effort; do not error if missing). Report whether at least one of the nine recognised hook event names is wired to a command that POSTs to `…/hooks/v1/events`. Heuristic: substring `"hooks/v1/events"` in any command string.
3. **Server reachability** — `GET /v1/health` against `--server`. Report build_sha + status.
4. **Last ingest per source** — `GET /v1/health/sources` (new endpoint, §7.4) returns per-source `last_ingested_at` for `transcript`, `otel-traces`, `otel-metrics`, `otel-logs`, `hook`, `file`, `git`. Output table:

   ```
   Source           Last ingest        Status
   transcript       2026-05-20 16:21Z  ✓ recent
   otel-traces      —                  ✗ no data (needs CLAUDE_CODE_ENHANCED_TELEMETRY_BETA=1)
   otel-metrics     2026-05-20 16:23Z  ✓ recent
   otel-logs        2026-05-20 16:23Z  ✓ recent
   hook             —                  ✗ no data (no hook entry in ~/.claude/settings.json)
   file             2026-05-20 16:22Z  ✓ recent
   git              2026-05-20 16:20Z  ✓ recent
   ```

5. **Recommendation block** — for each missing/misconfigured item, print a copy-pastable `export …` or jsonc snippet. Always inert; never modifies any file. Wording references CLAUDE.md non-goal explicitly.

### 7.3 Exit code

`0` if server reachable and at least transcript + (any of otel-metrics, otel-logs, hook) is recent. `1` otherwise. `--json` still exits `0` for tooling consumers that read the JSON.

### 7.4 New endpoint `GET /v1/health/sources`

```json
{
  "meta": {"schema_version":"0.5","generated_at":"<rfc3339>"},
  "data": {
    "sources": {
      "transcript":   { "last_ingested_at": "...", "row_count_24h": 132 },
      "otel-traces":  { "last_ingested_at": null,  "row_count_24h": 0 },
      "otel-metrics": { "last_ingested_at": "...", "row_count_24h": 412 },
      "otel-logs":    { "last_ingested_at": "...", "row_count_24h": 88 },
      "hook":         { "last_ingested_at": null,  "row_count_24h": 0 },
      "file":         { "last_ingested_at": "...", "row_count_24h": 21 },
      "git":          { "last_ingested_at": "...", "row_count_24h": 3 }
    }
  }
}
```

Implementation: one query `SELECT source_type, MAX(ingested_at), COUNT(*) FROM raw_event GROUP BY source_type` plus a fixed source-type taxonomy so missing rows still appear.

---

## 8. Real-payload Capture Procedure

This is the human step between Stage 1 and Stage 2. Documented in README + implementation-notes; not automated. Procedure:

```bash
# Terminal A
cargo run -- serve --auto-migrate --watch /path/to/some/repo &

# Terminal B
export CLAUDE_CODE_ENABLE_TELEMETRY=1
export CLAUDE_CODE_ENHANCED_TELEMETRY_BETA=1
export OTEL_METRICS_EXPORTER=otlp
export OTEL_LOGS_EXPORTER=otlp
export OTEL_TRACES_EXPORTER=otlp
export OTEL_EXPORTER_OTLP_PROTOCOL=http/json
export OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:7878
export OTEL_METRIC_EXPORT_INTERVAL=5000
export OTEL_LOGS_EXPORT_INTERVAL=2000

claude   # do anything that uses Bash, Edit, etc. then exit

# Extract one fixture per signal
sqlite3 .witmcc.sqlite "
  SELECT payload FROM raw_event
   WHERE source_type = 'otel-metrics'
   ORDER BY ingested_at DESC LIMIT 1;
" | python3 -m json.tool > tests/fixtures/otel/real/metrics_v01.json
# repeat for otel-logs, otel-traces
```

The real fixtures replace the synthetic `single_span.json`-style ones as the **canonical regression set**. The synthetic fixtures stay for edge cases (malformed, missing session.id) where realistic capture is awkward.

---

## 9. UI

- `webui/src/api/laneMapping.ts` — add `metric_sample`, `log_record` → `OTel`.
- `webui/src/components/Timeline.tsx` — diamond / triangle marker shapes.
- `webui/src/components/SourcePanel.tsx` — sub-renderers per `record_type`.
- No new pages.

---

## 10. Error Handling & Edge Cases

| Case | Behaviour |
|------|-----------|
| Empty `resourceMetrics` / `resourceLogs` | Stage 1: 200, `stored_raw_rows=0`. Stage 2: same, `accepted_*=0`. Not an error. |
| Metric data point with neither `asInt` nor `asDouble` (raw `value` JSON) | Stage 2: stored with both fields null and `histogram=<raw datapoint>` so source is preserved. |
| Log body that's an object, array, or non-string scalar | Stored as-is in `LogFacet.body` (OTLP `AnyValue`). |
| Metric resource has no `session.id` | Same convention: empty `session_id`, not in `/v1/sessions`, still queryable by event id. |
| Exponential histograms, summaries | Stored opaquely under `histogram` field; no UI affordance in slice-6 beyond JsonView. |
| Body > 4 MB after decompression | `400`. Same as slice-3. |
| Protocol = grpc / protobuf | Receiver returns `415` if `Content-Type` is `application/x-protobuf`. Doctor recommends `OTEL_EXPORTER_OTLP_PROTOCOL=http/json`. |
| Same payload re-POSTed | Idempotent via `payload_sha256` unique. |

---

## 11. Cross-source Dedup Policy (stated, deferred)

`otel-logs` and `transcript` can carry the same event (e.g., `user_prompt`). Slice-6 ships **separate nodes**; cross-source dedup is a later slice with its own design memo. Rationale identical to DEV-S4-05 (hook ↔ transcript-internal hook): merge keys differ, time-window heuristics need their own validation.

`metric_sample` does not duplicate transcript (counts are aggregate). No dedup work needed there.

---

## 12. Test Strategy

### 12.1 Fixtures

- **Stage 1**: `tests/fixtures/otel/metrics/minimal.json`, `logs/minimal.json` — smallest valid OTLP/JSON bodies (handcrafted from official OTLP spec). Used only to assert Stage-1 raw insertion works.
- **Stage 2 (after capture)**: `tests/fixtures/otel/real/metrics_v01.json`, `logs_v01.json`, `traces_v01.json` — from real `claude` run, **lightly redacted** by hand if any local path leaks (note redactions in fixture comments).

### 12.2 Unit tests

- `src/ingest/otel_metrics.rs`: parses each instrument kind (sum, gauge, histogram, exponentialHistogram, summary) from the real fixture; canonical JSON for sha is byte-stable across attribute reorderings.
- `src/ingest/otel_logs.rs`: extracts severity, body, attributes; preserves `trace_id`/`span_id` when present.

### 12.3 Integration tests

`tests/otel_metrics_ingest.rs`, `tests/otel_logs_ingest.rs`:

- POST minimal fixture → 200 + raw row.
- POST real fixture → 200 + raw row; Stage 2 reparse yields the expected `accepted_data_points` / `accepted_log_records`.
- Re-POST identical body → dedup, no second row.
- Gzip body → 200, raw row stored decompressed.
- Stage 2: `/v1/sessions/<id>/graph` shows `metric_sample` / `log_record` nodes.

`tests/health_sources.rs`:

- After ingest, `GET /v1/health/sources` shows non-null `last_ingested_at` for the corresponding source type.

`tests/doctor.rs`:

- Doctor with no env set: prints unset; exit 1.
- Doctor with mock server returning sources: prints the table; exit 0 when transcript + one other has rows.

### 12.4 UI tests

- `laneMapping.test.ts`: `laneForNodeKind('metric_sample') === 'OTel'`, `'log_record' === 'OTel'`.
- `SourcePanel.test.tsx`: renders metric instrument header for `metric_sample`, severity badge for `log_record`.
- `Timeline.test.tsx`: regression — 8 lanes still visible; OTel lane carries metric + log markers when graph contains them.

### 12.5 Acceptance smoke

After Stage 2 lands, the README OTel section is updated with the full env block. Running it must produce non-empty rows for all three OTel signals within 60s of `claude` exit.

---

## 13. Routing & Wiring

`src/api/mod.rs`:

```rust
.route("/otel/v1/metrics", post(otel::ingest_metrics))
.route("/otel/v1/logs",    post(otel::ingest_logs))
.route("/v1/health/sources", get(routes::health_sources))
```

`host_allowlist` + loopback bind apply identically.

`src/cli.rs`: new subcommand `Doctor`:

```rust
Doctor {
    #[arg(long)] json: bool,
    #[arg(long, default_value = "http://127.0.0.1:7878")] server: String,
}
```

---

## 14. Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Real Claude Code attribute names differ from what spec implies | Capture step is between Stage 1 and Stage 2; Stage 2 code reads attribute names from the real fixture, no guessing. |
| OTLP/JSON evolves; new fields silently ignored | Source-preserving: full request JSON preserved in `raw_event.payload`. Stage 2 parser only extracts known keys. |
| Log records carry secrets (user prompts, tool output) | M7 (redaction). Slice-6 README adds a prominent warning; doctor flags `OTEL_LOG_USER_PROMPTS=1` as "sensitive ON". |
| Metric cardinality explodes (every sample = row) | Bound by Claude Code's own export interval (default 60s). At 10 instruments × 12 sessions/day, ~17k rows/day. Acceptable for local-first MVP. Add retention in M7. |
| Doctor false-positive ("no data" when ingest in flight) | Time window is "any time" for `last_ingested_at`. Recent threshold (5 min) only colours output. |
| User confuses `witmcc serve` with `claude` | Doctor's recommendation block calls out the two-process model explicitly. |
| AC-2 still unmet | Document — out of slice-6 scope. Tracked in follow-up "transcript trace producer". |

---

## 15. Migration Path

1. Existing users `git pull` + `cargo run -- serve --auto-migrate`: migration `0004` applies (indexes only).
2. Stage 1 lands as a small PR — receiver only, no observed-event impact, all existing tests still pass.
3. User runs the capture procedure once.
4. Stage 2 lands with real fixtures; reparses any Stage-1 rows in DB on next ingest.
5. `witmcc doctor` is shipped in the same PR as Stage 2.

---

## 16. Acceptance Criteria for Slice-6

1. `POST /otel/v1/metrics` and `POST /otel/v1/logs` accept OTLP/JSON (gzip optional) and persist to `raw_event` with the new `source_type` values. (Stage 1)
2. `tests/fixtures/otel/real/{metrics,logs,traces}_v01.json` exist, captured from a real `claude` invocation.
3. `MetricSample` / `LogRecord` `ObservedEvent` records are produced from the real fixtures with stable `event_id`. (Stage 2)
4. `GET /v1/sessions/:id/graph` returns `metric_sample` and `log_record` nodes for the captured session.
5. Timeline UI shows metric and log markers on the OTel lane with distinct shapes; SourcePanel renders the appropriate sub-view for each.
6. `GET /v1/health/sources` returns the source taxonomy with `last_ingested_at` populated for every source that has rows.
7. `witmcc doctor` outputs the diagnostic table; exits `0` on a healthy capture, `1` on at least one missing collector. No file mutation occurs.
8. All existing cargo tests and webui vitest tests continue to pass; ≥ 8 new cargo tests and ≥ 4 new vitest tests added.
9. `SCHEMA_VERSION` bumped to `0.5`.
10. `docs/implementation-notes.html` gains a `slice-6` section with intentional deviations + known gaps.

---

## 17. Open Decisions (resolved for this slice)

| Decision | Choice | Rationale |
|---|---|---|
| Capture mechanism | None — Stage 1 raw receiver doubles as capture | Source-preserving principle; avoids throwaway tooling. |
| Per-data-point vs per-request raw storage | Per-request in Stage 1, per-data-point in Stage 2 ObservedEvent | Keeps Stage 1 trivially small; Stage 2 explosion is bounded by Claude Code's export interval. |
| Wire format | http/json only | Consistent with slice-3; doctor recommends the env. |
| Doctor automation level | Read-only, no mutation | CLAUDE.md non-goal. User-facing recommendation block prints copy-pastable snippets instead. |
| Lane proliferation | Stay on 8 lanes; shape-differentiate metric/log/span | UX: more lanes = more vertical scroll on existing screens. |
| Cross-source dedup (otel-logs ↔ transcript) | Out of scope; documented in §11 | Distinct merge keys; needs its own validation. |
| Schema bump | 0.4 → 0.5 in Stage 1 (additive enums in Stage 2 are still 0.5) | Stage 1 already changes consumer-visible source_type taxonomy. |

---

## 18. Follow-up slices unblocked by this work

- **transcript trace producer** — once metrics/logs ship `session.id`, wrapping `claude` to inject `traceparent` becomes the natural way to unlock AC-2.
- **cross-source dedup** — otel-logs ↔ transcript user_prompt / tool_result.
- **OTel metrics rollups** — time-series view for cost, tokens, tool calls.
- **token auth + redaction (M7)** — required before metrics/logs carrying prompts can be shared.
- **Findings engine (M5)** — missing_verification, tool_failure, etc. can use metrics (`tool.invocations` count) and logs (`event.name=tool_result`) as evidence.
