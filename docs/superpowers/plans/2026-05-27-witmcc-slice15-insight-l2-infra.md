# Slice-15 Implementation Plan — Insight L2 Infrastructure

**Spec:** `docs/superpowers/specs/2026-05-27-witmcc-slice15-insight-l2-infra-design.md`
**Architecture:** `docs/superpowers/specs/2026-05-27-witmcc-insight-engine-architecture.md`
**Branch:** `slice15-insight-l2-infra`

---

## Phase 0 — Branch & baseline

| 0a | Cut `slice15-insight-l2-infra` off slice-14 merge |
| 0b | Record baseline cargo + vitest counts |
| 0c | Confirm `ANTHROPIC_API_KEY` available for smoke (note in scratch; smoke skipped if absent) |

---

## Phase 1 — Red-locking tests

### Task 1 — Schemas

**Files:** `tests/migration_judge_cache_schema.rs`, `tests/migration_findings_pending_schema.rs`.

```rust
#[tokio::test]
async fn migration_creates_judge_verdict_cache_table() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let cols: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM pragma_table_info('judge_verdict_cache')"
    ).fetch_all(&pool).await.unwrap();
    for c in ["cache_key","category","model_id","prompt_template_version",
              "evidence_hash","verdict_json","created_at","last_hit_at"] {
        assert!(cols.iter().any(|n| n == c), "missing column {c}");
    }
}

#[tokio::test]
async fn migration_creates_findings_pending_judge_table() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let cols: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM pragma_table_info('findings_pending_judge')"
    ).fetch_all(&pool).await.unwrap();
    for c in ["candidate_id","session_id","category","confidence_l1",
              "evidence_refs","evidence_projection","queued_at","last_attempt_at","attempts"] {
        assert!(cols.iter().any(|n| n == c), "missing column {c}");
    }
}
```

### Task 2 — Trait compile-only red

**Files:** `tests/judge_trait_shape.rs`.

```rust
#[tokio::test]
async fn noop_judge_returns_disabled() {
    use witmcc::insight::judge::providers::NoopJudge;
    use witmcc::insight::judge::{JudgePrompt, JudgeError, JudgeProvider};
    let j = NoopJudge;
    let p = JudgePrompt {
        category: "risky_action".into(),
        candidate_id: "cand_1".into(),
        evidence_projection: serde_json::json!({}),
        system_template: "<placeholder>".into(),
    };
    let r = j.judge(p).await;
    assert!(matches!(r, Err(JudgeError::Disabled)));
}
```

### Task 3 — Fixture judge

**Files:** `tests/fixtures/judge/scenario_a.json`, `tests/judge_fixture_provider.rs`.

```json
{
  "risky_action||sha256:abc": { "promote": true,  "confidence_l2": 0.8, "reason": "destructive without prompt", "mismatch_summary": null },
  "risky_action||sha256:def": { "promote": false, "confidence_l2": 0.0, "reason": "user explicitly asked",      "mismatch_summary": null }
}
```

```rust
#[tokio::test]
async fn fixture_judge_returns_recorded_verdicts() {
    let j = witmcc::insight::judge::providers::FixtureJudge::load(
        std::path::Path::new("tests/fixtures/judge/scenario_a.json")
    ).unwrap();
    // Build prompt whose evidence_projection hashes to "abc"
    // (helper: synth_projection_hashing_to("abc"))
    // assert verdict.promote == true
}
```

### Task 4 — Cache wrapper

**Files:** `tests/judge_cache_wrapper.rs`.

```rust
#[tokio::test]
async fn cached_provider_serves_from_cache_on_second_call() {
    let pool = test_pool().await;
    let inner = ScriptedJudge::new(vec![ok_promote(0.8)]);  // returns Ok once, then panics if called again
    let prov = witmcc::insight::judge::cache::CachedProvider::new(inner, pool.clone());
    let p = synth_prompt();
    let _ = prov.judge(p.clone()).await.unwrap();
    let _ = prov.judge(p.clone()).await.unwrap();  // 2nd call must hit cache
}

#[tokio::test]
async fn cache_key_invalidates_when_prompt_template_changes() {
    // ...
}
```

### Task 5 — Budget guard

**Files:** `tests/judge_budget.rs`.

