# Usage Telemetry Facet — Implementation Plan (Slice 1 of insight-surface-redesign)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Parse Claude Code `message.usage` (token + cache + model) into a new `usage_facet` side-table during ingest, and expose a session aggregate at `GET /v1/sessions/:id/usage` — unblocking the redesign's Q1 (efficiency), Q2 (cost/token attribution), and Q5 (context accumulation).

**Architecture:** A per-assistant-message side-table mirroring the `verification_run` / `diff_hunk` pattern (side-table, **not** a new EventKind). Usage lives only in `raw_event.payload` (the full transcript line), **not** in `observed_event.payload` (which carries only `model`). So population JOINs `observed_event ⋈ raw_event` and dedupes by `raw_event_id` (one assistant API turn = one raw line = one usage object, even when split into multiple content-block events). A pure parser (`parse_usage`) is unit-tested; population + aggregation live in the repo; ingest wiring follows the existing per-session loop in `src/ingest/store.rs`.

**Tech Stack:** Rust (sqlx + SQLite, serde_json, chrono), axum (Pull API), React + TypeScript + @tanstack/react-query (frontend consumption). Tests: `cargo test`, `npx vitest run`, `npx tsc -b`.

**Out of scope for this plan (later slices):** cache-miss event detection & per-turn growth series (Q5 drill), the `컨텍스트 효율` KpiStrip card UI, verification-detection rewrite (slice 2), cost dollar estimate (slice 5). This slice delivers the facet + session aggregate endpoint + frontend query hook — testable on its own.

---

## File structure

| File | Responsibility | Create/Modify |
|---|---|---|
| `migrations/20260530120000_0014_usage_facet.sql` | `usage_facet` table + indexes | Create |
| `src/ingest/usage_facet.rs` | Pure `parse_usage` + `UsageParsed` | Create |
| `src/db/repo_usage_facet.rs` | Row, `insert`, `list_session`, `assistant_raw_lines`, `session_aggregate` | Create |
| `src/ingest/mod.rs` | register `pub mod usage_facet;` | Modify |
| `src/db/mod.rs` | register `pub mod repo_usage_facet;` | Modify |
| `src/ingest/store.rs` | wire population into per-session ingest loop | Modify |
| `src/api/dto.rs` | `SessionUsageDto` + `ModelUsageDto` | Modify |
| `src/api/routes.rs` | `session_usage` handler | Modify |
| `src/api/mod.rs` | register `/v1/sessions/:id/usage` route | Modify |
| `webui/src/api/types.ts` | `SessionUsageDto` TS type | Modify |
| `webui/src/api/client.ts` | `getSessionUsage` | Modify |
| `webui/src/lib/queries.ts` | `sessionKeys.usage` + `useSessionUsageQuery` | Modify |
| `tests/usage_facet_ingest.rs` | integration test vs real fixture | Create |

---

## Task 1: Migration — `usage_facet` table

**Files:**
- Create: `migrations/20260530120000_0014_usage_facet.sql`

- [ ] **Step 1: Write the migration**

Confirm the next migration number first: `ls migrations/ | tail -3` (expected highest is `..._0013_...`). Then create the file:

```sql
-- Slice insight-surface-redesign #1: usage_facet side-table.
-- One row per assistant API turn (keyed by raw_event_id). Stores token usage
-- parsed from the raw transcript line's message.usage (which is NOT present in
-- observed_event.payload — only `model` is). Side-table, not a new EventKind,
-- mirroring verification_run / diff_hunk.
--
-- parser_version: "usage_facet@v1"
-- schema_version: "usage_facet.v1"

CREATE TABLE IF NOT EXISTS usage_facet (
    raw_event_id                 TEXT PRIMARY KEY,
    -- one assistant API turn = one raw transcript line = one usage object
    schema_version               TEXT NOT NULL DEFAULT 'usage_facet.v1',
    session_id                   TEXT NOT NULL,
    model                        TEXT,
    input_tokens                 INTEGER NOT NULL DEFAULT 0,
    cache_creation_input_tokens  INTEGER NOT NULL DEFAULT 0,
    cache_read_input_tokens      INTEGER NOT NULL DEFAULT 0,
    output_tokens                INTEGER NOT NULL DEFAULT 0,
    observed_at                  TEXT NOT NULL,
    -- ISO 8601 UTC of the earliest content-block event of this message
    parser_version               TEXT NOT NULL,
    created_at                   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_usage_facet_session_observed
    ON usage_facet(session_id, observed_at);
```

