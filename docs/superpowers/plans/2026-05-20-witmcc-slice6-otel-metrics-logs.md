# Slice-6 OTel Metrics + Logs Receiver + Doctor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Commit messages:** Do **not** add `Co-Authored-By: Claude...` (or any other Claude attribution) footers. The repository's pre-commit hook rejects commits containing them.

**Goal:** Add OTLP/JSON receivers for **metrics** and **logs** so every Claude Code OTel signal (traces / metrics / logs) lands in witmcc with a single loopback endpoint. Anchor normalisation on a real `claude` payload rather than hand-written fixtures. Ship `witmcc doctor` so the operator can verify which collectors are wired.

**Architecture:** Two-stage receiver. **Stage 1** persists OTLP/JSON requests verbatim into `raw_event` with new `source_type` values ("otel-metrics", "otel-logs"); no normalisation. **Stage 2**, written against real fixtures captured between stages, populates `MetricSample` / `LogRecord` `ObservedEvent` rows and graph nodes. `witmcc doctor` queries a new `GET /v1/health/sources` endpoint and reads env vars + `~/.claude/settings.json` (read-only) to produce a diagnostic table.

**Tech Stack:** Rust 1.88, axum 0.7, sqlx 0.8 (SQLite), serde_json 1, sha2 0.10, flate2 1 (gzip), tokio 1.40. Webui: React 18, TypeScript 5, vitest 2.

**Spec:** `docs/superpowers/specs/2026-05-20-witmcc-slice6-otel-metrics-logs-design.md`

---

## File Structure

| Path | Action | Responsibility |
|---|---|---|
| `src/model/meta.rs` | modify | `SCHEMA_VERSION` 0.4.0 → 0.5.0; add `PARSER_VERSION_OTEL_METRICS`, `PARSER_VERSION_OTEL_LOGS`; add `SOURCE_TYPE_*` constants for the new types. |
| `src/model/observed.rs` | modify | `EventKind::MetricSample`, `EventKind::LogRecord` variants + `as_str` map (Stage 2). |
| `migrations/20260520180000_0004_otel_metrics_logs.sql` | create | Three indexes: instrument_name, log event_name, raw_event(source_type, session_id). |
| `src/api/otel.rs` | modify | `ingest_metrics` + `ingest_logs` handlers (Stage 1); extend with normalised response in Stage 2. |
| `src/api/mod.rs` | modify | Routes for `/otel/v1/metrics`, `/otel/v1/logs`, `/v1/health/sources`. |
| `src/api/routes.rs` | modify | `health_sources` handler. |
| `src/ingest/otel_metrics.rs` | create | Stage 2 normalisation: parse data points → `MetricSample` records, store via existing `repo_observed`. |
| `src/ingest/otel_logs.rs` | create | Stage 2 normalisation: parse log records → `LogRecord` records. |
| `src/ingest/mod.rs` | modify | Re-export new modules. |
| `src/graph/build.rs` | modify | Branches for `EventKind::MetricSample` and `LogRecord`. |
| `src/cli.rs` | modify | New subcommand `Doctor { json: bool, server: String }`. |
| `src/main.rs` | modify | Dispatch `Doctor` → `doctor_cmd`. |
| `src/doctor.rs` | create | Env + settings.json + server health probes; pretty + json output. |
| `tests/fixtures/otel/metrics/minimal.json` | create | Stage 1 hand-crafted minimal OTLP/JSON metrics body. |
| `tests/fixtures/otel/logs/minimal.json` | create | Stage 1 hand-crafted minimal OTLP/JSON logs body. |
| `tests/fixtures/otel/real/metrics_v01.json` | create (Task 5) | Real Claude Code metrics payload. |
| `tests/fixtures/otel/real/logs_v01.json` | create (Task 5) | Real Claude Code logs payload. |
| `tests/fixtures/otel/real/traces_v01.json` | create (Task 5) | Real Claude Code traces payload (beta). |
| `tests/otel_metrics_ingest.rs` | create | Stage 1 + Stage 2 integration. |
| `tests/otel_logs_ingest.rs` | create | Stage 1 + Stage 2 integration. |
| `tests/health_sources.rs` | create | `/v1/health/sources` taxonomy + freshness. |
| `tests/doctor.rs` | create | CLI smoke + mock server. |
| `tests/api.rs` | modify | Update `schema_version` assertion 0.4.0 → 0.5.0. |
| `tests/otel_ingest.rs` | modify | Same. |
| `tests/repo_observed.rs` | modify | Same. |
| `webui/src/api/laneMapping.ts` | modify | Map `metric_sample`, `log_record` → `OTel`. |
| `webui/src/api/__tests__/laneMapping.test.ts` | modify | Two new mappings. |
| `webui/src/components/Timeline.tsx` | modify | Shape branch: diamond for `metric_sample`, triangle for `log_record`. |
| `webui/src/components/__tests__/Timeline.test.tsx` | modify | Regression assertions. |
| `webui/src/components/SourcePanel.tsx` | modify | Sub-renderers for `metric_sample`, `log_record`. |
| `webui/src/components/__tests__/SourcePanel.test.tsx` | modify | Two new render tests. |
| `README.md` | modify | OTel section: full env block, capture procedure, doctor command. |
| `docs/02_technical_architecture_spec.html` | modify | Metrics + logs pipeline diagram entries. |
| `docs/03_data_model_spec.html` | modify | `MetricSample`, `LogRecord` + telemetry facet update. |
| `docs/implementation-notes.html` | modify | New `slice-6` section. |