```rust
#[tokio::test]
async fn budget_guard_exhausts_after_n_calls() {
    let inner = ScriptedJudge::new(vec![ok_promote(0.5); 10]);
    let g = witmcc::insight::judge::budget::BudgetGuard::new(inner, 3);
    assert!(g.judge(synth_prompt()).await.is_ok());
    assert!(g.judge(synth_prompt()).await.is_ok());
    assert!(g.judge(synth_prompt()).await.is_ok());
    assert!(matches!(g.judge(synth_prompt()).await,
                     Err(witmcc::insight::judge::JudgeError::BudgetExhausted)));
}
```

### Task 6 — Pipeline integration with judge

**Files:** `tests/insight_pipeline_l2.rs`.

A test-only `noop_category` extractor (in `cfg(test)`) ships in this slice for path coverage:

```rust
// src/insight/extractors/noop_test.rs  (cfg(test))
pub struct NoopTestExtractor;
impl InsightExtractor for NoopTestExtractor {
    fn category(&self) -> &'static str { "noop_test" }
    fn floor(&self) -> f32 { 0.0 }
    fn promotion_policy(&self) -> PromotionPolicy { PromotionPolicy::Never }   // always queue via judge
    fn extract(&self, view: &SessionInsightView) -> Vec<FindingCandidate> {
        // 1 candidate per session
        if view.events.is_empty() { return Vec::new(); }
        vec![FindingCandidate {
            category: "noop_test".into(),
            session_id: view.session_id.into(),
            confidence_l1: 0.5,
            evidence_refs: vec![view.events[0].event_id.clone()],
            evidence_projection: serde_json::json!({"first_event_id": view.events[0].event_id}),
        }]
    }
}
```

```rust
#[tokio::test]
async fn pipeline_queues_noop_test_when_judge_disabled() {
    let pool = test_pool_with_seeded_session().await;
    let runtime = build_runtime_with_noop_judge();
    pipeline::run_extractors_with_runtime(&pool, "sess_t", &runtime).await.unwrap();
    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM findings_pending_judge")
        .fetch_one(&pool).await.unwrap();
    assert!(pending >= 1);
}

#[tokio::test]
async fn pipeline_drains_pending_on_next_run_with_fixture_judge() {
    let pool = test_pool_with_seeded_session().await;
    let noop_runtime = build_runtime_with_noop_judge();
    pipeline::run_extractors_with_runtime(&pool, "sess_t", &noop_runtime).await.unwrap();

    let fixture_runtime = build_runtime_with_fixture_judge(
        "tests/fixtures/judge/scenario_a.json"
    );
    pipeline::run_extractors_with_runtime(&pool, "sess_t", &fixture_runtime).await.unwrap();

    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM findings_pending_judge")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(pending, 0);  // all drained
}

#[tokio::test]
async fn pipeline_records_budget_exhaustion_in_health() {
    let pool = test_pool_with_many_seeded_sessions().await;
    let runtime = build_runtime_with_fixture_judge_and_budget(1);
    pipeline::run_extractors_with_runtime(&pool, "sess_a", &runtime).await.unwrap();
    pipeline::run_extractors_with_runtime(&pool, "sess_b", &runtime).await.unwrap();
    let counters = runtime.metrics_snapshot();
    assert!(counters.judge_budget_exhaustions_24h >= 1);
}
```

### Task 7 — Health endpoint extension

**Files:** `tests/api_health_insight.rs`.

```rust
#[tokio::test]
async fn health_endpoint_includes_insight_block() {
    let pool = test_pool().await;
    let server = axum_test::TestServer::new(witmcc::api::build_router(pool)).unwrap();
    let r = server.get("/v1/health").await;
    let body: serde_json::Value = r.json();
    assert!(body["insight"].is_object());
    for k in ["judge_kind","judge_calls_24h","judge_pending_count",
              "judge_cache_hits_24h","judge_cache_misses_24h",
              "judge_budget_exhaustions_24h"] {
        assert!(body["insight"][k].is_number() || body["insight"][k].is_string());
    }
}
```

**Commit 1:** `test(slice-15): red-locking tests for judge pipeline, cache, budget, pending queue, health`

---

## Phase 2 — Migrations + repos

| 8  | `migrations/20260531120000_0009_judge_cache.sql` | schema test green |
| 9  | `migrations/20260531180000_0010_findings_pending.sql` | schema test green |
| 10 | `src/db/repo_judge_cache.rs` (`get/put/touch/sweep_older_than`) | repo roundtrip test |
| 11 | `src/db/repo_findings_pending.rs` (`enqueue/dequeue/list_session`) | repo roundtrip test |