- [ ] **Step 2: Verify schema applies**

Run: `cargo run --bin witmcc -- init-db 2>&1 | tail -5`
Expected: no migration error; the sqlx migration hash set updates.

- [ ] **Step 3: Commit**

```bash
git add migrations/20260530120000_0014_usage_facet.sql
git commit -m "feat(usage): migration 0014 — usage_facet side-table"
```

---

## Task 2: Pure parser + record struct (`src/ingest/usage_facet.rs`)

**Files:**
- Create: `src/ingest/usage_facet.rs`
- Modify: `src/ingest/mod.rs` (add `pub mod usage_facet;`)
- Test: in-file `#[cfg(test)]` module

- [ ] **Step 1: Write the failing test**

Create `src/ingest/usage_facet.rs` with only the test + stubs:

```rust
//! usage_facet extractor — parses message.usage from a raw transcript line.

pub const PARSER_VERSION: &str = "usage_facet@v1";
pub const SCHEMA_VERSION: &str = "usage_facet.v1";

/// Token usage parsed from one assistant transcript line.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UsageParsed {
    pub model: Option<String>,
    pub input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub output_tokens: i64,
}

/// Parse `message.usage` + `message.model` from a full raw transcript line
/// (the JSON stored in `raw_event.payload`). Returns `None` when the line is
/// not an assistant message or carries no usage object.
pub fn parse_usage(raw_line: &serde_json::Value) -> Option<UsageParsed> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_usage_and_model_from_assistant_line() {
        let line = json!({
            "type": "assistant",
            "message": {
                "role": "assistant",
                "model": "claude-opus-4-8",
                "usage": {
                    "input_tokens": 2,
                    "cache_creation_input_tokens": 5836,
                    "cache_read_input_tokens": 94234,
                    "output_tokens": 665
                }
            }
        });
        let u = parse_usage(&line).expect("usage present");
        assert_eq!(u.model.as_deref(), Some("claude-opus-4-8"));
        assert_eq!(u.input_tokens, 2);
        assert_eq!(u.cache_creation_input_tokens, 5836);
        assert_eq!(u.cache_read_input_tokens, 94234);
        assert_eq!(u.output_tokens, 665);
    }

    #[test]
    fn returns_none_when_no_usage() {
        let line = json!({ "type": "user", "message": { "role": "user" } });
        assert_eq!(parse_usage(&line), None);
    }

    #[test]
    fn missing_token_fields_default_to_zero() {
        let line = json!({
            "message": { "model": "claude-haiku-4-5-20251001", "usage": { "output_tokens": 10 } }
        });
        let u = parse_usage(&line).expect("usage present");
        assert_eq!(u.output_tokens, 10);
        assert_eq!(u.input_tokens, 0);
        assert_eq!(u.cache_read_input_tokens, 0);
    }
}
```