---

## Branching

Work happens on `slice6-otel-metrics-logs` branched from `main` (post slice-5 merge).

```bash
git checkout main && git pull --ff-only
git checkout -b slice6-otel-metrics-logs
# commit the design spec + this plan first
git add docs/superpowers/specs/2026-05-20-witmcc-slice6-* \
        docs/superpowers/plans/2026-05-20-witmcc-slice6-*
git commit -m "docs(slice-6): design spec + TDD plan — metrics/logs receiver + doctor"
```

---

## Task 1: Bump `SCHEMA_VERSION`, add source-type + parser constants, migration `0004`

**Files:**
- Modify: `src/model/meta.rs`
- Create: `migrations/20260520180000_0004_otel_metrics_logs.sql`
- Modify: `tests/api.rs`
- Modify: `tests/otel_ingest.rs`
- Modify: `tests/repo_observed.rs`

- [ ] **Step 1:** Pin assertions to the new version. Find every literal `"0.4.0"` in the three test files and replace with `"0.5.0"`. Tests must fail on `cargo test` before the bump.
- [ ] **Step 2:** Bump `SCHEMA_VERSION` constant in `src/model/meta.rs` from `"0.4.0"` to `"0.5.0"`.
- [ ] **Step 3:** Add constants in `src/model/meta.rs`:
  ```rust
  pub const SOURCE_TYPE_OTEL_METRICS: &str = "otel-metrics";
  pub const SOURCE_TYPE_OTEL_LOGS:    &str = "otel-logs";
  pub const PARSER_VERSION_OTEL_METRICS: &str = "otel-metrics@0.5";
  pub const PARSER_VERSION_OTEL_LOGS:    &str = "otel-logs@0.5";
  ```
- [ ] **Step 4:** Create migration `migrations/20260520180000_0004_otel_metrics_logs.sql`:
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
- [ ] **Step 5:** `cargo test` — schema-version assertions now pass; new migration applied on init.

**Verify:** `cargo test --all -- --include-ignored 2>&1 | tail -20` shows no regressions.

**Commit:** `chore(meta): bump SCHEMA_VERSION 0.4.0 -> 0.5.0; add otel-metrics/otel-logs constants + migration 0004`

---

## Task 2: Stage 1 — `POST /otel/v1/metrics` raw receiver

**Files:**
- Modify: `src/api/otel.rs`
- Modify: `src/api/mod.rs`
- Create: `tests/fixtures/otel/metrics/minimal.json`