**Commit 2:** `feat(db): 0009_judge_cache + 0010_findings_pending migrations + repos`

---

## Phase 3 — Trait + types + NoopJudge + FixtureJudge

| 12 | `src/insight/judge/mod.rs` + `types.rs` + `errors.rs` per spec §3 |
| 13 | `src/insight/judge/providers/noop.rs` |
| 14 | `src/insight/judge/providers/fixture.rs` |
| 15 | Update `src/insight/mod.rs` to expose the new module |

Judge trait shape test + fixture provider test green.

**Commit 3:** `feat(insight): JudgeProvider trait + NoopJudge + FixtureJudge`

---

## Phase 4 — AnthropicJudge

| 16 | `src/insight/judge/prompts/judge_v1.txt` — system template per spec §9 |
| 17 | `src/insight/judge/providers/anthropic.rs` — hand-rolled reqwest call |
| 18 | Tests: an `anthropic_judge_shape.rs` test that uses a mocked HTTP server (`mockito` if added; otherwise an in-process router that responds with a canned shape) to verify request body + response parse |

**Commit 4:** `feat(insight): AnthropicJudge implementation + prompt_v1 template`

---

## Phase 5 — Cache + Budget composition

| 19 | `src/insight/judge/cache.rs` + `CachedProvider` per spec §5 | cache wrapper test green |
| 20 | `src/insight/judge/budget.rs` + `BudgetGuard` per spec §4 | budget test green |
| 21 | Compose function `build_judge_runtime(kind, pool, budget, fixture_path) -> Arc<dyn JudgeProvider>` |

**Commit 5:** `feat(insight): CachedProvider + BudgetGuard composition`

---

## Phase 6 — Pipeline integration + CLI + health

| 22 | Extend `src/insight/pipeline.rs` per spec §6: extractors → judge for `Never` / `IfAbove` policies → queue on budget exhaust / disabled. Drain pending queue at start of next run. |
| 23 | Add `--judge`, `--judge-budget`, `--judge-fixture-path` flags on `witmcc serve` (clap derive) |
| 24 | Extend `/v1/health` handler per spec §8. In-memory counters in `src/api/metrics.rs`. |
| 25 | Wire metrics increments through `CachedProvider` + `BudgetGuard` + pipeline queue write |

**Commit 6:** `feat(cli): --judge flags + pipeline integration + /v1/health.insight.*`

---

## Phase 7 — Smoke + verification

```
Smoke — slice-15 (no API key path)

[ ] witmcc serve --judge none --port 4337 &
[ ] ingest aac68973
[ ] rebuild via curl trigger or wait for ingest auto-rebuild
[ ] curl -s http://127.0.0.1:4337/v1/health | jq '.insight'
    # judge_kind: "noop", judge_pending_count: 0 (no L2 categories yet)
[ ] curl -s http://127.0.0.1:4337/v1/findings | jq 'length'
    # Same as slice-14 (only L1 categories)
```

```
Smoke — slice-15 (fixture path)

[ ] mkdir -p tests/fixtures/judge/
[ ] put scenario_a.json
[ ] witmcc serve --judge fixture --judge-fixture-path tests/fixtures/judge/scenario_a.json --port 4337 &
[ ] noop_test extractor is cfg(test) only — production rebuild has no L2 candidates
[ ] curl -s http://127.0.0.1:4337/v1/health | jq '.insight.judge_kind'
    # "fixture"
```

```
Smoke — slice-15 (anthropic path, optional)

[ ] export ANTHROPIC_API_KEY=...
[ ] witmcc serve --judge anthropic --judge-budget 2 --port 4337 &
[ ] curl -s http://127.0.0.1:4337/v1/health | jq '.insight.judge_kind'
    # "anthropic"
[ ] No L2 categories yet — health.judge_calls_24h stays 0. Real exercise comes in slice-16 smoke.
```

```
Verification — slice-15

- cargo test count: baseline (post slice-14) → expected + 16..22
- aac68973 findings: unchanged from slice-14 (no new categories)
- /v1/health includes insight block
- A new failing-fast boot test: spawn `witmcc serve --judge anthropic` without ANTHROPIC_API_KEY ⇒ exits non-zero
```

---

## Phase 8 — PR

Title: `feat(slice-15): Insight L2 judge infrastructure (trait, providers, cache, budget, queue)`. Implementation notes update.
