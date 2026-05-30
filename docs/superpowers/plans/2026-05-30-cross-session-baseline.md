# Cross-Session Baseline — Implementation Plan (Slice 6 of insight-surface-redesign)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn a single session's raw usage numbers into *signal* by comparing them against the user's OWN rolling median across all stored sessions that have `usage_facet` rows. Expose a new endpoint `GET /v1/usage/baseline` returning the median (plus p25/p75) of four key per-session metrics — `cache_hit_ratio`, `billed_tokens`, `turns`, `output_tokens` — so the frontend can render a "vs your median" delta under each measured value (spec §4 proposal A, adopted §11.1).

**Architecture:** SQLite has **no** `MEDIAN()`. So the median is computed in Rust: a new repo query `per_session_metrics` returns ONE row per session (only sessions with ≥1 `usage_facet` row), each row carrying that session's four metrics computed in SQL the same way the existing `/v1/sessions/:id/usage` endpoint computes them (so the baseline is comparable to the per-session value). A pure helper `median_p25_p75(values)` (unit-tested with no DB) reduces a `Vec<f64>` to the three quantiles. The endpoint composes them into a `UsageBaselineDto`. No migration — this slice only *reads* the `usage_facet` table that slice 1 already created. The endpoint is registered in the auth-gated `/v1/*` group alongside the existing session-usage route.

**Tech Stack:** Rust (sqlx + SQLite, serde), axum (Pull API), React + TypeScript + @tanstack/react-query (frontend consumption). Tests: `cargo test`, `npx vitest run`, `npx tsc -b`.

**Why a separate endpoint (not a `vs_baseline` field folded into `/v1/sessions/:id/usage`):** the baseline is a *session-independent* aggregate over the whole store. Folding it into every per-session response would recompute the cross-session median on every session fetch and couple two concerns. A dedicated `GET /v1/usage/baseline` is fetched once and the frontend computes the delta client-side (it already has the per-session `SessionUsageDto` from `useSessionUsageQuery`). This matches the spec wording in §11.1: *"renders as a 'vs your median' delta under each measured value."*