- [ ] **Step 1:** Create `tests/fixtures/otel/metrics/minimal.json` — a minimal valid OTLP/JSON `ExportMetricsServiceRequest` with one `sum` data point carrying `session.id` in resource attributes. (Hand-crafted from OTLP spec; will be retired in favour of real fixture in Task 5.)
- [ ] **Step 2:** Add `ingest_metrics` handler to `src/api/otel.rs`. Logic:
  1. `decode_body(headers, body)` — reuse existing gzip + 4 MB size guard from `ingest_traces`.
  2. Compute `payload_sha256` over the decompressed bytes (canonical via existing `canonical_json` helper).
  3. Extract `session.id` if present in the first `resourceMetrics[0].resource`.
  4. Insert one `raw_event` row via `repo_raw::insert` with `source_type = SOURCE_TYPE_OTEL_METRICS`, `parser_version = PARSER_VERSION_OTEL_METRICS`, `source_uri = format!("otel-metrics://post/{}", &sha[..16])`, `source_line_no = 0`.
  5. Return `Envelope { data: { accepted_resource_metrics, stored_raw_rows }, meta }`.
- [ ] **Step 3:** Wire the route in `src/api/mod.rs`:
  ```rust
  .route("/otel/v1/metrics", post(otel::ingest_metrics))
  ```
- [ ] **Step 4:** Smoke locally:
  ```bash
  cargo run -- serve --auto-migrate &
  curl -X POST http://127.0.0.1:7878/otel/v1/metrics \
    -H 'Content-Type: application/json' \
    --data-binary @tests/fixtures/otel/metrics/minimal.json | jq
  ```
  Expect `data.stored_raw_rows = 1`. Re-running yields `stored_raw_rows = 0` via dedup.

**Verify:** Manual curl returns 200; `sqlite3 .witmcc.sqlite "SELECT source_type, COUNT(*) FROM raw_event GROUP BY source_type"` shows `otel-metrics`.

**Commit:** `feat(api): POST /otel/v1/metrics — Stage1 raw receiver (gzip + dedup)`

---

## Task 3: Stage 1 — `POST /otel/v1/logs` raw receiver

**Files:**
- Modify: `src/api/otel.rs`
- Modify: `src/api/mod.rs`
- Create: `tests/fixtures/otel/logs/minimal.json`

Mirror Task 2 for logs. `source_type = "otel-logs"`, `parser_version = "otel-logs@0.5"`. Response field `accepted_resource_logs`. Hand-crafted fixture has one `logRecords` entry with `severityText="INFO"`, `body.stringValue` set, and a resource `session.id`.

**Commit:** `feat(api): POST /otel/v1/logs — Stage1 raw receiver (gzip + dedup)`

---

## Task 4: Stage 1 integration tests

**Files:**
- Create: `tests/otel_metrics_ingest.rs`
- Create: `tests/otel_logs_ingest.rs`

Each file has at minimum:

1. **POST minimal fixture → 200, one raw row stored, session listed in `/v1/sessions`** (only if fixture carries `session.id`).
2. **Re-POST → dedup, second row count stays at 1.**
3. **Gzip body → 200, same row count.**
4. **Body > 4 MB → 413.**
5. **Non-JSON content-type → 415.**

Use the existing `tests/support` helpers (`spawn_test_server`, `temp_db_pool`) — the slice-3 / slice-4 tests show the pattern.

**Verify:** `cargo test --test otel_metrics_ingest --test otel_logs_ingest` is green; total cargo tests count rises by 10.

**Commit:** `test(otel): Stage1 metrics + logs receivers — ingest, dedup, gzip, size guard`

---

## Task 5: Capture real Claude Code OTel payload + freeze fixtures

**Files:**
- Create: `tests/fixtures/otel/real/metrics_v01.json`
- Create: `tests/fixtures/otel/real/logs_v01.json`
- Create: `tests/fixtures/otel/real/traces_v01.json`
- Modify: `README.md` (capture procedure section)

This task **requires a human-in-the-loop step**: an actual `claude` invocation against witmcc.

- [ ] **Step 1:** Build and run witmcc:
  ```bash
  cargo build --release
  ./target/release/witmcc serve --auto-migrate --watch "$PWD" &
  ```