Add to `src/ingest/mod.rs`: `pub mod usage_facet;`

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test usage_facet::tests 2>&1 | tail -15`
Expected: FAIL — panics at `todo!()` in `parse_usage`.

- [ ] **Step 3: Implement `parse_usage`**

Replace the `todo!()` body:

```rust
pub fn parse_usage(raw_line: &serde_json::Value) -> Option<UsageParsed> {
    let usage = raw_line.pointer("/message/usage")?;
    let model = raw_line
        .pointer("/message/model")
        .and_then(|v| v.as_str())
        .map(String::from);
    let n = |k: &str| usage.get(k).and_then(|v| v.as_i64()).unwrap_or(0);
    Some(UsageParsed {
        model,
        input_tokens: n("input_tokens"),
        cache_creation_input_tokens: n("cache_creation_input_tokens"),
        cache_read_input_tokens: n("cache_read_input_tokens"),
        output_tokens: n("output_tokens"),
    })
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test usage_facet::tests 2>&1 | tail -15`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src/ingest/usage_facet.rs src/ingest/mod.rs
git commit -m "feat(usage): pure parse_usage extractor + tests"
```

---

## Task 3: Repo layer (`src/db/repo_usage_facet.rs`)

**Files:**
- Create: `src/db/repo_usage_facet.rs`
- Modify: `src/db/mod.rs` (add `pub mod repo_usage_facet;`)

- [ ] **Step 1: Write the repo module**

This module has no pure-logic unit test (it is SQL I/O, covered by the Task 4 integration test). Create `src/db/repo_usage_facet.rs`:

```rust
//! Repository for the usage_facet side-table.
use anyhow::Result;
use sqlx::{Row, SqlitePool};

/// A usage_facet row ready for insertion.
#[derive(Debug, Clone, Default)]
pub struct UsageFacetRow {
    pub raw_event_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub model: Option<String>,
    pub input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub output_tokens: i64,
    pub observed_at: String,
    pub parser_version: String,
}

/// One assistant raw transcript line for a session, deduped by raw_event_id.
/// `raw` is the full `raw_event.payload` JSON text; `model` is the cheap copy
/// already present on `observed_event.payload`.
#[derive(Debug, Clone)]
pub struct AssistantRawLine {
    pub raw_event_id: String,
    pub session_id: String,
    pub observed_at: String,
    pub model: Option<String>,
    pub raw: String,
}

/// Aggregate over a session's usage_facet rows.
#[derive(Debug, Clone, Default)]
pub struct UsageAggregate {
    pub turns: i64,
    pub input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub output_tokens: i64,
    pub by_model: Vec<ModelUsage>,
}

#[derive(Debug, Clone, Default)]
pub struct ModelUsage {
    pub model: String,
    pub turns: i64,
    pub output_tokens: i64,
}

pub async fn insert(pool: &SqlitePool, row: &UsageFacetRow) -> Result<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO usage_facet(
            raw_event_id, schema_version, session_id, model,
            input_tokens, cache_creation_input_tokens, cache_read_input_tokens,
            output_tokens, observed_at, parser_version)
         VALUES (?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&row.raw_event_id)
    .bind(&row.schema_version)
    .bind(&row.session_id)
    .bind(&row.model)
    .bind(row.input_tokens)
    .bind(row.cache_creation_input_tokens)
    .bind(row.cache_read_input_tokens)
    .bind(row.output_tokens)
    .bind(&row.observed_at)
    .bind(&row.parser_version)
    .execute(pool)
    .await?;
    Ok(())
}

/// Distinct assistant raw lines for a session (one per raw_event_id), so the
/// caller can parse usage from `raw`. Usage lives only in raw_event.payload.
pub async fn assistant_raw_lines(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Vec<AssistantRawLine>> {
    let rows = sqlx::query(
        "SELECT oe.raw_event_id            AS raw_event_id,
                oe.session_id              AS session_id,
                MIN(oe.observed_at)        AS observed_at,
                json_extract(oe.payload,'$.model') AS model,
                CAST(re.payload AS TEXT)   AS raw
         FROM observed_event oe
         JOIN raw_event re ON oe.raw_event_id = re.raw_event_id
         WHERE oe.kind = 'assistant_message' AND oe.session_id = ?
         GROUP BY oe.raw_event_id",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| AssistantRawLine {
            raw_event_id: r.get("raw_event_id"),
            session_id: r.get("session_id"),
            observed_at: r.get("observed_at"),
            model: r.get("model"),
            raw: r.get("raw"),
        })
        .collect())
}

pub async fn session_aggregate(pool: &SqlitePool, session_id: &str) -> Result<UsageAggregate> {
    let row = sqlx::query(
        "SELECT COUNT(*) AS turns,
                COALESCE(SUM(input_tokens),0) AS input_tokens,
                COALESCE(SUM(cache_creation_input_tokens),0) AS cc,
                COALESCE(SUM(cache_read_input_tokens),0) AS cr,
                COALESCE(SUM(output_tokens),0) AS output_tokens
         FROM usage_facet WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await?;

    let by_model_rows = sqlx::query(
        "SELECT COALESCE(model,'unknown') AS model,
                COUNT(*) AS turns,
                COALESCE(SUM(output_tokens),0) AS output_tokens
         FROM usage_facet WHERE session_id = ?
         GROUP BY model ORDER BY turns DESC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;

    Ok(UsageAggregate {
        turns: row.get("turns"),
        input_tokens: row.get("input_tokens"),
        cache_creation_input_tokens: row.get("cc"),
        cache_read_input_tokens: row.get("cr"),
        output_tokens: row.get("output_tokens"),
        by_model: by_model_rows
            .into_iter()
            .map(|r| ModelUsage {
                model: r.get("model"),
                turns: r.get("turns"),
                output_tokens: r.get("output_tokens"),
            })
            .collect(),
    })
}
```

Add to `src/db/mod.rs`: `pub mod repo_usage_facet;`

- [ ] **Step 2: Verify it compiles**

Run: `cargo build 2>&1 | tail -15`
Expected: compiles clean (no test yet — exercised in Task 4).

- [ ] **Step 3: Commit**

```bash
git add src/db/repo_usage_facet.rs src/db/mod.rs
git commit -m "feat(usage): repo_usage_facet (insert, raw lines, aggregate)"
```

---

## Task 4: Ingest wiring + real-fixture integration test

**Files:**
- Modify: `src/ingest/store.rs` (import + per-session loop)
- Create: `tests/usage_facet_ingest.rs`

- [ ] **Step 1: Write the failing integration test**

The real fixture `tests/fixtures/transcripts/real/verification_v01.jsonl` is session `aac68973-729e-4014-a02b-28a556f5ff29` and its assistant lines carry real `message.usage` (cache_read > 0). Mirror the pool + ingest setup used by `tests/api_diff_hunks.rs` / `tests/api_findings.rs` (`SqlitePoolOptions` :memory: + `migrate`, and `store::ingest_file(&pool, path, &NoopSink)`). Import `NoopSink` exactly as those sibling tests do (the no-op `IngestSink`). Create `tests/usage_facet_ingest.rs`:

```rust
//! Real-data anchoring: ingesting the frozen verification_v01 transcript must
//! populate usage_facet with real token counts (cache_read > 0).
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::{migrate, repo_usage_facet};
use witmcc::ingest::store;
// NoopSink: copy the exact import line from tests/api_diff_hunks.rs (no-op IngestSink).
use witmcc::ingest::store::NoopSink;

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
async fn ingest_populates_usage_facet_from_real_fixture() {
    let pool = empty_pool().await;
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/transcripts/real/verification_v01.jsonl"),
        &NoopSink,
    )
    .await
    .expect("ingest");

    let agg = repo_usage_facet::session_aggregate(&pool, "aac68973-729e-4014-a02b-28a556f5ff29")
        .await
        .expect("aggregate");

    assert!(agg.turns > 0, "expected assistant turns with usage");
    assert!(
        agg.cache_read_input_tokens > 0,
        "real fixture has prompt-cache reads"
    );
    let billed = agg.input_tokens + agg.cache_creation_input_tokens;
    assert!(agg.cache_read_input_tokens + billed > 0);
}
```

(If `NoopSink`'s import path differs, copy it verbatim from `tests/api_diff_hunks.rs` line ~11 — do not invent one.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test usage_facet_ingest 2>&1 | tail -20`
Expected: FAIL — `agg.turns == 0` (population not wired yet).

- [ ] **Step 3: Wire population into `src/ingest/store.rs`**

Add to the module import line (currently `use crate::ingest::{diff_hunk, mapping, transcript, verification_run};`):

```rust
use crate::ingest::{diff_hunk, mapping, transcript, usage_facet, verification_run};
```

In the `for session_id in &stats.sessions_touched { ... }` loop, immediately AFTER the existing verification_run extraction block and BEFORE `crate::graph::build::rebuild_session(...)`, add:

```rust
        // insight-redesign #1 — populate usage_facet from raw transcript lines.
        // Usage lives only in raw_event.payload, so we read the joined raw line
        // and parse it; dedupe is by raw_event_id (one assistant turn = one row).
        if !session_id.is_empty() {
            let lines = repo_usage_facet::assistant_raw_lines(pool, session_id).await?;
            for line in lines {
                let Ok(val) = serde_json::from_str::<serde_json::Value>(&line.raw) else {
                    continue;
                };
                let Some(u) = usage_facet::parse_usage(&val) else {
                    continue;
                };
                repo_usage_facet::insert(
                    pool,
                    &repo_usage_facet::UsageFacetRow {
                        raw_event_id: line.raw_event_id,
                        schema_version: usage_facet::SCHEMA_VERSION.to_string(),
                        session_id: line.session_id,
                        model: u.model.or(line.model),
                        input_tokens: u.input_tokens,
                        cache_creation_input_tokens: u.cache_creation_input_tokens,
                        cache_read_input_tokens: u.cache_read_input_tokens,
                        output_tokens: u.output_tokens,
                        observed_at: line.observed_at,
                        parser_version: usage_facet::PARSER_VERSION.to_string(),
                    },
                )
                .await?;
            }
        }
```

Add the repo import at the top of `store.rs` if repos are imported individually (grep `use crate::db::` in the file and match its style, e.g. add `repo_usage_facet` to the existing `use crate::db::{...}` list).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test usage_facet_ingest 2>&1 | tail -20`
Expected: PASS. Then full suite: `cargo test 2>&1 | tail -15` — no regressions.

- [ ] **Step 5: Commit**

```bash
git add src/ingest/store.rs tests/usage_facet_ingest.rs
git commit -m "feat(usage): wire usage_facet population into ingest + real-fixture test"
```

---

## Task 5: Pull API endpoint `GET /v1/sessions/:id/usage`

**Files:**
- Modify: `src/api/dto.rs`, `src/api/routes.rs`, `src/api/mod.rs`
- Test: `tests/api_usage.rs` (Create)

- [ ] **Step 1: Add the DTOs** (`src/api/dto.rs`, near `VerificationRunDto`)

```rust
/// insight-redesign #1 — session token-usage aggregate.
#[derive(Serialize)]
pub struct SessionUsageDto {
    pub session_id: String,
    pub turns: i64,
    pub input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub output_tokens: i64,
    /// input + cache_creation + output (cache_read is NOT billed)
    pub billed_tokens: i64,
    /// cache_read / (cache_read + cache_creation + input); null when denom 0
    pub cache_hit_ratio: Option<f64>,
    pub by_model: Vec<ModelUsageDto>,
}

#[derive(Serialize)]
pub struct ModelUsageDto {
    pub model: String,
    pub turns: i64,
    pub output_tokens: i64,
}
```

- [ ] **Step 2: Write the failing endpoint test** (`tests/api_usage.rs`)

Mirror `tests/api_episodes.rs` exactly for server construction: `AppState::new_for_tests(pool)` + `TestServer::new(router(state))` + `server.get(path).await`. Create `tests/api_usage.rs`:

```rust
//! GET /v1/sessions/:id/usage returns the token-usage aggregate envelope.
use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::api::{router, AppState};
use witmcc::db::migrate;
use witmcc::ingest::store;
use witmcc::ingest::store::NoopSink; // same import as tests/api_diff_hunks.rs

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
    assert!(data["turns"].as_i64().unwrap() > 0);
    assert!(data["cache_read_input_tokens"].as_i64().unwrap() > 0);
    assert!(data["billed_tokens"].as_i64().unwrap() > 0);
    let chr = data["cache_hit_ratio"].as_f64().unwrap();
    assert!(chr > 0.0 && chr <= 1.0);
}
```

Run: `cargo test --test api_usage 2>&1 | tail -20`
Expected: FAIL — route not registered (404 / handler missing).

- [ ] **Step 3: Add the handler** (`src/api/routes.rs`, near `session_verification_runs`)

```rust
/// insight-redesign #1 — `GET /v1/sessions/:id/usage`
pub async fn session_usage(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let agg = repo_usage_facet::session_aggregate(&pool, &id)
        .await
        .expect("db");
    let billed = agg.input_tokens + agg.cache_creation_input_tokens + agg.output_tokens;
    let denom = agg.cache_read_input_tokens + agg.cache_creation_input_tokens + agg.input_tokens;
    let cache_hit_ratio = if denom > 0 {
        Some(agg.cache_read_input_tokens as f64 / denom as f64)
    } else {
        None
    };
    let data = SessionUsageDto {
        session_id: id,
        turns: agg.turns,
        input_tokens: agg.input_tokens,
        cache_creation_input_tokens: agg.cache_creation_input_tokens,
        cache_read_input_tokens: agg.cache_read_input_tokens,
        output_tokens: agg.output_tokens,
        billed_tokens: billed,
        cache_hit_ratio,
        by_model: agg
            .by_model
            .into_iter()
            .map(|m| ModelUsageDto {
                model: m.model,
                turns: m.turns,
                output_tokens: m.output_tokens,
            })
            .collect(),
    };
    Json(Envelope { meta: ResponseMeta::now(), data })
}
```

Ensure `repo_usage_facet`, `SessionUsageDto`, `ModelUsageDto` are imported at the top of `routes.rs` (match the existing `use` style).

- [ ] **Step 4: Register the route** (`src/api/mod.rs`, immediately after the `verification-runs` route)

```rust
        .route(
            "/v1/sessions/:id/usage",
            get(routes::session_usage),
        )
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --test api_usage 2>&1 | tail -20`
Expected: PASS. Then `cargo test 2>&1 | tail -10` — no regressions.

- [ ] **Step 6: Commit**

```bash
git add src/api/dto.rs src/api/routes.rs src/api/mod.rs tests/api_usage.rs
git commit -m "feat(usage): GET /v1/sessions/:id/usage aggregate endpoint"
```

---

## Task 6: Frontend query wiring

**Files:**
- Modify: `webui/src/api/types.ts`, `webui/src/api/client.ts`, `webui/src/lib/queries.ts`
- Test: `webui/src/api/__tests__/client.endpoints.test.ts` (Modify — add case)

- [ ] **Step 1: Add the TS type** (`webui/src/api/types.ts`)

```typescript
export type ModelUsageDto = { model: string; turns: number; output_tokens: number };

export type SessionUsageDto = {
  session_id: string;
  turns: number;
  input_tokens: number;
  cache_creation_input_tokens: number;
  cache_read_input_tokens: number;
  output_tokens: number;
  billed_tokens: number;
  cache_hit_ratio: number | null;
  by_model: ModelUsageDto[];
};
```

- [ ] **Step 2: Write the failing client test** (add to `webui/src/api/__tests__/client.endpoints.test.ts`)

Follow the file's existing pattern verbatim: `ENVELOPE = (data) => ({ meta:{...}, data })`, `mockJson(payload)`, and the module-level `fetchSpy` set in `beforeEach`. Add this case (and add `getSessionUsage` to the existing import from `../client`):

```typescript
it('getSessionUsage unwraps the usage envelope', async () => {
  const expected = {
    session_id: 's1', turns: 3, input_tokens: 10,
    cache_creation_input_tokens: 20, cache_read_input_tokens: 900,
    output_tokens: 30, billed_tokens: 60, cache_hit_ratio: 0.96, by_model: [],
  };
  fetchSpy.mockImplementation(mockJson(ENVELOPE(expected)));
  const out = await getSessionUsage('s1');
  expect(out).toEqual(expected);
});
```

Run: `npx vitest run src/api/__tests__/client.endpoints.test.ts 2>&1 | tail -15`
Expected: FAIL — `getSessionUsage` is not exported.

- [ ] **Step 3: Add the client fn** (`webui/src/api/client.ts`, near `getVerificationRuns`)

```typescript
export const getSessionUsage = (id: string): Promise<SessionUsageDto> =>
  jsonGet<SessionUsageDto>(`/v1/sessions/${encodeURIComponent(id)}/usage`);
```

Add `SessionUsageDto` to the type import from `./types`.

- [ ] **Step 4: Add the query hook + key** (`webui/src/lib/queries.ts`)

In `sessionKeys`, add: `usage: (id: string) => ['session', id, 'usage'] as const,`
Then:

```typescript
export function useSessionUsageQuery(id: string, opts?: QOpts<SessionUsageDto>) {
  return useQuery<SessionUsageDto>({
    queryKey: sessionKeys.usage(id),
    queryFn: () => getSessionUsage(id),
    enabled: !!id,
    ...opts,
  });
}
```

Add `getSessionUsage` to the imports from `../api/client` and `SessionUsageDto` from `../api/types`.

- [ ] **Step 5: Run tests + types**

Run: `npx vitest run src/api/__tests__/client.endpoints.test.ts 2>&1 | tail -15` → PASS
Run: `npx tsc -b 2>&1 | tail -15` → clean

- [ ] **Step 6: Commit**

```bash
git add webui/src/api/types.ts webui/src/api/client.ts webui/src/lib/queries.ts webui/src/api/__tests__/client.endpoints.test.ts
git commit -m "feat(usage): frontend types + getSessionUsage + useSessionUsageQuery"
```

---

## Task 7: Re-ingest verification + manual endpoint smoke

**Files:** none (operational verification)

- [ ] **Step 1: Rebuild DB and re-ingest** so existing dev data gains usage_facet rows

Run: `cargo run --bin witmcc -- init-db && cargo run --bin witmcc -- ingest --all 2>&1 | tail -5`
Expected: ingest completes; no errors.

- [ ] **Step 2: Smoke the endpoint against a real session**

Run: `cargo run --bin witmcc -- serve --bind 127.0.0.1 --port 7878 &` then
`sleep 2 && curl -s http://127.0.0.1:7878/v1/sessions/653ea169-1121-442e-9cc9-776471a10895/usage | python3 -m json.tool | head -30`
Expected: JSON with `turns`, `cache_read_input_tokens` (large), `billed_tokens`, `cache_hit_ratio` ≈ 0.97–0.98, and a `by_model` array showing `claude-opus-4-8` and `claude-haiku-4-5-20251001`. Stop the server afterward.

- [ ] **Step 3: Document in implementation-notes**

Add a section to `docs/implementation-notes.html` (new `§` entry): usage_facet (migration 0014), the raw_event-JOIN rationale (usage absent from observed_event.payload), dedup-by-raw_event_id, the `/v1/sessions/:id/usage` aggregate, and the operational note that `init-db` + re-ingest is required. Commit:

```bash
git add docs/implementation-notes.html
git commit -m "docs(usage): implementation notes for usage_facet slice"
```

---

## Done criteria

- `usage_facet` populated on ingest; `GET /v1/sessions/:id/usage` returns a correct aggregate (cache-hit, billed vs cache-read split, per-model) with the standard Envelope.
- All new tests pass; `cargo test` + `npx vitest run` + `npx tsc -b` clean; no regressions.
- Real-fixture invariant locks cache_read > 0 from `verification_v01.jsonl`.
- This unblocks the redesign's Q1/Q2/Q5 data. Next slice: verification-detection rewrite (slice 2); then the `컨텍스트 효율` / 토큰 KpiStrip cards consume `useSessionUsageQuery`.