**Out of scope for this plan (later/other work):** the KpiStrip card UI that renders the "vs median" delta visually (component slice — this plan only delivers the data path + query hook, mirroring slice 1's scope boundary); windowing the baseline to "recent N sessions" (this slice computes the median over **all** sessions with `usage_facet` rows — simplest correct version per the slice brief); per-model baselines; cost-dollar baselines.

---

## File structure

| File | Responsibility | Create/Modify |
|---|---|---|
| `src/db/repo_usage_facet.rs` | `SessionMetrics` struct + `per_session_metrics` query + `median_p25_p75` pure helper + unit tests | Modify |
| `src/api/dto.rs` | `UsageBaselineDto` + `BaselineStat` | Modify |
| `src/api/routes.rs` | `usage_baseline` handler | Modify |
| `src/api/mod.rs` | register `/v1/usage/baseline` route | Modify |
| `tests/api_usage_baseline.rs` | endpoint integration test (seed multiple sessions, assert medians) | Create |
| `webui/src/api/types.ts` | `UsageBaselineDto` + `BaselineStat` TS types | Modify |
| `webui/src/api/client.ts` | `getUsageBaseline` | Modify |
| `webui/src/lib/queries.ts` | `usageKeys.baseline` + `useUsageBaselineQuery` | Modify |
| `webui/src/api/__tests__/client.endpoints.test.ts` | client test for `getUsageBaseline` | Modify |
| `webui/src/lib/__tests__/queries.test.tsx` | hook test for `useUsageBaselineQuery` | Modify |

---

## Task 1: Repo — per-session metrics query + median helper (`src/db/repo_usage_facet.rs`)

**Files:**
- Modify: `src/db/repo_usage_facet.rs` (add struct, query fn, pure helper, and unit tests)

The existing module already imports `use sqlx::{Row, SqlitePool};` and `use crate::error::Result;`, and has a `#[cfg(test)] mod tests` with a `pool()` helper and a `row(...)` builder that inserts into `sess_uf_test`. Reuse them.

- [ ] **Step 1: Write the failing unit tests**

Add these two structs ABOVE the existing `pub async fn insert(...)` (after the `ModelUsage` struct definition, around line 49):

```rust
/// insight-redesign #6 — one row per session that has usage_facet rows.
/// Each metric is computed the same way `/v1/sessions/:id/usage` computes it,
/// so a session's value is directly comparable to the cross-session baseline.
#[derive(Debug, Clone)]
pub struct SessionMetrics {
    pub session_id: String,
    /// cache_read / (cache_read + cache_creation + input); None when denom 0.
    pub cache_hit_ratio: Option<f64>,
    /// input + cache_creation + output (cache_read is NOT billed).
    pub billed_tokens: i64,
    pub turns: i64,
    pub output_tokens: i64,
}

/// insight-redesign #6 — quantile triple for one baseline metric.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quantiles {
    pub p25: f64,
    pub median: f64,
    pub p75: f64,
}
```

Then add the pure helper (also above `insert`, no DB) — start with a `todo!()` body so the test goes red:

```rust
/// Compute p25 / median (p50) / p75 from an unsorted slice of values using the
/// "nearest-rank with linear interpolation" method (type-7, the default in R /
/// numpy.percentile). Returns None for an empty slice. SQLite has no MEDIAN(),
/// so this is computed in Rust over the per-session metric values.
pub fn median_p25_p75(values: &[f64]) -> Option<Quantiles> {
    todo!()
}
```

Now add the unit tests INSIDE the existing `#[cfg(test)] mod tests { ... }` block (after the `insert_or_replace_dedup` test, before the closing `}`):

```rust
    #[test]
    fn quantiles_of_empty_is_none() {
        assert!(median_p25_p75(&[]).is_none());
    }

    #[test]
    fn quantiles_single_value() {
        let q = median_p25_p75(&[7.0]).unwrap();
        assert_eq!(q.median, 7.0);
        assert_eq!(q.p25, 7.0);
        assert_eq!(q.p75, 7.0);
    }

    #[test]
    fn median_odd_count_is_middle() {
        // sorted: 1,3,5,7,9 -> median index 2 -> 5.0
        let q = median_p25_p75(&[5.0, 1.0, 9.0, 3.0, 7.0]).unwrap();
        assert_eq!(q.median, 5.0);
    }

    #[test]
    fn median_even_count_interpolates_midpoint() {
        // sorted: 2,4,6,8 -> median between 4 and 6 -> 5.0
        let q = median_p25_p75(&[8.0, 2.0, 6.0, 4.0]).unwrap();
        assert_eq!(q.median, 5.0);
    }

    #[test]
    fn p25_p75_type7_interpolation() {
        // sorted: 10,20,30,40 (n=4). type-7 rank h = (n-1)*p.
        // p25: h = 3*0.25 = 0.75 -> 10 + 0.75*(20-10) = 17.5
        // p75: h = 3*0.75 = 2.25 -> 30 + 0.25*(40-30) = 32.5
        let q = median_p25_p75(&[40.0, 10.0, 30.0, 20.0]).unwrap();
        assert_eq!(q.p25, 17.5);
        assert_eq!(q.median, 25.0);
        assert_eq!(q.p75, 32.5);
    }

    #[tokio::test]
    async fn per_session_metrics_one_row_per_session() {
        let pool = pool().await;
        // Session A (the shared sess_uf_test): two opus/haiku turns.
        insert(&pool, &row("raw_a1", "claude-opus-4-8", 2, 100, 5000, 300))
            .await
            .unwrap();
        insert(
            &pool,
            &row("raw_a2", "claude-haiku-4-5-20251001", 3, 200, 6000, 400),
        )
        .await
        .unwrap();
        // Session B: one turn, distinct session_id.
        let mut b = row("raw_b1", "claude-opus-4-8", 10, 0, 0, 50);
        b.session_id = "sess_uf_other".into();
        insert(&pool, &b).await.unwrap();

        let mut metrics = per_session_metrics(&pool).await.unwrap();
        metrics.sort_by(|x, y| x.session_id.cmp(&y.session_id));
        assert_eq!(metrics.len(), 2, "one row per session with usage rows");

        let a = &metrics[0];
        assert_eq!(a.session_id, "sess_uf_other");
        // Session B denom = 0+0+10 = 10, cache_read 0 -> ratio 0.0.
        assert_eq!(a.cache_hit_ratio, Some(0.0));
        assert_eq!(a.billed_tokens, 10 + 0 + 50);
        assert_eq!(a.turns, 1);
        assert_eq!(a.output_tokens, 50);

        let s = &metrics[1];
        assert_eq!(s.session_id, "sess_uf_test");
        // Session A: input 5, cc 300, cr 11000 -> denom 11305, ratio 11000/11305.
        let ratio = s.cache_hit_ratio.unwrap();
        assert!((ratio - 11000.0 / 11305.0).abs() < 1e-9);
        assert_eq!(s.billed_tokens, 5 + 300 + 700);
        assert_eq!(s.turns, 2);
        assert_eq!(s.output_tokens, 700);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test repo_usage_facet 2>&1 | tail -25`
Expected: FAIL — `median_p25_p75` panics at `todo!()`; `per_session_metrics_one_row_per_session` fails to compile / link (`per_session_metrics` not defined yet). (If the missing function blocks compilation, that *is* the red state — implement Step 3, then re-run.)

- [ ] **Step 3: Implement `median_p25_p75`**

Replace the `todo!()` body:

```rust
pub fn median_p25_p75(values: &[f64]) -> Option<Quantiles> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let quantile = |p: f64| -> f64 {
        // type-7 (R/numpy default): rank h = (n-1)*p, linear interpolation.
        let n = v.len();
        if n == 1 {
            return v[0];
        }
        let h = (n as f64 - 1.0) * p;
        let lo = h.floor() as usize;
        let hi = h.ceil() as usize;
        let frac = h - lo as f64;
        v[lo] + frac * (v[hi] - v[lo])
    };
    Some(Quantiles {
        p25: quantile(0.25),
        median: quantile(0.5),
        p75: quantile(0.75),
    })
}
```

- [ ] **Step 4: Implement `per_session_metrics`**

Add this query fn after `session_aggregate` (and before the `fn map_assistant_raw_line` helpers). It mirrors the billed/cache-hit arithmetic from `src/api/routes.rs::session_usage` but computed per-session in SQL — `cache_hit_ratio` is left NULL when the denominator is 0 (CASE guard) so the Rust side reads it as `Option<f64>`:

```rust
/// insight-redesign #6 — per-session metric rows for the cross-session
/// baseline. One row per session that has at least one usage_facet row.
/// billed_tokens = input + cache_creation + output (cache_read NOT billed).
/// cache_hit_ratio = cache_read / (cache_read + cache_creation + input);
/// NULL when the denominator is 0 (mirrors `/v1/sessions/:id/usage`).
pub async fn per_session_metrics(pool: &SqlitePool) -> Result<Vec<SessionMetrics>> {
    let rows = sqlx::query(
        "SELECT session_id,
                COUNT(*) AS turns,
                COALESCE(SUM(input_tokens),0)
                  + COALESCE(SUM(cache_creation_input_tokens),0)
                  + COALESCE(SUM(output_tokens),0)            AS billed_tokens,
                COALESCE(SUM(output_tokens),0)                AS output_tokens,
                CASE
                  WHEN (COALESCE(SUM(cache_read_input_tokens),0)
                        + COALESCE(SUM(cache_creation_input_tokens),0)
                        + COALESCE(SUM(input_tokens),0)) > 0
                  THEN CAST(COALESCE(SUM(cache_read_input_tokens),0) AS REAL)
                       / (COALESCE(SUM(cache_read_input_tokens),0)
                          + COALESCE(SUM(cache_creation_input_tokens),0)
                          + COALESCE(SUM(input_tokens),0))
                  ELSE NULL
                END                                           AS cache_hit_ratio
         FROM usage_facet
         GROUP BY session_id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(map_session_metrics).collect())
}

fn map_session_metrics(r: sqlx::sqlite::SqliteRow) -> SessionMetrics {
    SessionMetrics {
        session_id: r.get("session_id"),
        cache_hit_ratio: r.get::<Option<f64>, _>("cache_hit_ratio"),
        billed_tokens: r.get::<i64, _>("billed_tokens"),
        turns: r.get::<i64, _>("turns"),
        output_tokens: r.get::<i64, _>("output_tokens"),
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test repo_usage_facet 2>&1 | tail -25`
Expected: PASS (existing 2 + new 6 tests). Then `cargo build 2>&1 | tail -5` — clean.

- [ ] **Step 6: Commit**

```bash
git add src/db/repo_usage_facet.rs
git commit -m "feat(usage): per_session_metrics query + median_p25_p75 helper + tests"
```

---

## Task 2: DTOs (`src/api/dto.rs`)

**Files:**
- Modify: `src/api/dto.rs` (add `BaselineStat` + `UsageBaselineDto` near `SessionUsageDto`)

- [ ] **Step 1: Add the DTOs**

Insert immediately AFTER the existing `ModelUsageDto` struct (currently ends at line 263):

```rust
/// insight-redesign #6 — one baseline metric's quantile triple.
/// `median` is the user's rolling norm; the frontend renders the measured
/// session value as a delta against it ("vs your median"). All three are null
/// when no session in the store has usage_facet rows for this metric.
#[derive(Serialize)]
pub struct BaselineStat {
    pub p25: Option<f64>,
    pub median: Option<f64>,
    pub p75: Option<f64>,
}

/// insight-redesign #6 — cross-session usage baseline. Median (+ p25/p75) of
/// each key metric across ALL stored sessions that have usage_facet rows.
/// `session_count` is the number of sessions the baseline was computed over.
#[derive(Serialize)]
pub struct UsageBaselineDto {
    pub session_count: i64,
    pub cache_hit_ratio: BaselineStat,
    pub billed_tokens: BaselineStat,
    pub turns: BaselineStat,
    pub output_tokens: BaselineStat,
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build 2>&1 | tail -5`
Expected: clean (DTOs are `Serialize`-only; no handler uses them yet — that is Task 3).

- [ ] **Step 3: Commit**

```bash
git add src/api/dto.rs
git commit -m "feat(usage): UsageBaselineDto + BaselineStat DTOs"
```

---

## Task 3: Endpoint `GET /v1/usage/baseline` (`src/api/routes.rs` + `src/api/mod.rs`)

**Files:**
- Modify: `src/api/routes.rs` (add `usage_baseline` handler)
- Modify: `src/api/mod.rs` (register the route)
- Create: `tests/api_usage_baseline.rs`

- [ ] **Step 1: Write the failing endpoint test**

`tests/api_usage.rs` uses `use witmcc::live::NoopSink;` (NOT `ingest::store::NoopSink`) — copy that import verbatim. This test seeds **two distinct sessions' rows directly** via `repo_usage_facet::insert` (the real-fixture ingest path is already locked by `tests/usage_facet_ingest.rs`; here we need ≥2 sessions with known values to assert the median deterministically). Create `tests/api_usage_baseline.rs`:

```rust
//! GET /v1/usage/baseline returns the cross-session median (+ p25/p75) of the
//! key usage metrics. Seeds two sessions with known values and asserts the
//! median is computed correctly in Rust (SQLite has no MEDIAN()).
use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::api::{router, AppState};
use witmcc::db::{migrate, repo_usage_facet};
use witmcc::db::repo_usage_facet::UsageFacetRow;

async fn empty_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

fn uf(raw_event_id: &str, session_id: &str, input: i64, cc: i64, cr: i64, output: i64) -> UsageFacetRow {
    UsageFacetRow {
        raw_event_id: raw_event_id.into(),
        schema_version: "usage_facet.v1".into(),
        session_id: session_id.into(),
        model: Some("claude-opus-4-8".into()),
        input_tokens: input,
        cache_creation_input_tokens: cc,
        cache_read_input_tokens: cr,
        output_tokens: output,
        observed_at: "2026-05-30T10:00:00Z".into(),
        parser_version: "usage_facet@v1".into(),
    }
}

#[tokio::test]
async fn baseline_endpoint_returns_median_across_sessions() {
    let pool = empty_pool().await;

    // Session 1: turns=1, billed = 100 + 0 + 100 = 200, output 100,
    //   denom = cr(0)+cc(0)+input(100)=100, cache_hit_ratio = 0/100 = 0.0
    repo_usage_facet::insert(&pool, &uf("r1", "sess_lo", 100, 0, 0, 100))
        .await
        .unwrap();
    // Session 2: turns=1, billed = 100 + 0 + 300 = 400, output 300,
    //   denom = cr(900)+cc(0)+input(100)=1000, cache_hit_ratio = 900/1000 = 0.9
    repo_usage_facet::insert(&pool, &uf("r2", "sess_hi", 100, 0, 900, 300))
        .await
        .unwrap();

    let state = AppState::new_for_tests(pool);
    let server = TestServer::new(router(state)).unwrap();
    let r = server.get("/v1/usage/baseline").await;
    r.assert_status_ok();
    let body = r.json::<Value>();
    let data = &body["data"];

    assert_eq!(data["session_count"].as_i64().unwrap(), 2);
    // Two values -> median is the midpoint (type-7 interpolation).
    // billed_tokens: [200, 400] -> median 300.
    assert_eq!(data["billed_tokens"]["median"].as_f64().unwrap(), 300.0);
    // output_tokens: [100, 300] -> median 200.
    assert_eq!(data["output_tokens"]["median"].as_f64().unwrap(), 200.0);
    // turns: [1, 1] -> median 1.
    assert_eq!(data["turns"]["median"].as_f64().unwrap(), 1.0);
    // cache_hit_ratio: [0.0, 0.9] -> median 0.45.
    let chr = data["cache_hit_ratio"]["median"].as_f64().unwrap();
    assert!((chr - 0.45).abs() < 1e-9, "got {chr}");
    // p25/p75 present (not null) when there is data.
    assert!(data["billed_tokens"]["p25"].as_f64().is_some());
    assert!(data["billed_tokens"]["p75"].as_f64().is_some());
}

#[tokio::test]
async fn baseline_endpoint_empty_store_returns_nulls() {
    let pool = empty_pool().await;
    let state = AppState::new_for_tests(pool);
    let server = TestServer::new(router(state)).unwrap();
    let r = server.get("/v1/usage/baseline").await;
    r.assert_status_ok();
    let body = r.json::<Value>();
    let data = &body["data"];
    assert_eq!(data["session_count"].as_i64().unwrap(), 0);
    assert!(data["billed_tokens"]["median"].is_null());
    assert!(data["cache_hit_ratio"]["median"].is_null());
}
```

Run: `cargo test --test api_usage_baseline 2>&1 | tail -20`
Expected: FAIL — route not registered (404 / handler missing).

- [ ] **Step 2: Add the handler** (`src/api/routes.rs`, immediately AFTER `session_usage`, which ends at line 419)

`repo_usage_facet` is already imported at the top of `routes.rs` (line 15). `UsageBaselineDto` + `BaselineStat` come in via the existing `use crate::api::dto::*;` glob (line 11). Add:

```rust
/// insight-redesign #6 — `GET /v1/usage/baseline`
///
/// Cross-session baseline: median (+ p25/p75) of each key usage metric over
/// ALL stored sessions that have usage_facet rows. SQLite has no MEDIAN(), so
/// the per-session metric rows are pulled and the quantiles computed in Rust.
/// `cache_hit_ratio` is averaged only over sessions where it is defined (a
/// session with a 0-token denominator carries None and is excluded from that
/// metric's distribution only).
pub async fn usage_baseline(State(pool): State<SqlitePool>) -> impl IntoResponse {
    let metrics = repo_usage_facet::per_session_metrics(&pool)
        .await
        .expect("db");

    let session_count = metrics.len() as i64;

    let cache_hit_vals: Vec<f64> = metrics.iter().filter_map(|m| m.cache_hit_ratio).collect();
    let billed_vals: Vec<f64> = metrics.iter().map(|m| m.billed_tokens as f64).collect();
    let turns_vals: Vec<f64> = metrics.iter().map(|m| m.turns as f64).collect();
    let output_vals: Vec<f64> = metrics.iter().map(|m| m.output_tokens as f64).collect();

    fn stat(values: &[f64]) -> BaselineStat {
        match repo_usage_facet::median_p25_p75(values) {
            Some(q) => BaselineStat {
                p25: Some(q.p25),
                median: Some(q.median),
                p75: Some(q.p75),
            },
            None => BaselineStat {
                p25: None,
                median: None,
                p75: None,
            },
        }
    }

    let data = UsageBaselineDto {
        session_count,
        cache_hit_ratio: stat(&cache_hit_vals),
        billed_tokens: stat(&billed_vals),
        turns: stat(&turns_vals),
        output_tokens: stat(&output_vals),
    };
    Json(Envelope {
        meta: ResponseMeta::now(),
        data,
    })
}
```

- [ ] **Step 3: Register the route** (`src/api/mod.rs`, immediately AFTER the `/v1/sessions/:id/usage` route block, which ends at line 121)

```rust
        .route("/v1/usage/baseline", get(routes::usage_baseline))
```

Place it BEFORE the `/v1/verification-runs/:id` route so all routes stay inside the `authed` group (the auth `.layer(...)` is appended after the route list). Concretely, insert it between the `usage` block (lines 118-121) and the `verification-runs/:id` block (lines 122-125).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test api_usage_baseline 2>&1 | tail -20`
Expected: PASS (2 tests). Then full suite: `cargo test 2>&1 | tail -15` — no regressions.

- [ ] **Step 5: Commit**

```bash
git add src/api/routes.rs src/api/mod.rs tests/api_usage_baseline.rs
git commit -m "feat(usage): GET /v1/usage/baseline cross-session median endpoint"
```

---

## Task 4: Frontend types + client (`webui/src/api/types.ts`, `client.ts`)

**Files:**
- Modify: `webui/src/api/types.ts` (add `BaselineStat` + `UsageBaselineDto`)
- Modify: `webui/src/api/client.ts` (add `getUsageBaseline`)
- Modify: `webui/src/api/__tests__/client.endpoints.test.ts` (add a case)

All `webui` commands run from the `webui/` directory. The existing slice-1 client test (`getSessionUsage`) lives in `client.endpoints.test.ts` and follows the `ENVELOPE` / `mockJson` / `fetchSpy` pattern — match it exactly.

- [ ] **Step 1: Add the TS types** (`webui/src/api/types.ts`, immediately AFTER the existing `SessionUsageDto` type, which ends at line 191)

```typescript
/** insight-redesign #6 — one baseline metric's quantile triple. All null
 *  when no session in the store has usage_facet rows for the metric. */
export type BaselineStat = {
  p25: number | null;
  median: number | null;
  p75: number | null;
};

/** insight-redesign #6 — cross-session usage baseline. Median (+ p25/p75) of
 *  each key metric across all stored sessions with usage_facet rows. The UI
 *  renders a measured session value as a delta against `*.median`
 *  ("vs your median"). */
export type UsageBaselineDto = {
  session_count: number;
  cache_hit_ratio: BaselineStat;
  billed_tokens: BaselineStat;
  turns: BaselineStat;
  output_tokens: BaselineStat;
};
```

- [ ] **Step 2: Write the failing client test** (`webui/src/api/__tests__/client.endpoints.test.ts`)

Add `getUsageBaseline` to the existing import block (lines 8-15, alongside `getSessionUsage`). Then add this `describe` after the `getSessionUsage` block (line 100):

```typescript
describe('getUsageBaseline', () => {
  it('hits GET /v1/usage/baseline and unwraps the envelope `data`', async () => {
    const expected = {
      session_count: 2,
      cache_hit_ratio: { p25: 0.0, median: 0.45, p75: 0.9 },
      billed_tokens: { p25: 200, median: 300, p75: 400 },
      turns: { p25: 1, median: 1, p75: 1 },
      output_tokens: { p25: 100, median: 200, p75: 300 },
    };
    fetchSpy.mockImplementation(mockJson(ENVELOPE(expected)));
    const out = await getUsageBaseline();
    expect(fetchSpy).toHaveBeenCalledWith('/v1/usage/baseline', expect.any(Object));
    expect(out).toEqual(expected);
  });
});
```

Run (from `webui/`): `npx vitest run src/api/__tests__/client.endpoints.test.ts 2>&1 | tail -15`
Expected: FAIL — `getUsageBaseline` is not exported.

- [ ] **Step 3: Add the client fn** (`webui/src/api/client.ts`, immediately AFTER `getSessionUsage`, which ends at line 89)

Add `UsageBaselineDto` to the type import block (lines 1-14, alongside `SessionUsageDto`). Then:

```typescript
/** insight-redesign #6 — cross-session usage baseline (no session id; this is
 *  a store-wide aggregate). The UI computes per-session deltas client-side. */
export const getUsageBaseline = (): Promise<UsageBaselineDto> =>
  jsonGet<UsageBaselineDto>('/v1/usage/baseline');
```

- [ ] **Step 4: Run the client test to verify it passes**

Run (from `webui/`): `npx vitest run src/api/__tests__/client.endpoints.test.ts 2>&1 | tail -15`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add webui/src/api/types.ts webui/src/api/client.ts webui/src/api/__tests__/client.endpoints.test.ts
git commit -m "feat(usage): frontend UsageBaselineDto type + getUsageBaseline client"
```

---

## Task 5: Frontend query hook (`webui/src/lib/queries.ts` + hook test)

**Files:**
- Modify: `webui/src/lib/queries.ts` (add `usageKeys` + `useUsageBaselineQuery`)
- Modify: `webui/src/lib/__tests__/queries.test.tsx` (add hook test)

The baseline is **not** session-scoped, so it does NOT belong under `sessionKeys` (which key by session id). Add a sibling `usageKeys` object.

- [ ] **Step 1: Write the failing hook test** (`webui/src/lib/__tests__/queries.test.tsx`)

Add `useUsageBaselineQuery` and `usageKeys` to the existing import from `../queries` (lines 17-23). Then add this `describe` block after the `useFindingsQuery` block (ends line 128, before the file's final `}`):

```typescript
describe('useUsageBaselineQuery', () => {
  it('caches the baseline under usageKeys.baseline()', async () => {
    const payload = {
      session_count: 2,
      cache_hit_ratio: { p25: 0.0, median: 0.45, p75: 0.9 },
      billed_tokens: { p25: 200, median: 300, p75: 400 },
      turns: { p25: 1, median: 1, p75: 1 },
      output_tokens: { p25: 100, median: 200, p75: 300 },
    };
    fetchSpy.mockResolvedValue(mockOk(ENVELOPE(payload)));
    const qc = createQueryClient();
    const { result } = renderHook(() => useUsageBaselineQuery(), { wrapper: wrap(qc) });
    await waitFor(() => expect(result.current.data).toEqual(payload));
    expect(qc.getQueryData(usageKeys.baseline())).toEqual(payload);
  });
});
```

Also extend the existing `sessionKeys` describe (lines 53-60) is not needed — `usageKeys` is asserted via `getQueryData` above.

Run (from `webui/`): `npx vitest run src/lib/__tests__/queries.test.tsx 2>&1 | tail -15`
Expected: FAIL — `useUsageBaselineQuery` / `usageKeys` not exported.

- [ ] **Step 2: Add the key + hook** (`webui/src/lib/queries.ts`)

Add `getUsageBaseline` to the `../api/client` import (lines 11-21) and `UsageBaselineDto` to the `../api/types` import (lines 22-32).

Add the `usageKeys` object immediately AFTER the `sessionKeys` object closes (line 47):

```typescript
/** insight-redesign #6 — store-wide usage baseline (not session-scoped). */
export const usageKeys = {
  baseline: () => ['usage', 'baseline'] as const,
};
```

Add the hook after `useSessionUsageQuery` (ends line 119):

```typescript
export function useUsageBaselineQuery(opts?: QOpts<UsageBaselineDto>) {
  return useQuery<UsageBaselineDto>({
    queryKey: usageKeys.baseline(),
    queryFn: () => getUsageBaseline(),
    staleTime: 60_000,
    ...opts,
  });
}
```

- [ ] **Step 3: Run the hook test + typecheck**

Run (from `webui/`): `npx vitest run src/lib/__tests__/queries.test.tsx 2>&1 | tail -15` → PASS
Run (from `webui/`): `npx tsc -b 2>&1 | tail -15` → clean
Then the full frontend suite (from `webui/`): `npx vitest run 2>&1 | tail -15` → no regressions.

- [ ] **Step 4: Commit**

```bash
git add webui/src/lib/queries.ts webui/src/lib/__tests__/queries.test.tsx
git commit -m "feat(usage): usageKeys + useUsageBaselineQuery hook + test"
```

---

## Task 6: Manual endpoint smoke + implementation notes

**Files:** Modify `docs/implementation-notes.html`

- [ ] **Step 1: Smoke the endpoint against the real dev DB**

The dev DB already has `usage_facet` rows from slice 1's re-ingest (CLAUDE.md operational note). Run:

```bash
cargo run --bin witmcc -- serve --bind 127.0.0.1 --port 7878 &
sleep 2 && curl -s http://127.0.0.1:7878/v1/usage/baseline | python3 -m json.tool
```

Expected: an envelope with `data.session_count > 1`, and `cache_hit_ratio.median`, `billed_tokens.median`, `turns.median`, `output_tokens.median` all populated with plausible numbers (cache_hit_ratio median near the high cache-hit values seen in slice 1, e.g. ~0.9+). Stop the server afterward (`kill %1`).

> Note: `--auth off` is the default (CLAUDE.md DEV-S19-08), so the curl needs no `Authorization` header. If the DB is empty, run `cargo run --bin witmcc -- init-db && cargo run --bin witmcc -- ingest --all` first.

- [ ] **Step 2: Document in implementation-notes**

Append a new `§` entry to `docs/implementation-notes.html` (match the existing section markup) covering: the new `GET /v1/usage/baseline` endpoint; the **median-in-Rust** decision (SQLite has no `MEDIAN()`), the type-7 interpolation method, and the `per_session_metrics` query that computes each session's metric the SAME way `/v1/sessions/:id/usage` does (so the baseline is comparable); the choice of a separate endpoint over a folded `vs_baseline` field (frontend computes the delta client-side); and that `cache_hit_ratio`'s distribution excludes sessions with a 0-token denominator (None), while `session_count` counts all sessions with usage rows. Note **no migration** — this slice only reads slice 1's `usage_facet` table.

- [ ] **Step 3: Commit**

```bash
git add docs/implementation-notes.html
git commit -m "docs(usage): implementation notes for cross-session baseline slice"
```

---

## Done criteria

- `GET /v1/usage/baseline` returns the median (+ p25/p75) of `cache_hit_ratio`, `billed_tokens`, `turns`, `output_tokens` across all sessions with `usage_facet` rows, in the standard Envelope, with `session_count`.
- Median computed in Rust (type-7 interpolation), unit-tested with odd/even/single/empty cases; per-session metric query asserted to return one row per session with the same arithmetic as the per-session usage endpoint.
- Endpoint integration test seeds two sessions with known values and asserts the exact medians (300 billed / 200 output / 0.45 cache-hit) plus the empty-store null case.
- Frontend `getUsageBaseline` + `useUsageBaselineQuery` wired with client + hook tests; `npx tsc -b` clean.
- All new tests pass; `cargo test` + `npx vitest run` (from `webui/`) clean; no regressions.
- This unblocks the redesign's proposal A: the KpiStrip "vs your median" delta UI (a later component slice) consumes `useUsageBaselineQuery` alongside `useSessionUsageQuery`.
```