- [ ] **Step 2:** In a second shell, configure Claude Code:
  ```bash
  export CLAUDE_CODE_ENABLE_TELEMETRY=1
  export CLAUDE_CODE_ENHANCED_TELEMETRY_BETA=1
  export OTEL_METRICS_EXPORTER=otlp
  export OTEL_LOGS_EXPORTER=otlp
  export OTEL_TRACES_EXPORTER=otlp
  export OTEL_EXPORTER_OTLP_PROTOCOL=http/json
  export OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:7878
  export OTEL_METRIC_EXPORT_INTERVAL=5000
  export OTEL_LOGS_EXPORT_INTERVAL=2000
  ```
- [ ] **Step 3:** Run a small `claude` session that exercises Bash, Edit, and at least one user prompt. Exit normally so final metric/log batches flush.
- [ ] **Step 4:** Confirm rows landed:
  ```bash
  sqlite3 .witmcc.sqlite \
    "SELECT source_type, COUNT(*), MAX(ingested_at) FROM raw_event GROUP BY source_type;"
  ```
  Expect rows for `otel-metrics`, `otel-logs`, `otel` (existing traces source_type).
- [ ] **Step 5:** Export one payload per signal:
  ```bash
  for st in otel-metrics otel-logs otel; do
    out=$(echo "$st" | sed 's/otel-metrics/metrics/; s/otel-logs/logs/; s/^otel$/traces/')
    sqlite3 .witmcc.sqlite \
      "SELECT payload FROM raw_event WHERE source_type='$st' ORDER BY ingested_at DESC LIMIT 1;" \
      | python3 -m json.tool > "tests/fixtures/otel/real/${out}_v01.json"
  done
  ```
- [ ] **Step 6:** Inspect each file by hand. Redact local paths and absolute home directories if present (replace with `~`). Note any non-public attribute names in implementation-notes draft for Stage 2.
- [ ] **Step 7:** Add the procedure (steps 1–5) to `README.md` under a new "Capturing real OTel fixtures" subsection. Keep it short — the design spec already has the full prose.

**Verify:** All three real fixture files exist, valid JSON, not larger than 200 KB each. No raw home directory in the bytes (grep `$HOME` after redaction).

**Commit:** `test(otel): freeze real Claude Code payloads — metrics/logs/traces v01 + capture README`

---

## Task 6: Stage 2 — `MetricSample` normalisation

**Files:**
- Modify: `src/model/observed.rs` — add `EventKind::MetricSample` variant + `as_str`.
- Create: `src/ingest/otel_metrics.rs`
- Modify: `src/api/otel.rs` — extend `ingest_metrics` response with `accepted_data_points`, `sessions_touched`.
- Modify: `src/ingest/mod.rs` — re-export.

- [ ] **Step 1:** Add `EventKind::MetricSample`, serialised `"metric_sample"`. Compile breaks in `match` exhaustiveness — fix every site (graph::build will be done in Task 8; for now `unreachable!()` is fine until then).
- [ ] **Step 2:** Define `MetricFacet` in `src/model/observed.rs` matching the design spec §5.2. Serialise into the existing `payload` TEXT column as JSON.
- [ ] **Step 3:** Implement `parse_request(json: &serde_json::Value) -> Vec<MetricSampleRecord>` in `src/ingest/otel_metrics.rs`. Walk `resourceMetrics[].scopeMetrics[].metrics[]`; each metric branches on `sum`/`gauge`/`histogram`/`exponentialHistogram`/`summary` and yields one record per data point. Handle missing fields gracefully — unknown → `null`, never panic. Use the real fixture `metrics_v01.json` as the primary test driver.
- [ ] **Step 4:** Implement `store(pool, request_json) -> StoreResult` that:
  1. Re-uses Stage 1 raw insertion (no duplicate raw row).
  2. For each `MetricSampleRecord` derives `event_id = format!("metric:{r}:{i}:{t}:{a}", …)` per design spec §5.5.
  3. Inserts into `observed_event` via existing `repo_observed::insert`, dedup on `event_id` PK.
  4. Returns `(accepted_data_points, rejected_data_points, sessions_touched)`.
- [ ] **Step 5:** Wire `store` from `ingest_metrics` handler **after** raw insertion. Update response envelope.
- [ ] **Step 6:** Unit tests inside `src/ingest/otel_metrics.rs`:
  - Real fixture parses into ≥ 1 record per known instrument kind found in the fixture.
  - Canonical attribute hashing is stable across key reordering.
  - Missing `dataPoints` array → empty records, no panic.

**Verify:** `cargo test ingest::otel_metrics` is green; `tests/otel_metrics_ingest.rs` now also asserts `accepted_data_points > 0` for the minimal fixture.

**Commit:** `feat(ingest): otel_metrics — parse data points into MetricSample ObservedEvents`

---

## Task 7: Stage 2 — `LogRecord` normalisation

**Files:**
- Modify: `src/model/observed.rs` — `EventKind::LogRecord` + `LogFacet`.
- Create: `src/ingest/otel_logs.rs`
- Modify: `src/api/otel.rs` — extend `ingest_logs` response.
- Modify: `src/ingest/mod.rs` — re-export.

Symmetric to Task 6. `LogFacet` per spec §5.2. `event_id = "log:<resource_sha8>:<time_unix_nano>:<body_sha8>"`. Walk `resourceLogs[].scopeLogs[].logRecords[]`. Populate `trace_id`/`span_id` on the `ObservedEvent` row when the log carries `traceId`/`spanId`.

**Commit:** `feat(ingest): otel_logs — parse log records into LogRecord ObservedEvents`

---

## Task 8: Graph nodes for `metric_sample` + `log_record`

**Files:**
- Modify: `src/graph/build.rs`

- [ ] **Step 1:** Add `EventKind::MetricSample` and `EventKind::LogRecord` branches in `compute`:
  ```rust
  EventKind::MetricSample => ("metric_sample", json!({
      "session_id":      session_id,
      "instrument_name": e.payload.get("instrument_name"),
      "time_unix_nano":  e.payload.get("time_unix_nano"),
  })),
  EventKind::LogRecord => ("log_record", json!({
      "session_id":     session_id,
      "time_unix_nano": e.payload.get("time_unix_nano"),
      "trace_id":       e.trace_id,
      "span_id":        e.span_id,
  })),
  ```
- [ ] **Step 2:** Exclude both kinds from `turn_order` edges (they are not conversation turns — same posture as slice-5's file/git nodes).
- [ ] **Step 3:** Integration test addition (`tests/otel_metrics_ingest.rs`, `tests/otel_logs_ingest.rs`): after ingest, `GET /v1/sessions/<id>/graph` returns ≥ 1 node of each kind.

**Verify:** Cargo tests still green; new graph assertions pass.

**Commit:** `feat(graph): metric_sample / log_record nodes; excluded from turn_order`

---

## Task 9: Webui — lane mapping + Timeline shapes + SourcePanel sub-renderers

**Files:**
- Modify: `webui/src/api/laneMapping.ts`
- Modify: `webui/src/api/__tests__/laneMapping.test.ts`
- Modify: `webui/src/components/Timeline.tsx`
- Modify: `webui/src/components/__tests__/Timeline.test.tsx`
- Modify: `webui/src/components/SourcePanel.tsx`
- Modify: `webui/src/components/__tests__/SourcePanel.test.tsx`

- [ ] **Step 1:** `laneMapping.ts` — add cases:
  ```ts
  case 'metric_sample':
  case 'log_record':
    return 'OTel';
  ```
- [ ] **Step 2:** `Timeline.tsx` marker shape branch — `metric_sample` → diamond, `log_record` → triangle, `otel_span` keeps its existing shape. (Existing marker code is a small switch; minimal CSS change.)
- [ ] **Step 3:** `SourcePanel.tsx` — branches:
  - `record_type === 'metric_sample'` → header row `instrument_name (instrument_kind)`; value row (`value_int` or `value_float`); first 10 attribute entries; then full JsonView.
  - `record_type === 'log_record'` → severity badge; `event_name`; first 10 attribute entries; body (JsonView if non-string, pre-wrap text if string); then full JsonView.
- [ ] **Step 4:** Test additions:
  - `laneMapping.test.ts` — two new mappings.
  - `Timeline.test.tsx` — regression: 8 lanes still visible; metric + log markers render on `OTel` lane when graph contains them.
  - `SourcePanel.test.tsx` — render assertion for each new record_type.

**Verify:** `cd webui && npm test` is green; vitest test count rises by ≥ 4.

**Commit:** `feat(webui): metric_sample / log_record markers + SourcePanel sub-renderers`

---

## Task 10: `GET /v1/health/sources` endpoint

**Files:**
- Modify: `src/api/routes.rs`
- Modify: `src/api/mod.rs`
- Create: `tests/health_sources.rs`

- [ ] **Step 1:** Implement `health_sources(pool) -> Envelope<HealthSources>`. SQL:
  ```sql
  SELECT source_type,
         MAX(ingested_at) AS last_ingested_at,
         SUM(CASE WHEN ingested_at >= datetime('now','-1 day') THEN 1 ELSE 0 END) AS row_count_24h
    FROM raw_event
   GROUP BY source_type;
  ```
  Reshape into the fixed taxonomy from spec §7.4 (`transcript`, `otel-traces`, `otel-metrics`, `otel-logs`, `hook`, `file`, `git`); missing source types appear with `last_ingested_at: null, row_count_24h: 0`.
- [ ] **Step 2:** Note that traces use `source_type = "otel"` in the existing schema; map that to the doctor-facing label `"otel-traces"` in the response shape (no DB rename).
- [ ] **Step 3:** Route it: `.route("/v1/health/sources", get(routes::health_sources))`.
- [ ] **Step 4:** Test (`tests/health_sources.rs`):
  - Empty DB: every source has `last_ingested_at: null, row_count_24h: 0`.
  - After Stage-1 ingestion of metrics + logs minimals: those two have non-null timestamps.

**Verify:** Cargo tests green; new test file passes.

**Commit:** `feat(api): GET /v1/health/sources — per-source last_ingested_at + 24h count`

---

## Task 11: `witmcc doctor` CLI

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/main.rs`
- Create: `src/doctor.rs`
- Create: `tests/doctor.rs`

- [ ] **Step 1:** Add the subcommand in `src/cli.rs`:
  ```rust
  Doctor {
      #[arg(long)] json: bool,
      #[arg(long, default_value = "http://127.0.0.1:7878")] server: String,
  }
  ```
- [ ] **Step 2:** Implement `src/doctor.rs::run(opts) -> std::io::Result<i32>`:
  1. Collect env vars: `CLAUDE_CODE_ENABLE_TELEMETRY`, `CLAUDE_CODE_ENHANCED_TELEMETRY_BETA`, `OTEL_METRICS_EXPORTER`, `OTEL_LOGS_EXPORTER`, `OTEL_TRACES_EXPORTER`, `OTEL_EXPORTER_OTLP_PROTOCOL`, `OTEL_EXPORTER_OTLP_ENDPOINT`. For each, derive a status (`good` / `wrong-value` / `unset`) with an expected-value rule (e.g., `OTEL_EXPORTER_OTLP_PROTOCOL` must be `http/json`).
  2. Read `~/.claude/settings.json` if it exists. `serde_json::from_str` best-effort. Status = `wired` iff at least one of the 9 hook names has a `command` containing the substring `"hooks/v1/events"`.
  3. HTTP `GET {server}/v1/health` and `GET {server}/v1/health/sources` via `reqwest` (already a dep). Timeout 2s.
  4. Print a colourised table (or JSON if `--json`). For human output, use `owo-colors` (already a dep; if not, fall back to ANSI escapes — keep zero new deps).
  5. Recommendation block: for each unset / wrong / missing item, print the exact `export FOO=bar` or jsonc snippet. **Never run** the export.
  6. Exit code: `0` if server reachable AND `transcript` plus at least one of `(otel-metrics, otel-logs, hook)` has rows in the 24h window. Else `1`. With `--json`, always exit `0`.
- [ ] **Step 3:** Wire `Doctor` in `src/main.rs` → `doctor::run`.
- [ ] **Step 4:** Tests (`tests/doctor.rs`):
  - Mock server returning empty sources → exit `1`, output mentions "no data" for transcript.
  - Mock server with transcript + otel-metrics rows → exit `0`.
  - `--json` → output parses as JSON with `sources` and `env` keys.
  - Use `assert_cmd` + `wiremock` (already used in OTel tests).

**Verify:** `cargo run -- doctor` on a clean machine prints sensible output; `cargo test --test doctor` green.

**Commit:** `feat(cli): witmcc doctor — env + hook settings + per-source last_ingested diagnostic`

---

## Task 12: Patch `docs/02_technical_architecture_spec.html` + `docs/03_data_model_spec.html`

**Files:**
- Modify: `docs/02_technical_architecture_spec.html`
- Modify: `docs/03_data_model_spec.html`

- [ ] **Step 1:** `docs/02` — find the OTel section diagram / table and add metrics + logs alongside traces. Two new bullet points in the receiver list: `/otel/v1/metrics` (per-data-point MetricSample), `/otel/v1/logs` (per-record LogRecord). Note `/v1/health/sources` under Pull API.
- [ ] **Step 2:** `docs/03` — add `MetricSample` and `LogRecord` to the EventKind enumeration. Extend telemetry facet description: `MetricFacet`, `LogFacet`. Add `correlation_keys.metric_name` and `correlation_keys.log_event_name` as 1st-class keys (per CLAUDE.md OTel-first principle). Bump the `SCHEMA_VERSION` mentioned in body text to `0.5`.
- [ ] **Step 3:** Sanity-extract the text and grep for "metric_sample" + "log_record":
  ```bash
  python3 -c "
  import re, html, sys
  t = open(sys.argv[1]).read()
  t = re.sub(r'<(script|style)[^>]*>.*?</\\1>', '', t, flags=re.S|re.I)
  print(html.unescape(re.sub(r'<[^>]+>', '\\n', t)))
  " docs/03_data_model_spec.html | grep -E 'metric_sample|log_record|MetricFacet|LogFacet'
  ```
  Expect ≥ 4 hits.

**Commit:** `docs(slice-6): 02 + 03 spec patches — Metrics/Logs in pipeline + data model`

---

## Task 13: README + `docs/implementation-notes.html` update

**Files:**
- Modify: `README.md`
- Modify: `docs/implementation-notes.html`

- [ ] **Step 1:** `README.md` — expand the OTel section with:
  - The full env block from spec §8 (capture procedure).
  - The doctor command quick reference.
  - A "Known gaps" callout reminding that redaction (M7) is still pending and OTel log records can carry secrets.
- [ ] **Step 2:** `docs/implementation-notes.html` — new `slice-6` section in the same style as slice-5. Cover at minimum:
  - **DEV-S6-01** Two-stage receiver design (Stage 1 raw, Stage 2 normalise) — why no separate capture tool.
  - **DEV-S6-02** `source_type = "otel"` vs doctor label `"otel-traces"` — schema preserved, label normalised at API boundary.
  - **DEV-S6-03** Per-data-point ObservedEvent (no rollups) — bounded by Claude Code's export interval.
  - **DEV-S6-04** otel-logs ↔ transcript dedup deferred (mirror DEV-S4-05).
  - **DEV-S6-05** Doctor never mutates files (CLAUDE.md non-goal restated).
  - Commit reference list.
- [ ] **Step 3:** Update `docs/implementation-notes.html` `localnav` to include slice-6 anchors.

**Commit:** `docs(slice-6): implementation-notes section + README OTel envelope + doctor`

---

## Final Verification

```bash
# Backend
cargo test --all -- --include-ignored

# Webui
cd webui && npm ci && npm test

# Manual smoke (loops back to Task 5 procedure)
./target/release/witmcc serve --auto-migrate &
./target/release/witmcc doctor   # exit 1 expected before any claude run
# ... export OTel envs + run claude ...
./target/release/witmcc doctor   # exit 0 expected; all rows recent
```

**Definition of Done:**

- All 10 acceptance criteria from design spec §16 hold.
- `witmcc doctor` reports green when a real `claude` session has flushed once.
- All new fixtures under `tests/fixtures/otel/real/` are committed.
- `SCHEMA_VERSION` is `0.5.0`; migration `0004` ships.
- Three new docs (design spec, plan, implementation-notes section) describe everything.

---

## Branch Merge

```bash
# After all tasks green, squash-or-merge into main:
git checkout main
git merge --no-ff slice6-otel-metrics-logs
git tag witmcc-slice-6
```
