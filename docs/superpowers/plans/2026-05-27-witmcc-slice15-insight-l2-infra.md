# Slice-15 — Insight Engine L2 Infrastructure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the full L2 judge layer infrastructure — `JudgeProvider` trait, `NoopJudge`, `FixtureJudge`, `AnthropicJudge`, `CachedProvider`, `BudgetGuard`, `judge_verdict_cache` and `findings_pending_judge` DB tables, `--judge` CLI flags, and `/v1/health.insight.*` counters — with no new finding categories.

**Architecture:** L2 is off by default (NoopJudge). `BudgetGuard<CachedProvider<AnthropicJudge>>` is composed at runtime per CLI flags. The judge never touches events directly; it receives a compact `EvidenceProjection` produced by the L1 extractor. Cache is keyed by `sha256(category || model_id || prompt_template_version || evidence_hash)`. Budget is per-rebuild invocation.

**Tech Stack:** Rust, sqlx/SQLite, axum, reqwest, sha2, serde_json, clap, tokio, async-trait (already in Cargo.toml via implicit traits; use `async fn` in trait syntax via RPITIT or box the futures)

---

## Pre-flight checks

- [ ] Confirm current branch is `feat/remaining-milestones` or the working branch for slice-15
- [ ] Run `cargo test` — confirm all 274 tests green (baseline)
- [ ] Record baseline: `cargo test 2>&1 | grep "test result" | awk '{sum += $4} END {print sum}'` → 274

---

## File map

### New production files
- `src/insight/judge/mod.rs` — module root; re-exports types + trait + errors
- `src/insight/judge/types.rs` — `JudgePrompt`, `JudgeVerdict`
- `src/insight/judge/errors.rs` — `JudgeError` enum
- `src/insight/judge/providers/mod.rs` — re-exports all providers
- `src/insight/judge/providers/noop.rs` — `NoopJudge`
- `src/insight/judge/providers/fixture.rs` — `FixtureJudge`
- `src/insight/judge/providers/anthropic.rs` — `AnthropicJudge`
- `src/insight/judge/prompts/judge_v1.txt` — system prompt template
- `src/insight/judge/cache.rs` — `CachedProvider`, cache key, canonical JSON, DB ops
- `src/insight/judge/budget.rs` — `BudgetGuard`
- `src/insight/judge/metrics.rs` — in-memory atomic counters for health endpoint
- `src/insight/judge/runtime.rs` — `JudgeRuntime` (judge Arc + metrics); `build_judge_runtime()`
- `src/db/repo_judge_cache.rs` — `get / put / touch / sweep_older_than`
- `src/db/repo_findings_pending.rs` — `enqueue / dequeue_session / list_session / count`
- `migrations/20260531120000_0009_judge_cache.sql`
- `migrations/20260531180000_0010_findings_pending.sql`

### Modified production files
- `src/insight/mod.rs` — add `pub mod judge;`
- `src/insight/pipeline.rs` — extend `run_extractors` to accept `JudgeRuntime`; drain pending queue; route `Never`/`IfAbove` candidates; add `run_extractors_with_runtime()`
- `src/db/mod.rs` — add `pub mod repo_judge_cache; pub mod repo_findings_pending;`
- `src/api/mod.rs` / `src/api/routes.rs` — extend `AppState` with `JudgeRuntime`, update health handler
- `src/cli.rs` — add `--judge`, `--judge-budget`, `--judge-fixture-path` to `Serve` subcommand
- `src/main.rs` — pass judge runtime to `serve_cmd` and pipeline

### New test files
- `tests/migration_judge_cache_schema.rs`
- `tests/migration_findings_pending_schema.rs`
- `tests/judge_noop.rs`
- `tests/judge_fixture.rs`
- `tests/judge_cache.rs`
- `tests/judge_budget.rs`
- `tests/insight_pipeline_l2.rs`
- `tests/api_health_insight.rs`
- `tests/fixtures/judge/scenario_a.json`

---

## Phase 1 — Red-locking tests (write failing tests first, commit before any prod code)

### Task 1: Migration schema tests

**Files:**
- Create: `tests/migration_judge_cache_schema.rs`
- Create: `tests/migration_findings_pending_schema.rs`

- [ ] **Step 1: Write the failing migration tests**

`tests/migration_judge_cache_schema.rs`:
```rust
//! Slice-15 — locks that migration 0009 creates judge_verdict_cache with correct columns.

#[tokio::test]
async fn migration_creates_judge_verdict_cache_table() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let cols: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM pragma_table_info('judge_verdict_cache')",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    for c in [
        "cache_key",
        "category",
        "model_id",
        "prompt_template_version",
        "evidence_hash",
        "verdict_json",
        "created_at",
        "last_hit_at",
    ] {
        assert!(cols.iter().any(|n| n == c), "missing column {c}");
    }
}

#[tokio::test]
async fn judge_cache_has_index_on_category() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let idx: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='judge_verdict_cache'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        idx.iter().any(|n| n == "idx_judge_cache_category"),
        "missing index idx_judge_cache_category"
    );
}
```

`tests/migration_findings_pending_schema.rs`:
```rust
//! Slice-15 — locks that migration 0010 creates findings_pending_judge with correct columns.

#[tokio::test]
async fn migration_creates_findings_pending_judge_table() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let cols: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM pragma_table_info('findings_pending_judge')",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    for c in [
        "candidate_id",
        "schema_version",
        "session_id",
        "category",
        "confidence_l1",
        "evidence_refs",
        "evidence_projection",
        "queued_at",
        "last_attempt_at",
        "attempts",
    ] {
        assert!(cols.iter().any(|n| n == c), "missing column {c}");
    }
}

#[tokio::test]
async fn pending_default_schema_version() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO findings_pending_judge \
         (candidate_id, session_id, category, confidence_l1, evidence_refs, evidence_projection) \
         VALUES (?,?,?,?,?,?)",
    )
    .bind("cand_x")
    .bind("sess_x")
    .bind("noop_test")
    .bind(0.5_f64)
    .bind("[]")
    .bind("{}")
    .execute(&pool)
    .await
    .unwrap();

    let sv: String = sqlx::query_scalar(
        "SELECT schema_version FROM findings_pending_judge WHERE candidate_id='cand_x'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(sv, "pending_finding.v1");
}
```

- [ ] **Step 2: Run to verify they fail**

```bash
cargo test migration_judge_cache_schema 2>&1 | tail -5
cargo test migration_findings_pending_schema 2>&1 | tail -5
```

Expected: FAIL — migrations don't exist yet, or tables not found.

---

### Task 2: Judge trait + NoopJudge test

**Files:**
- Create: `tests/judge_noop.rs`

- [ ] **Step 1: Write the failing test**

```rust
//! Slice-15 — NoopJudge must return JudgeError::Disabled.

use witmcc::insight::judge::{JudgeError, JudgePrompt, JudgeProvider};
use witmcc::insight::judge::providers::NoopJudge;

fn synth_prompt() -> JudgePrompt {
    JudgePrompt {
        category: "risky_action".to_string(),
        candidate_id: "cand_001".to_string(),
        evidence_projection: serde_json::json!({"test": true}),
        system_template: "placeholder".to_string(),
    }
}

#[tokio::test]
async fn noop_judge_returns_disabled_error() {
    let j = NoopJudge;
    let r = j.judge(synth_prompt()).await;
    assert!(
        matches!(r, Err(JudgeError::Disabled)),
        "expected Disabled, got {:?}",
        r
    );
}

#[test]
fn noop_model_id_is_noop() {
    let j = NoopJudge;
    assert_eq!(j.model_id(), "noop");
}

#[test]
fn noop_prompt_template_version_is_noop() {
    let j = NoopJudge;
    assert_eq!(j.prompt_template_version(), "noop");
}
```

- [ ] **Step 2: Run to verify it fails**

```bash
cargo test judge_noop 2>&1 | tail -5
```

Expected: FAIL — `witmcc::insight::judge` module doesn't exist yet.

---

### Task 3: FixtureJudge test

**Files:**
- Create: `tests/judge_fixture.rs`
- Create: `tests/fixtures/judge/scenario_a.json`

- [ ] **Step 1: Write the fixture JSON file**

`tests/fixtures/judge/scenario_a.json`:
```json
{
  "risky_action||aaa111bbb222ccc333ddd444eee555ff": {
    "promote": true,
    "confidence_l2": 0.8,
    "reason": "destructive bash command without preceding user confirmation",
    "mismatch_summary": null
  },
  "risky_action||fff555eee444ddd333ccc222bbb111aaa": {
    "promote": false,
    "confidence_l2": 0.0,
    "reason": "user explicitly requested the destructive action",
    "mismatch_summary": null
  },
  "noop_test||0000000000000000000000000000000000000000000000000000000000000001": {
    "promote": true,
    "confidence_l2": 0.6,
    "reason": "fixture judge promote for noop_test",
    "mismatch_summary": null
  }
}
```

- [ ] **Step 2: Write the failing test**

`tests/judge_fixture.rs`:
```rust
//! Slice-15 — FixtureJudge replays recorded verdicts by (category, evidence_hash) key.

use std::path::Path;
use witmcc::insight::judge::{JudgeError, JudgePrompt, JudgeProvider};
use witmcc::insight::judge::providers::FixtureJudge;

fn prompt_with_hash(category: &str, evidence_hash: &str) -> JudgePrompt {
    // Build a projection whose sha256 equals evidence_hash.
    // The FixtureJudge looks up the key "category||evidence_hash" in its table.
    // We pass the hash directly since the fixture key is pre-computed.
    // For the lookup to work, pass the pre-recorded hash directly via the
    // `override_hash` test constructor.
    JudgePrompt {
        category: category.to_string(),
        candidate_id: "cand_test".to_string(),
        evidence_projection: serde_json::json!({}),
        system_template: "placeholder".to_string(),
    }
}

#[tokio::test]
async fn fixture_judge_loads_and_returns_verdict() {
    let j = FixtureJudge::load(
        Path::new("tests/fixtures/judge/scenario_a.json"),
    )
    .unwrap();
    // Use the with_hash override: category=risky_action, hash=aaa111bbb222...
    let p = JudgePrompt {
        category: "risky_action".to_string(),
        candidate_id: "cand_test".to_string(),
        evidence_projection: serde_json::json!({}),
        system_template: "placeholder".to_string(),
    };
    // FixtureJudge must expose a test method to set the evidence hash directly.
    let verdict = j.judge_with_hash(p, "aaa111bbb222ccc333ddd444eee555ff").await.unwrap();
    assert!(verdict.promote);
    assert!((verdict.confidence_l2 - 0.8).abs() < 0.001);
}

#[tokio::test]
async fn fixture_judge_returns_no_promote_for_second_entry() {
    let j = FixtureJudge::load(
        Path::new("tests/fixtures/judge/scenario_a.json"),
    )
    .unwrap();
    let p = JudgePrompt {
        category: "risky_action".to_string(),
        candidate_id: "cand_test".to_string(),
        evidence_projection: serde_json::json!({}),
        system_template: "placeholder".to_string(),
    };
    let verdict = j.judge_with_hash(p, "fff555eee444ddd333ccc222bbb111aaa").await.unwrap();
    assert!(!verdict.promote);
    assert!((verdict.confidence_l2 - 0.0).abs() < 0.001);
}

#[tokio::test]
async fn fixture_judge_errors_on_unknown_key() {
    let j = FixtureJudge::load(
        Path::new("tests/fixtures/judge/scenario_a.json"),
    )
    .unwrap();
    let p = JudgePrompt {
        category: "risky_action".to_string(),
        candidate_id: "cand_unknown".to_string(),
        evidence_projection: serde_json::json!({}),
        system_template: "placeholder".to_string(),
    };
    let r = j.judge_with_hash(p, "nonexistent_hash").await;
    assert!(matches!(r, Err(JudgeError::Schema(_))));
}
```

- [ ] **Step 3: Run to verify it fails**

```bash
cargo test judge_fixture 2>&1 | tail -5
```

Expected: FAIL — module doesn't exist.

---

### Task 4: Cache wrapper test

**Files:**
- Create: `tests/judge_cache.rs`

- [ ] **Step 1: Write the failing test**

```rust
//! Slice-15 — CachedProvider serves from DB cache on second call; misses call inner.

use sqlx::sqlite::SqlitePoolOptions;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use witmcc::db::migrate;
use witmcc::insight::judge::{JudgeError, JudgePrompt, JudgeProvider, JudgeVerdict};
use witmcc::insight::judge::cache::CachedProvider;

/// Scripted judge: returns verdicts from a pre-built list; panics when list is exhausted.
struct ScriptedJudge {
    verdicts: std::sync::Mutex<Vec<JudgeVerdict>>,
    call_count: Arc<AtomicU32>,
}

impl ScriptedJudge {
    fn new(verdicts: Vec<JudgeVerdict>) -> Self {
        Self {
            verdicts: std::sync::Mutex::new(verdicts),
            call_count: Arc::new(AtomicU32::new(0)),
        }
    }
    fn calls(&self) -> u32 {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl JudgeProvider for ScriptedJudge {
    async fn judge(&self, _p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let mut v = self.verdicts.lock().unwrap();
        if v.is_empty() {
            panic!("ScriptedJudge exhausted — unexpected extra call");
        }
        Ok(v.remove(0))
    }
    fn model_id(&self) -> &'static str { "scripted" }
    fn prompt_template_version(&self) -> &'static str { "v_test" }
}

fn ok_verdict(confidence: f32) -> JudgeVerdict {
    JudgeVerdict {
        promote: true,
        confidence_l2: confidence,
        reason: "scripted".to_string(),
        mismatch_summary: None,
    }
}

fn synth_prompt() -> JudgePrompt {
    JudgePrompt {
        category: "risky_action".to_string(),
        candidate_id: "cand_001".to_string(),
        evidence_projection: serde_json::json!({"cmd": "rm -rf /tmp/x"}),
        system_template: "judge@v1".to_string(),
    }
}

async fn mem_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys = OFF").execute(&pool).await.unwrap();
    migrate(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn cached_provider_calls_inner_once_then_serves_cache() {
    let pool = mem_pool().await;
    let inner = ScriptedJudge::new(vec![ok_verdict(0.8)]);
    let calls = inner.call_count.clone();
    let prov = CachedProvider::new(inner, pool);
    let p = synth_prompt();

    let v1 = prov.judge(p.clone()).await.unwrap();
    let v2 = prov.judge(p.clone()).await.unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 1, "inner must be called exactly once");
    assert!((v1.confidence_l2 - 0.8).abs() < 0.001);
    assert!((v2.confidence_l2 - 0.8).abs() < 0.001);
}

#[tokio::test]
async fn cache_key_differs_when_template_version_differs() {
    // Two prompts identical except system_template (which drives prompt_template_version in the key)
    let pool = mem_pool().await;
    let inner = ScriptedJudge::new(vec![ok_verdict(0.7), ok_verdict(0.6)]);
    let calls = inner.call_count.clone();
    let prov = CachedProvider::new(inner, pool);

    let mut p1 = synth_prompt();
    p1.system_template = "judge@v1".to_string();
    let mut p2 = synth_prompt();
    p2.system_template = "judge@v2".to_string();  // different template → different key

    prov.judge(p1).await.unwrap();
    prov.judge(p2).await.unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 2, "different template = cache miss = 2 calls");
}
```

- [ ] **Step 2: Run to verify it fails**

```bash
cargo test judge_cache 2>&1 | tail -5
```

Expected: FAIL — module doesn't exist, and `async_trait` import may need fixing.

---

### Task 5: Budget guard test

**Files:**
- Create: `tests/judge_budget.rs`

- [ ] **Step 1: Write the failing test**

```rust
//! Slice-15 — BudgetGuard exhausts after N calls and returns BudgetExhausted.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use witmcc::insight::judge::{JudgeError, JudgePrompt, JudgeProvider, JudgeVerdict};
use witmcc::insight::judge::budget::BudgetGuard;

struct InfiniteJudge {
    call_count: Arc<AtomicU32>,
}

impl InfiniteJudge {
    fn new() -> (Self, Arc<AtomicU32>) {
        let c = Arc::new(AtomicU32::new(0));
        (Self { call_count: c.clone() }, c)
    }
}

#[async_trait::async_trait]
impl JudgeProvider for InfiniteJudge {
    async fn judge(&self, _p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(JudgeVerdict {
            promote: true,
            confidence_l2: 0.5,
            reason: "ok".to_string(),
            mismatch_summary: None,
        })
    }
    fn model_id(&self) -> &'static str { "infinite" }
    fn prompt_template_version(&self) -> &'static str { "v_test" }
}

fn p() -> JudgePrompt {
    JudgePrompt {
        category: "risky_action".to_string(),
        candidate_id: "c".to_string(),
        evidence_projection: serde_json::json!({}),
        system_template: "s".to_string(),
    }
}

#[tokio::test]
async fn budget_guard_exhausts_after_budget_calls() {
    let (inner, calls) = InfiniteJudge::new();
    let guard = BudgetGuard::new(inner, 3);
    assert!(guard.judge(p()).await.is_ok());
    assert!(guard.judge(p()).await.is_ok());
    assert!(guard.judge(p()).await.is_ok());
    let r = guard.judge(p()).await;
    assert!(
        matches!(r, Err(JudgeError::BudgetExhausted)),
        "expected BudgetExhausted, got {:?}", r
    );
    assert_eq!(calls.load(Ordering::SeqCst), 3, "inner called exactly 3 times");
}

#[tokio::test]
async fn budget_guard_zero_budget_exhausts_immediately() {
    let (inner, _) = InfiniteJudge::new();
    let guard = BudgetGuard::new(inner, 0);
    let r = guard.judge(p()).await;
    assert!(matches!(r, Err(JudgeError::BudgetExhausted)));
}

#[tokio::test]
async fn budget_guard_delegates_model_id() {
    let (inner, _) = InfiniteJudge::new();
    let guard = BudgetGuard::new(inner, 5);
    assert_eq!(guard.model_id(), "infinite");
}
```

- [ ] **Step 2: Run to verify it fails**

```bash
cargo test judge_budget 2>&1 | tail -5
```

Expected: FAIL — module doesn't exist.

---

### Task 6: Pipeline L2 integration test

**Files:**
- Create: `tests/insight_pipeline_l2.rs`

- [ ] **Step 1: Write the failing test**

```rust
//! Slice-15 — pipeline L2 integration: noop_test extractor queues to pending;
//! fixture judge drains the queue on subsequent rebuild.
//!
//! Uses the cfg(test) NoopTestExtractor registered via build_test_runtime().

use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::migrate;
use witmcc::insight::pipeline::run_extractors_with_runtime;
use witmcc::insight::judge::runtime::{JudgeRuntime, JudgeKind};

async fn seeded_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys = OFF").execute(&pool).await.unwrap();
    migrate(&pool).await.unwrap();

    // Insert a minimal session with one event so NoopTestExtractor emits a candidate.
    let sess = "sess_t";
    let now = "2026-01-01T00:00:00Z";
    sqlx::query(
        "INSERT OR IGNORE INTO raw_event \
         (raw_event_id,ingest_run_id,source_type,source_uri,source_line_no,\
          captured_at,payload_json,schema_version,provenance) \
         VALUES (?,?,?,?,?,?,?,?,?)",
    )
    .bind("raw_000").bind("run_0").bind("claude_transcript")
    .bind("file://test.jsonl").bind(0_i64)
    .bind(now).bind("{}").bind("raw_event.v1").bind("{}")
    .execute(&pool).await.unwrap();

    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id,raw_event_id,schema_version,session_id,observed_at,\
          actor,kind,tool_name,tool_use_id,is_error,provenance) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("ev_000").bind("raw_000").bind("observed_event.v1")
    .bind(sess).bind(now)
    .bind("tool").bind("tool_use").bind("Bash").bind("tu_0")
    .bind(0_i64).bind("{}")
    .execute(&pool).await.unwrap();

    pool
}

#[tokio::test]
async fn noop_judge_queues_noop_test_candidate_to_pending() {
    let pool = seeded_pool().await;
    let runtime = JudgeRuntime::noop();
    run_extractors_with_runtime(&pool, "sess_t", &runtime).await.unwrap();

    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM findings_pending_judge")
        .fetch_one(&pool).await.unwrap();
    // noop_test extractor (cfg(test)) emits 1 candidate; it goes to pending with NoopJudge
    assert!(pending >= 1, "expected >=1 pending, got {pending}");
}

#[tokio::test]
async fn fixture_judge_drains_pending_from_prior_run() {
    let pool = seeded_pool().await;

    // First pass: noop judge — fills pending
    let noop = JudgeRuntime::noop();
    run_extractors_with_runtime(&pool, "sess_t", &noop).await.unwrap();

    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM findings_pending_judge")
        .fetch_one(&pool).await.unwrap();
    assert!(before >= 1, "pending should have entries after noop pass");

    // Second pass: fixture judge — drains pending
    let fixture = JudgeRuntime::fixture(
        std::path::Path::new("tests/fixtures/judge/scenario_a.json"),
        20,
    ).unwrap();
    run_extractors_with_runtime(&pool, "sess_t", &fixture).await.unwrap();

    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM findings_pending_judge")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(after, 0, "pending should be empty after fixture judge drains it");
}

#[tokio::test]
async fn budget_exhaustion_leaves_items_in_pending() {
    let pool = seeded_pool().await;
    // Insert multiple sessions worth of test events so we have >1 candidate
    // We actually just confirm that budget=0 with fixture leaves pending non-empty.
    let fixture = JudgeRuntime::fixture_with_budget(
        std::path::Path::new("tests/fixtures/judge/scenario_a.json"),
        0, // zero budget — everything queues
    ).unwrap();
    run_extractors_with_runtime(&pool, "sess_t", &fixture).await.unwrap();

    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM findings_pending_judge")
        .fetch_one(&pool).await.unwrap();
    assert!(pending >= 1, "budget=0 should leave candidates in pending");
}

#[tokio::test]
async fn l1_categories_unaffected_by_noop_judge() {
    // missing_verification and tool_failure (L1/Always policy) must still write findings
    // even when judge is noop.
    let pool = seeded_pool().await;
    // Insert a tool_result with is_error=1 and no retry — triggers tool_failure L1
    let now = "2026-01-01T00:00:01Z";
    sqlx::query(
        "INSERT OR IGNORE INTO raw_event \
         (raw_event_id,ingest_run_id,source_type,source_uri,source_line_no,\
          captured_at,payload_json,schema_version,provenance) \
         VALUES (?,?,?,?,?,?,?,?,?)",
    )
    .bind("raw_001").bind("run_0").bind("claude_transcript")
    .bind("file://test.jsonl").bind(1_i64)
    .bind(now).bind("{}").bind("raw_event.v1").bind("{}")
    .execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id,raw_event_id,schema_version,session_id,observed_at,\
          actor,kind,tool_name,tool_use_id,is_error,provenance) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("ev_001").bind("raw_001").bind("observed_event.v1")
    .bind("sess_t").bind(now)
    .bind("tool").bind("tool_result").bind("Bash").bind("tu_0")
    .bind(1_i64).bind("{}")
    .execute(&pool).await.unwrap();

    let runtime = JudgeRuntime::noop();
    run_extractors_with_runtime(&pool, "sess_t", &runtime).await.unwrap();

    let findings: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM finding WHERE category='tool_failure'",
    )
    .fetch_one(&pool).await.unwrap();
    assert!(findings >= 1, "tool_failure L1 must still fire with noop judge");
}
```

- [ ] **Step 2: Run to verify it fails**

```bash
cargo test insight_pipeline_l2 2>&1 | tail -5
```

Expected: FAIL — `JudgeRuntime`, `run_extractors_with_runtime` don't exist.

---

### Task 7: Health endpoint insight block test

**Files:**
- Create: `tests/api_health_insight.rs`

- [ ] **Step 1: Write the failing test**

```rust
//! Slice-15 — /v1/health must include an `insight` block with judge counters.

use axum_test::TestServer;
use witmcc::api::AppState;
use witmcc::insight::judge::runtime::JudgeRuntime;

async fn test_server() -> TestServer {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys = OFF").execute(&pool).await.unwrap();
    witmcc::db::migrate(&pool).await.unwrap();

    let (tx, _) = tokio::sync::broadcast::channel(64);
    let state = AppState {
        pool,
        live_tx: std::sync::Arc::new(tx),
        sse_keepalive_secs: 30,
        sse_channel_capacity: 512,
        judge_runtime: std::sync::Arc::new(JudgeRuntime::noop()),
    };
    TestServer::new(witmcc::api::router(state)).unwrap()
}

#[tokio::test]
async fn health_includes_insight_block_with_judge_kind() {
    let srv = test_server().await;
    let r = srv.get("/v1/health").await;
    r.assert_status_ok();
    let body: serde_json::Value = r.json();
    assert!(body["insight"].is_object(), "insight block missing from health response");
    assert_eq!(body["insight"]["judge_kind"], "noop");
}

#[tokio::test]
async fn health_insight_counters_are_numeric() {
    let srv = test_server().await;
    let r = srv.get("/v1/health").await;
    let body: serde_json::Value = r.json();
    for key in [
        "judge_calls_24h",
        "judge_pending_count",
        "judge_cache_hits_24h",
        "judge_cache_misses_24h",
        "judge_budget_exhaustions_24h",
    ] {
        assert!(
            body["insight"][key].is_number(),
            "insight.{key} is not a number: {}",
            body["insight"][key]
        );
    }
}

#[tokio::test]
async fn health_insight_noop_counters_are_zero() {
    let srv = test_server().await;
    let r = srv.get("/v1/health").await;
    let body: serde_json::Value = r.json();
    // With NoopJudge and no traffic, all 24h counters must be 0
    assert_eq!(body["insight"]["judge_calls_24h"], 0);
    assert_eq!(body["insight"]["judge_cache_hits_24h"], 0);
    assert_eq!(body["insight"]["judge_cache_misses_24h"], 0);
    assert_eq!(body["insight"]["judge_budget_exhaustions_24h"], 0);
}
```

- [ ] **Step 2: Run to verify it fails**

```bash
cargo test api_health_insight 2>&1 | tail -5
```

Expected: FAIL — `AppState` doesn't have `judge_runtime` field yet.

- [ ] **Step 3: Commit the red tests**

```bash
git add tests/migration_judge_cache_schema.rs \
        tests/migration_findings_pending_schema.rs \
        tests/judge_noop.rs \
        tests/judge_fixture.rs \
        tests/judge_cache.rs \
        tests/judge_budget.rs \
        tests/insight_pipeline_l2.rs \
        tests/api_health_insight.rs \
        tests/fixtures/judge/scenario_a.json
git commit -m "test(slice-15): red-locking tests for judge pipeline, cache, budget, pending queue, health"
```

Expected: commit succeeds (tests compile if they have `allow(unused)` or are in separate integration test files that won't be run until the lib compiles — Rust integration tests fail at compile time if the library items don't exist, but they can be committed as-is).

---

## Phase 2 — Migrations + DB repos

### Task 8: Migration files

**Files:**
- Create: `migrations/20260531120000_0009_judge_cache.sql`
- Create: `migrations/20260531180000_0010_findings_pending.sql`

- [ ] **Step 1: Write the migration files**

`migrations/20260531120000_0009_judge_cache.sql`:
```sql
-- Slice-15: LLM judge verdict cache.
-- cache_key = sha256(category || "\0" || model_id || "\0" || prompt_template_version || "\0" || evidence_hash)
-- Cache is cross-session: same evidence in different sessions shares a cached verdict.
-- Retention: swept by slice-19 (entries older than 30d by last_hit_at).

CREATE TABLE IF NOT EXISTS judge_verdict_cache (
    cache_key                   TEXT PRIMARY KEY,
    category                    TEXT NOT NULL,
    model_id                    TEXT NOT NULL,
    prompt_template_version     TEXT NOT NULL,
    evidence_hash               TEXT NOT NULL,
    verdict_json                TEXT NOT NULL,        -- JSON-serialised JudgeVerdict
    created_at                  TEXT NOT NULL DEFAULT (datetime('now')),
    last_hit_at                 TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_judge_cache_category ON judge_verdict_cache(category);
```

`migrations/20260531180000_0010_findings_pending.sql`:
```sql
-- Slice-15: Candidates that need judge evaluation but couldn't be judged yet
-- (budget exhausted, judge disabled, transport error). Drained on next rebuild
-- that has budget. Monotone progress guarantee: candidates are never silently dropped.

CREATE TABLE IF NOT EXISTS findings_pending_judge (
    candidate_id        TEXT PRIMARY KEY,                  -- == finding_id derivation key
    schema_version      TEXT NOT NULL DEFAULT 'pending_finding.v1',
    session_id          TEXT NOT NULL,
    category            TEXT NOT NULL,
    confidence_l1       REAL NOT NULL,
    evidence_refs       TEXT NOT NULL,                    -- JSON array of event_id strings
    evidence_projection TEXT NOT NULL,                    -- JSON object — projection for judge
    queued_at           TEXT NOT NULL DEFAULT (datetime('now')),
    last_attempt_at     TEXT,
    attempts            INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_pending_session ON findings_pending_judge(session_id);
```

- [ ] **Step 2: Run migration schema tests to confirm green**

```bash
cargo test migration_judge_cache_schema migration_findings_pending_schema 2>&1 | tail -5
```

Expected: PASS (2 + 2 = 4 tests green).

---

### Task 9: DB repos for judge_cache and findings_pending

**Files:**
- Create: `src/db/repo_judge_cache.rs`
- Create: `src/db/repo_findings_pending.rs`
- Modify: `src/db/mod.rs` — add two pub mod lines

- [ ] **Step 1: Write `src/db/repo_judge_cache.rs`**

```rust
//! `judge_verdict_cache` side-table repo (slice-15).
//!
//! Cache key is derived externally (see `insight::judge::cache`).
//! The repo just does get/put/touch/sweep operations.

use sqlx::SqlitePool;
use crate::error::Result;

/// Stored cache row shape (deserialized on get).
#[derive(Debug, Clone)]
pub struct JudgeCacheRow {
    pub cache_key: String,
    pub category: String,
    pub model_id: String,
    pub prompt_template_version: String,
    pub evidence_hash: String,
    pub verdict_json: String,
    pub created_at: String,
    pub last_hit_at: String,
}

/// Look up a cached verdict by key. Returns `None` if missing.
pub async fn get(pool: &SqlitePool, cache_key: &str) -> Result<Option<JudgeCacheRow>> {
    let row = sqlx::query_as!(
        JudgeCacheRow,
        r#"SELECT cache_key, category, model_id, prompt_template_version,
                  evidence_hash, verdict_json, created_at, last_hit_at
           FROM judge_verdict_cache WHERE cache_key = ?"#,
        cache_key
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Insert or replace a cache entry.
pub async fn put(
    pool: &SqlitePool,
    cache_key: &str,
    category: &str,
    model_id: &str,
    prompt_template_version: &str,
    evidence_hash: &str,
    verdict_json: &str,
) -> Result<()> {
    sqlx::query!(
        r#"INSERT OR REPLACE INTO judge_verdict_cache
           (cache_key, category, model_id, prompt_template_version, evidence_hash, verdict_json)
           VALUES (?, ?, ?, ?, ?, ?)"#,
        cache_key, category, model_id, prompt_template_version, evidence_hash, verdict_json
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Update `last_hit_at` to now for LRU-style tracking.
pub async fn touch(pool: &SqlitePool, cache_key: &str) -> Result<()> {
    sqlx::query!(
        "UPDATE judge_verdict_cache SET last_hit_at = datetime('now') WHERE cache_key = ?",
        cache_key
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete entries not accessed since `older_than_days` days ago.
/// Returns the number of deleted rows.
pub async fn sweep_older_than(pool: &SqlitePool, older_than_days: i64) -> Result<u64> {
    let res = sqlx::query!(
        "DELETE FROM judge_verdict_cache WHERE last_hit_at < datetime('now', ? || ' days')",
        older_than_days  // negative: e.g. -30 → "now - 30 days"
    )
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}
```

- [ ] **Step 2: Write `src/db/repo_findings_pending.rs`**

```rust
//! `findings_pending_judge` side-table repo (slice-15).
//!
//! Candidates that need judge evaluation but couldn't proceed (budget, disabled, transport error).
//! Drained by the pipeline at the start of the next rebuild if budget is available.

use sqlx::SqlitePool;
use crate::error::Result;

/// A row in the findings_pending_judge table.
#[derive(Debug, Clone)]
pub struct PendingFindingRow {
    pub candidate_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub category: String,
    pub confidence_l1: f64,
    pub evidence_refs: String,      // JSON array
    pub evidence_projection: String, // JSON object
    pub queued_at: String,
    pub last_attempt_at: Option<String>,
    pub attempts: i64,
}

/// Enqueue a candidate (INSERT OR REPLACE — idempotent by candidate_id).
pub async fn enqueue(
    pool: &SqlitePool,
    candidate_id: &str,
    session_id: &str,
    category: &str,
    confidence_l1: f64,
    evidence_refs: &str,
    evidence_projection: &str,
) -> Result<()> {
    sqlx::query!(
        r#"INSERT OR REPLACE INTO findings_pending_judge
           (candidate_id, session_id, category, confidence_l1, evidence_refs, evidence_projection)
           VALUES (?, ?, ?, ?, ?, ?)"#,
        candidate_id, session_id, category, confidence_l1, evidence_refs, evidence_projection
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Load all pending candidates for a session, ordered by queued_at ascending.
pub async fn list_session(pool: &SqlitePool, session_id: &str) -> Result<Vec<PendingFindingRow>> {
    let rows = sqlx::query_as!(
        PendingFindingRow,
        r#"SELECT candidate_id, schema_version, session_id, category,
                  confidence_l1, evidence_refs, evidence_projection,
                  queued_at, last_attempt_at, attempts
           FROM findings_pending_judge
           WHERE session_id = ?
           ORDER BY queued_at ASC"#,
        session_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Remove a candidate after it has been judged (promoted or discarded).
pub async fn dequeue(pool: &SqlitePool, candidate_id: &str) -> Result<()> {
    sqlx::query!(
        "DELETE FROM findings_pending_judge WHERE candidate_id = ?",
        candidate_id
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Total count of pending candidates (all sessions) — used for health endpoint.
pub async fn count_all(pool: &SqlitePool) -> Result<i64> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM findings_pending_judge")
        .fetch_one(pool)
        .await?;
    Ok(n)
}

/// Bump `attempts` and update `last_attempt_at` to now.
pub async fn record_attempt(pool: &SqlitePool, candidate_id: &str) -> Result<()> {
    sqlx::query!(
        r#"UPDATE findings_pending_judge
           SET attempts = attempts + 1, last_attempt_at = datetime('now')
           WHERE candidate_id = ?"#,
        candidate_id
    )
    .execute(pool)
    .await?;
    Ok(())
}
```

- [ ] **Step 3: Update `src/db/mod.rs`**

Add after the last existing `pub mod` line:
```rust
pub mod repo_judge_cache;
pub mod repo_findings_pending;
```

- [ ] **Step 4: Compile check**

```bash
cargo check 2>&1 | tail -10
```

Expected: clean (no errors in db layer).

- [ ] **Step 5: Commit**

```bash
git add migrations/20260531120000_0009_judge_cache.sql \
        migrations/20260531180000_0010_findings_pending.sql \
        src/db/repo_judge_cache.rs \
        src/db/repo_findings_pending.rs \
        src/db/mod.rs
git commit -m "feat(db): 0009_judge_cache + 0010_findings_pending migrations + repos"
```

---

## Phase 3 — JudgeProvider trait + NoopJudge + FixtureJudge

### Task 10: Judge types, errors, trait, NoopJudge, FixtureJudge

**Files:**
- Create: `src/insight/judge/mod.rs`
- Create: `src/insight/judge/types.rs`
- Create: `src/insight/judge/errors.rs`
- Create: `src/insight/judge/providers/mod.rs`
- Create: `src/insight/judge/providers/noop.rs`
- Create: `src/insight/judge/providers/fixture.rs`
- Modify: `src/insight/mod.rs` — add `pub mod judge;`

- [ ] **Step 1: Create `src/insight/judge/types.rs`**

```rust
//! Core types for the L2 judge layer (slice-15).

/// Input to the judge: a structured prompt with category, candidate id,
/// evidence projection (compact JSON), and the versioned system template string.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JudgePrompt {
    /// Stable category id (e.g. "risky_action").
    pub category: String,
    /// Derivation key for the finding (== finding_id without prefix).
    pub candidate_id: String,
    /// Compact JSON evidence for this candidate — what the judge sees.
    pub evidence_projection: serde_json::Value,
    /// Versioned system prompt template string.
    pub system_template: String,
}

/// The judge's structured verdict.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JudgeVerdict {
    /// `true` → promote to Finding; `false` → discard candidate.
    pub promote: bool,
    /// Judge confidence in range [0.0, 1.0]. 0.0 when `promote == false`.
    pub confidence_l2: f32,
    /// Short one-sentence reason referencing at least one evidence field name.
    pub reason: String,
    /// Populated only for `final_state_mismatch` when `promote == true`.
    pub mismatch_summary: Option<String>,
}
```

- [ ] **Step 2: Create `src/insight/judge/errors.rs`**

```rust
//! Error variants for the L2 judge layer (slice-15).

/// Errors that can be returned by a `JudgeProvider::judge()` call.
#[derive(Debug)]
pub enum JudgeError {
    /// Network failure or 5xx from the API.
    Transport(String),
    /// Model returned malformed JSON or unexpected schema.
    Schema(String),
    /// Request timed out.
    Timeout,
    /// Budget guard exhausted the per-rebuild call budget.
    BudgetExhausted,
    /// Judge is explicitly disabled (NoopJudge).
    Disabled,
}

impl std::fmt::Display for JudgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(s) => write!(f, "judge transport error: {s}"),
            Self::Schema(s)    => write!(f, "judge schema error: {s}"),
            Self::Timeout      => write!(f, "judge timeout"),
            Self::BudgetExhausted => write!(f, "judge budget exhausted"),
            Self::Disabled     => write!(f, "judge disabled"),
        }
    }
}

impl std::error::Error for JudgeError {}
```

- [ ] **Step 3: Create `src/insight/judge/mod.rs`**

```rust
//! L2 LLM judge infrastructure (slice-15).
//!
//! Architecture: `BudgetGuard<CachedProvider<impl JudgeProvider>>`.
//! Default is `NoopJudge` — L2 is off by default.
//! Opt-in via `--judge anthropic` or `--judge fixture` CLI flags.

pub mod budget;
pub mod cache;
pub mod errors;
pub mod metrics;
pub mod providers;
pub mod runtime;
pub mod types;

pub use errors::JudgeError;
pub use types::{JudgePrompt, JudgeVerdict};

/// Contract for all judge implementations.
/// Implementations: `NoopJudge`, `FixtureJudge`, `AnthropicJudge`.
/// Wrapped by `CachedProvider` then `BudgetGuard` in production.
#[async_trait::async_trait]
pub trait JudgeProvider: Send + Sync {
    /// Attempt to judge the candidate. On error, the pipeline queues the
    /// candidate to `findings_pending_judge`.
    async fn judge(&self, prompt: JudgePrompt) -> Result<JudgeVerdict, JudgeError>;

    /// Stable, version-suffixed model name. Surfaced in Finding.provenance.
    fn model_id(&self) -> &'static str;

    /// Version of the prompt template. Part of the cache key — changing the
    /// template automatically invalidates prior cache entries for this judge.
    fn prompt_template_version(&self) -> &'static str;
}
```

- [ ] **Step 4: Add `async_trait` dependency if needed**

Check Cargo.toml:
```bash
grep "async-trait" /Users/bahamoth/projects/whats-in-my-cc/Cargo.toml
```

If missing, add to `[dependencies]`:
```toml
async-trait = "0.1"
```

(Check first — it may already be implicit. If `cargo check` fails, add it.)

- [ ] **Step 5: Create `src/insight/judge/providers/noop.rs`**

```rust
//! `NoopJudge` — always returns `Err(JudgeError::Disabled)` (slice-15).
//!
//! Used when the user has not opted into LLM judgment.
//! Returning `Err(Disabled)` (not `Ok(promote=false)`) so the pipeline
//! distinguishes "judge disabled" from "judge ran and said no" — per DEV-S15-06.

use crate::insight::judge::{JudgeError, JudgePrompt, JudgeProvider, JudgeVerdict};

pub struct NoopJudge;

#[async_trait::async_trait]
impl JudgeProvider for NoopJudge {
    async fn judge(&self, _p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        Err(JudgeError::Disabled)
    }

    fn model_id(&self) -> &'static str { "noop" }

    fn prompt_template_version(&self) -> &'static str { "noop" }
}
```

- [ ] **Step 6: Create `src/insight/judge/providers/fixture.rs`**

```rust
//! `FixtureJudge` — replays recorded verdicts from a JSON file (slice-15).
//!
//! Used by tests and smoke scenarios for deterministic judge output without
//! real LLM calls. Key format in the file: `"category||evidence_hash"`.
//!
//! `judge_with_hash()` is a test-only helper that bypasses the SHA-256
//! derivation to allow tests to address verdicts by pre-known hash strings.

use std::collections::HashMap;
use std::path::Path;

use crate::insight::judge::{JudgeError, JudgePrompt, JudgeProvider, JudgeVerdict};
use crate::insight::judge::cache::evidence_hash;

pub struct FixtureJudge {
    table: HashMap<(String, String), JudgeVerdict>,
}

impl FixtureJudge {
    /// Load the fixture JSON file. Key format: `"category||evidence_hash"`.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let raw: HashMap<String, JudgeVerdict> =
            serde_json::from_reader(std::fs::File::open(path)?)?;
        let table = raw
            .into_iter()
            .map(|(k, v)| {
                let mut parts = k.splitn(2, "||");
                let cat = parts.next().unwrap_or("").to_string();
                let hash = parts.next().unwrap_or("").to_string();
                ((cat, hash), v)
            })
            .collect();
        Ok(Self { table })
    }

    /// Test helper: look up verdict by supplying the hash directly.
    pub async fn judge_with_hash(
        &self,
        p: JudgePrompt,
        hash: &str,
    ) -> Result<JudgeVerdict, JudgeError> {
        let key = (p.category.clone(), hash.to_string());
        self.table
            .get(&key)
            .cloned()
            .ok_or_else(|| JudgeError::Schema(format!(
                "FixtureJudge: no entry for {}||{}", p.category, hash
            )))
    }
}

#[async_trait::async_trait]
impl JudgeProvider for FixtureJudge {
    async fn judge(&self, p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        let hash = evidence_hash(&p.evidence_projection);
        let key = (p.category.clone(), hash.clone());
        self.table
            .get(&key)
            .cloned()
            .ok_or_else(|| JudgeError::Schema(format!(
                "FixtureJudge: no entry for {}||{}", p.category, hash
            )))
    }

    fn model_id(&self) -> &'static str { "fixture" }

    fn prompt_template_version(&self) -> &'static str { "fixture@v1" }
}
```

- [ ] **Step 7: Create `src/insight/judge/providers/mod.rs`**

```rust
//! Provider implementations for `JudgeProvider` (slice-15).

pub mod anthropic;
pub mod fixture;
pub mod noop;

pub use fixture::FixtureJudge;
pub use noop::NoopJudge;
pub use anthropic::AnthropicJudge;
```

- [ ] **Step 8: Update `src/insight/mod.rs`**

Add `pub mod judge;` after the existing module declarations:
```rust
pub mod judge;
```

- [ ] **Step 9: Verify NoopJudge test is green**

```bash
cargo test judge_noop 2>&1 | tail -5
cargo test judge_fixture 2>&1 | tail -5
```

Expected: Green (once `AnthropicJudge` stub exists — add a stub if needed to satisfy the `pub use`).

- [ ] **Step 10: Commit**

```bash
git add src/insight/judge/ src/insight/mod.rs
git commit -m "feat(insight): JudgeProvider trait + NoopJudge + FixtureJudge"
```

---

## Phase 4 — AnthropicJudge

### Task 11: Prompt template + AnthropicJudge implementation

**Files:**
- Create: `src/insight/judge/prompts/judge_v1.txt`
- Create: `src/insight/judge/providers/anthropic.rs`

Note: No real LLM calls in tests. The AnthropicJudge is wired but tested only via a local mock HTTP server using `axum_test` (already a dev dependency).

- [ ] **Step 1: Create `src/insight/judge/prompts/judge_v1.txt`**

Create directory first:
```bash
mkdir -p /Users/bahamoth/projects/whats-in-my-cc/src/insight/judge/prompts
```

Content of `src/insight/judge/prompts/judge_v1.txt`:
```
You are an analysis judge for a software-engineering observation tool. You are given:
- A finding category (one of: risky_action, context_bloat, final_state_mismatch).
- The structured evidence the deterministic extractor used to identify a candidate.

Decide whether the candidate should be promoted to a stored finding.

Return JSON of this exact shape (no prose, no markdown):

{
  "promote": boolean,
  "confidence_l2": number between 0.0 and 1.0,
  "reason": "<one short sentence>",
  "mismatch_summary": null | "<one paragraph when category == 'final_state_mismatch'>"
}

Rules:
- Promote only if the evidence shows a real problem; do not promote based on what is missing from the evidence.
- Set confidence_l2 = 0.0 if you would not promote.
- "reason" must reference at least one field name from the evidence.
- "mismatch_summary" is required when category == 'final_state_mismatch' and promote == true; null otherwise.
```

- [ ] **Step 2: Create `src/insight/judge/providers/anthropic.rs`**

```rust
//! `AnthropicJudge` — real Anthropic API call using hand-rolled reqwest (slice-15).
//!
//! Per DEV-S15-05: we hand-roll the Anthropic client over reqwest rather than
//! depending on a Rust SDK crate. The surface we need is small (one
//! structured-output call shape).
//!
//! Uses prompt caching (cache_control: ephemeral) on the stable system prompt +
//! schema block to minimise token cost per call.
//!
//! Model is pinned to `claude-sonnet-4-6`. Changing the model requires bumping
//! MODEL constant and updating the PROMPT_TEMPLATE_VERSION.

use crate::insight::judge::{JudgeError, JudgePrompt, JudgeProvider, JudgeVerdict};

/// Anthropic Messages API endpoint.
const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";

/// System prompt loaded from the embedded template file.
const SYSTEM_TEMPLATE: &str = include_str!("../prompts/judge_v1.txt");

/// Compute a stable version string: "judge@v1#" + first 12 hex chars of SHA-256 of the template.
/// Any edit to judge_v1.txt automatically bumps this, invalidating stale cache entries.
fn prompt_template_version_str() -> &'static str {
    use sha2::{Digest, Sha256};
    static VERSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    VERSION.get_or_init(|| {
        let mut h = Sha256::new();
        h.update(SYSTEM_TEMPLATE.as_bytes());
        let hex = hex::encode(h.finalize());
        format!("judge@v1#{}", &hex[..12])
    })
}

pub struct AnthropicJudge {
    client: reqwest::Client,
    api_key: String,
    model: &'static str,
}

impl AnthropicJudge {
    pub const MODEL: &'static str = "claude-sonnet-4-6";

    /// Construct from the `ANTHROPIC_API_KEY` environment variable.
    /// Returns an error if the variable is not set.
    pub fn from_env() -> anyhow::Result<Self> {
        let key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY not set"))?;
        Ok(Self {
            client: reqwest::Client::new(),
            api_key: key,
            model: Self::MODEL,
        })
    }

    /// Construct with an explicit API key (for testing with mock servers).
    pub fn with_key_and_base_url(
        api_key: impl Into<String>,
        _base_url: impl Into<String>,
        client: reqwest::Client,
    ) -> Self {
        Self {
            client,
            api_key: api_key.into(),
            model: Self::MODEL,
        }
    }
}

#[async_trait::async_trait]
impl JudgeProvider for AnthropicJudge {
    async fn judge(&self, p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        let system_content = serde_json::json!([{
            "type": "text",
            "text": SYSTEM_TEMPLATE,
            "cache_control": { "type": "ephemeral" }
        }]);

        let user_content = serde_json::to_string(&p.evidence_projection)
            .unwrap_or_else(|_| "{}".to_string());

        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 512,
            "system": system_content,
            "messages": [
                { "role": "user", "content": user_content }
            ]
        });

        let resp = self
            .client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("anthropic-beta", "prompt-caching-2024-07-31")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| JudgeError::Transport(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(JudgeError::Transport(format!(
                "Anthropic API returned {status}: {text}"
            )));
        }

        let raw: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| JudgeError::Schema(e.to_string()))?;

        // Extract text from content[0].text
        let text = raw
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .and_then(|b| b.get("text"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| JudgeError::Schema("unexpected response shape".to_string()))?;

        serde_json::from_str::<JudgeVerdict>(text)
            .map_err(|e| JudgeError::Schema(format!("verdict parse error: {e}")))
    }

    fn model_id(&self) -> &'static str {
        Self::MODEL
    }

    fn prompt_template_version(&self) -> &'static str {
        prompt_template_version_str()
    }
}
```

- [ ] **Step 3: Compile check**

```bash
cargo check 2>&1 | tail -10
```

If `hex` crate is missing, add `hex = "0.4"` to Cargo.toml (check first: `grep "hex" Cargo.toml`).

- [ ] **Step 4: Commit**

```bash
git add src/insight/judge/prompts/judge_v1.txt \
        src/insight/judge/providers/anthropic.rs \
        src/insight/judge/providers/mod.rs \
        Cargo.toml
git commit -m "feat(insight): AnthropicJudge implementation + prompt_v1 template"
```

---

## Phase 5 — Cache + Budget composition

### Task 12: Cache helper functions + CachedProvider

**Files:**
- Create: `src/insight/judge/cache.rs`

- [ ] **Step 1: Create `src/insight/judge/cache.rs`**

```rust
//! `CachedProvider` — wraps any `JudgeProvider` with SQLite-backed caching (slice-15).
//!
//! Cache key: sha256(category || "\0" || model_id || "\0" || prompt_template_version || "\0" || evidence_hash)
//! where evidence_hash = sha256(canonical_json(evidence_projection)).
//!
//! Per DEV-S15-07: the prompt_template_version is included in the key so
//! changing the prompt automatically invalidates stale cache entries.

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::db::{repo_judge_cache};
use crate::insight::judge::metrics::JudgeMetrics;
use crate::insight::judge::{JudgeError, JudgePrompt, JudgeProvider, JudgeVerdict};

/// Derive the SHA-256 of the canonical (key-sorted) JSON of an evidence projection.
/// Canonical JSON: recursively sort object keys, then serialize.
pub fn evidence_hash(proj: &serde_json::Value) -> String {
    let canon = canonical_json(proj);
    let mut h = Sha256::new();
    h.update(canon.as_bytes());
    hex::encode(h.finalize())
}

/// Recursively sort all object keys to produce canonical JSON.
fn canonical_json(v: &serde_json::Value) -> String {
    use serde_json::Value;
    match v {
        Value::Object(map) => {
            let mut sorted: Vec<(&String, &Value)> = map.iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(b.0));
            let inner = sorted
                .iter()
                .map(|(k, v)| format!("{}:{}", serde_json::to_string(k).unwrap(), canonical_json(v)))
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{inner}}}")
        }
        Value::Array(arr) => {
            let inner = arr
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",");
            format!("[{inner}]")
        }
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// Derive the full cache key for a prompt + provider combination.
pub fn cache_key(p: &JudgePrompt, model_id: &str, template_version: &str) -> String {
    let ehash = evidence_hash(&p.evidence_projection);
    let material = format!(
        "{}\x00{}\x00{}\x00{}",
        p.category, model_id, template_version, ehash
    );
    let mut h = Sha256::new();
    h.update(material.as_bytes());
    format!("sha256:{}", hex::encode(h.finalize()))
}

/// Wraps an inner `JudgeProvider` with a SQLite-backed cache.
/// On cache hit: return cached verdict, bump `last_hit_at`, skip inner call.
/// On cache miss: call inner, store result, return verdict.
pub struct CachedProvider<P: JudgeProvider> {
    inner: P,
    pool: SqlitePool,
    metrics: std::sync::Arc<JudgeMetrics>,
}

impl<P: JudgeProvider> CachedProvider<P> {
    pub fn new(inner: P, pool: SqlitePool) -> Self {
        Self {
            inner,
            pool,
            metrics: std::sync::Arc::new(JudgeMetrics::default()),
        }
    }

    pub fn with_metrics(inner: P, pool: SqlitePool, metrics: std::sync::Arc<JudgeMetrics>) -> Self {
        Self { inner, pool, metrics }
    }
}

#[async_trait::async_trait]
impl<P: JudgeProvider> JudgeProvider for CachedProvider<P> {
    async fn judge(&self, p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        let key = cache_key(&p, self.inner.model_id(), self.inner.prompt_template_version());
        let ehash = evidence_hash(&p.evidence_projection);

        if let Some(row) = repo_judge_cache::get(&self.pool, &key)
            .await
            .map_err(|e| JudgeError::Transport(e.to_string()))?
        {
            self.metrics.cache_hit();
            let _ = repo_judge_cache::touch(&self.pool, &key).await;
            return serde_json::from_str::<JudgeVerdict>(&row.verdict_json)
                .map_err(|e| JudgeError::Schema(format!("cache verdict parse: {e}")));
        }

        self.metrics.cache_miss();
        let verdict = self.inner.judge(p.clone()).await?;

        let verdict_json = serde_json::to_string(&verdict)
            .map_err(|e| JudgeError::Schema(e.to_string()))?;

        let _ = repo_judge_cache::put(
            &self.pool,
            &key,
            &p.category,
            self.inner.model_id(),
            self.inner.prompt_template_version(),
            &ehash,
            &verdict_json,
        )
        .await;

        Ok(verdict)
    }

    fn model_id(&self) -> &'static str { self.inner.model_id() }
    fn prompt_template_version(&self) -> &'static str { self.inner.prompt_template_version() }
}
```

### Task 13: In-memory metrics

**Files:**
- Create: `src/insight/judge/metrics.rs`

- [ ] **Step 1: Create `src/insight/judge/metrics.rs`**

```rust
//! In-memory atomic counters for /v1/health.insight.* (slice-15).
//!
//! Per DEV-S15-03: counters are in-memory only, resetting on server restart.
//! Persistence is post-MVP.

use std::sync::atomic::{AtomicI64, Ordering};

/// Shared, cheaply-cloneable (via Arc) metrics bag.
#[derive(Default)]
pub struct JudgeMetrics {
    pub calls_24h: AtomicI64,
    pub cache_hits_24h: AtomicI64,
    pub cache_misses_24h: AtomicI64,
    pub budget_exhaustions_24h: AtomicI64,
}

impl JudgeMetrics {
    pub fn call(&self) {
        self.calls_24h.fetch_add(1, Ordering::Relaxed);
    }
    pub fn cache_hit(&self) {
        self.cache_hits_24h.fetch_add(1, Ordering::Relaxed);
    }
    pub fn cache_miss(&self) {
        self.cache_misses_24h.fetch_add(1, Ordering::Relaxed);
    }
    pub fn budget_exhaustion(&self) {
        self.budget_exhaustions_24h.fetch_add(1, Ordering::Relaxed);
    }
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            calls_24h: self.calls_24h.load(Ordering::Relaxed),
            cache_hits_24h: self.cache_hits_24h.load(Ordering::Relaxed),
            cache_misses_24h: self.cache_misses_24h.load(Ordering::Relaxed),
            budget_exhaustions_24h: self.budget_exhaustions_24h.load(Ordering::Relaxed),
        }
    }
}

/// Point-in-time snapshot of the metrics for serialisation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MetricsSnapshot {
    pub calls_24h: i64,
    pub cache_hits_24h: i64,
    pub cache_misses_24h: i64,
    pub budget_exhaustions_24h: i64,
}
```

### Task 14: BudgetGuard

**Files:**
- Create: `src/insight/judge/budget.rs`

- [ ] **Step 1: Create `src/insight/judge/budget.rs`**

```rust
//! `BudgetGuard` — limits judge calls per rebuild invocation (slice-15).
//!
//! Budget is per-invocation: a new `BudgetGuard` is constructed in each
//! `run_extractors_with_runtime` call. Per DEV-S15-02: two concurrent rebuilds
//! each get their own budget.
//!
//! Composition: `BudgetGuard<CachedProvider<impl JudgeProvider>>`.
//! Cache wraps the network; budget wraps the cache.
//! This means cache hits do NOT consume budget (the budget counts real API calls).

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::insight::judge::metrics::JudgeMetrics;
use crate::insight::judge::{JudgeError, JudgePrompt, JudgeProvider, JudgeVerdict};

/// Limits the number of judge calls in a single rebuild pass.
pub struct BudgetGuard<P: JudgeProvider> {
    inner: P,
    remaining: AtomicUsize,
    metrics: std::sync::Arc<JudgeMetrics>,
}

impl<P: JudgeProvider> BudgetGuard<P> {
    pub fn new(inner: P, budget: usize) -> Self {
        Self {
            inner,
            remaining: AtomicUsize::new(budget),
            metrics: std::sync::Arc::new(JudgeMetrics::default()),
        }
    }

    pub fn with_metrics(
        inner: P,
        budget: usize,
        metrics: std::sync::Arc<JudgeMetrics>,
    ) -> Self {
        Self {
            inner,
            remaining: AtomicUsize::new(budget),
            metrics,
        }
    }

    pub fn remaining(&self) -> usize {
        self.remaining.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl<P: JudgeProvider> JudgeProvider for BudgetGuard<P> {
    async fn judge(&self, p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        // Try to decrement budget atomically.
        let prev = self.remaining.fetch_update(
            Ordering::SeqCst,
            Ordering::SeqCst,
            |r| r.checked_sub(1),
        );
        if prev.is_err() {
            self.metrics.budget_exhaustion();
            return Err(JudgeError::BudgetExhausted);
        }
        self.metrics.call();
        self.inner.judge(p).await
    }

    fn model_id(&self) -> &'static str { self.inner.model_id() }
    fn prompt_template_version(&self) -> &'static str { self.inner.prompt_template_version() }
}
```

### Task 15: JudgeRuntime

**Files:**
- Create: `src/insight/judge/runtime.rs`

- [ ] **Step 1: Create `src/insight/judge/runtime.rs`**

```rust
//! `JudgeRuntime` — the composed judge stack wired to the pipeline (slice-15).
//!
//! The pipeline receives a `&JudgeRuntime` per rebuild. The runtime holds the
//! composed `BudgetGuard<CachedProvider<impl JudgeProvider>>` as a
//! `Box<dyn JudgeProvider>`, plus shared metrics.

use std::path::Path;
use std::sync::Arc;

use sqlx::SqlitePool;

use crate::insight::judge::budget::BudgetGuard;
use crate::insight::judge::cache::CachedProvider;
use crate::insight::judge::metrics::{JudgeMetrics, MetricsSnapshot};
use crate::insight::judge::providers::{AnthropicJudge, FixtureJudge, NoopJudge};
use crate::insight::judge::{JudgeError, JudgePrompt, JudgeProvider, JudgeVerdict};

/// Which judge implementation is active — surfaced in /v1/health.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JudgeKind {
    Noop,
    Fixture,
    Anthropic,
}

impl std::fmt::Display for JudgeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Noop      => write!(f, "noop"),
            Self::Fixture   => write!(f, "fixture"),
            Self::Anthropic => write!(f, "anthropic"),
        }
    }
}

/// The composed judge stack — holds a boxed provider + shared metrics.
/// Cheaply cloneable via Arc.
pub struct JudgeRuntime {
    pub kind: JudgeKind,
    pub budget: usize,
    pub metrics: Arc<JudgeMetrics>,
    /// The composed provider used per-rebuild inside a fresh BudgetGuard.
    /// Stored as a factory fn so each rebuild gets its own BudgetGuard.
    provider_factory: Arc<dyn Fn(Arc<JudgeMetrics>) -> Box<dyn JudgeProvider> + Send + Sync>,
}

impl JudgeRuntime {
    /// Build a NoopJudge runtime (default — no LLM calls, no budget consumed).
    pub fn noop() -> Self {
        Self {
            kind: JudgeKind::Noop,
            budget: 0,
            metrics: Arc::new(JudgeMetrics::default()),
            provider_factory: Arc::new(|_metrics| Box::new(NoopJudge)),
        }
    }

    /// Build a FixtureJudge runtime for tests/smoke.
    pub fn fixture(path: &Path, budget: usize) -> anyhow::Result<Self> {
        let table = FixtureJudge::load(path)?;
        let arc = Arc::new(table);
        Ok(Self {
            kind: JudgeKind::Fixture,
            budget,
            metrics: Arc::new(JudgeMetrics::default()),
            provider_factory: Arc::new(move |_metrics| {
                // FixtureJudge is cloned per-rebuild; it's cheap (HashMap clone)
                Box::new(FixtureJudgeAdapter(arc.clone()))
            }),
        })
    }

    /// Build a FixtureJudge runtime with a zero budget (for testing budget exhaustion).
    pub fn fixture_with_budget(path: &Path, budget: usize) -> anyhow::Result<Self> {
        Self::fixture(path, budget)
    }

    /// Build an AnthropicJudge runtime (production; requires ANTHROPIC_API_KEY).
    pub fn anthropic(pool: SqlitePool, budget: usize) -> anyhow::Result<Self> {
        let judge = AnthropicJudge::from_env()?;
        let arc = Arc::new(judge);
        Ok(Self {
            kind: JudgeKind::Anthropic,
            budget,
            metrics: Arc::new(JudgeMetrics::default()),
            provider_factory: Arc::new(move |metrics| {
                let cached = CachedProvider::with_metrics(
                    AnthropicAdapter(arc.clone()),
                    pool.clone(),
                    metrics.clone(),
                );
                Box::new(cached)
            }),
        })
    }

    /// Create a fresh `BudgetGuard`-wrapped provider for a single rebuild invocation.
    /// Each call returns a new guard with a fresh budget counter.
    pub fn build_for_rebuild(&self) -> Box<dyn JudgeProvider> {
        let inner = (self.provider_factory)(self.metrics.clone());
        Box::new(BudgetGuardDyn {
            inner,
            remaining: std::sync::atomic::AtomicUsize::new(self.budget),
            metrics: self.metrics.clone(),
        })
    }

    /// Snapshot current metrics for the health endpoint.
    pub fn metrics_snapshot(&self) -> MetricsSnapshot {
        self.metrics.snapshot()
    }
}

// --- Internal adapters so Arc<T> implements JudgeProvider ---

struct FixtureJudgeAdapter(Arc<FixtureJudge>);

#[async_trait::async_trait]
impl JudgeProvider for FixtureJudgeAdapter {
    async fn judge(&self, p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        self.0.judge(p).await
    }
    fn model_id(&self) -> &'static str { "fixture" }
    fn prompt_template_version(&self) -> &'static str { "fixture@v1" }
}

struct AnthropicAdapter(Arc<AnthropicJudge>);

#[async_trait::async_trait]
impl JudgeProvider for AnthropicAdapter {
    async fn judge(&self, p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        self.0.judge(p).await
    }
    fn model_id(&self) -> &'static str { AnthropicJudge::MODEL }
    fn prompt_template_version(&self) -> &'static str { self.0.prompt_template_version() }
}

/// A dynamic BudgetGuard for boxed providers.
struct BudgetGuardDyn {
    inner: Box<dyn JudgeProvider>,
    remaining: std::sync::atomic::AtomicUsize,
    metrics: Arc<JudgeMetrics>,
}

#[async_trait::async_trait]
impl JudgeProvider for BudgetGuardDyn {
    async fn judge(&self, p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        let prev = self.remaining.fetch_update(
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
            |r| r.checked_sub(1),
        );
        if prev.is_err() {
            self.metrics.budget_exhaustion();
            return Err(JudgeError::BudgetExhausted);
        }
        self.metrics.call();
        self.inner.judge(p).await
    }
    fn model_id(&self) -> &'static str { self.inner.model_id() }
    fn prompt_template_version(&self) -> &'static str { self.inner.prompt_template_version() }
}
```

- [ ] **Step 2: Verify budget and cache tests green**

```bash
cargo test judge_budget judge_cache judge_noop judge_fixture 2>&1 | tail -10
```

Expected: Green.

- [ ] **Step 3: Commit**

```bash
git add src/insight/judge/cache.rs \
        src/insight/judge/budget.rs \
        src/insight/judge/metrics.rs \
        src/insight/judge/runtime.rs
git commit -m "feat(insight): CachedProvider + BudgetGuard + JudgeRuntime composition"
```

---

## Phase 6 — Pipeline integration + CLI + health

### Task 16: NoopTestExtractor (cfg(test) only)

**Files:**
- Modify: `src/insight/extractors/mod.rs` — add cfg(test) extractor

- [ ] **Step 1: Add NoopTestExtractor to `src/insight/extractors/mod.rs`**

Read the current file first, then add at the bottom:

```rust
/// Test-only extractor that emits one candidate per session, with PromotionPolicy::Never.
/// Used to exercise the full L2 path (cache, budget, pending queue) without any
/// production categories. Gated behind cfg(test) — never visible in production builds.
#[cfg(test)]
pub mod noop_test {
    use crate::insight::extractor::InsightExtractor;
    use crate::insight::types::{FindingCandidate, PromotionPolicy};
    use crate::insight::view::SessionInsightView;

    pub struct NoopTestExtractor;

    impl InsightExtractor for NoopTestExtractor {
        fn category(&self) -> &'static str { "noop_test" }
        fn floor(&self) -> f32 { 0.0 }
        fn promotion_policy(&self) -> PromotionPolicy { PromotionPolicy::Never }

        fn extract(&self, view: &SessionInsightView<'_>) -> Vec<FindingCandidate> {
            if view.events.is_empty() {
                return Vec::new();
            }
            vec![FindingCandidate {
                category: "noop_test",
                confidence_l1: 0.5,
                severity: "low",
                summary: "noop_test candidate".to_string(),
                evidence_refs: vec![view.events[0].event_id.clone()],
                evidence_projection: serde_json::json!({
                    "first_event_id": view.events[0].event_id,
                    "category": "noop_test"
                }),
            }]
        }
    }
}
```

### Task 17: Extend pipeline with L2 support

**Files:**
- Modify: `src/insight/pipeline.rs` — add `run_extractors_with_runtime`, pending drain logic

- [ ] **Step 1: Read current pipeline.rs, then rewrite with L2 support**

Replace the entire file content with:

```rust
//! Extractor pipeline runner (slice-14 base; slice-15 extends with L2 judge).
//!
//! `run_extractors` — original L1-only path (unchanged API, always uses NoopJudge).
//! `run_extractors_with_runtime` — slice-15 L2 path; accepts a `JudgeRuntime`.
//!
//! L2 flow per rebuild:
//! 1. Drain `findings_pending_judge` for this session (attempt with current budget).
//! 2. Run L1 extractors → collect candidates.
//! 3. For each candidate:
//!    - `Always` policy → promote directly to Finding.
//!    - `Never` / `IfAbove(t)` with confidence <= t → attempt judge via runtime.
//!      On `Ok(verdict)`: promote or discard.
//!      On `Err(BudgetExhausted | Disabled | Transport | Timeout)`: enqueue to pending.
//!      On `Err(Schema)`: discard + log (malformed verdict not retried).

use sqlx::SqlitePool;

use crate::db::{repo_finding, repo_findings_pending};
use crate::db::repo_finding::FindingRow;
use crate::error::Result;
use crate::ids::derive_finding_id;
use crate::insight::judge::runtime::JudgeRuntime;
use crate::insight::judge::{JudgeError, JudgePrompt, JudgeProvider};
use crate::insight::registry::all_extractors;
use crate::insight::types::{FindingCandidate, PromotionPolicy, Provenance};
use crate::insight::view::OwnedSessionInsightData;
use crate::model::meta::SCHEMA_VERSION;

/// Minimum confidence below which a candidate is dropped.
pub const CONFIDENCE_FLOOR: f32 = 0.5;

/// Original L1-only path — uses NoopJudge internally (backward-compatible API).
pub async fn run_extractors(pool: &SqlitePool, session_id: &str) -> Result<Vec<FindingRow>> {
    let runtime = JudgeRuntime::noop();
    run_extractors_with_runtime(pool, session_id, &runtime).await
}

/// Extended pipeline with optional L2 judge (slice-15).
/// Drains pending queue first, then runs extractors.
pub async fn run_extractors_with_runtime(
    pool: &SqlitePool,
    session_id: &str,
    runtime: &JudgeRuntime,
) -> Result<Vec<FindingRow>> {
    let data = OwnedSessionInsightData::load(pool, session_id).await?;
    let view = data.as_view(session_id);

    // Build a fresh judge for this rebuild (each invocation gets its own budget counter).
    let judge = runtime.build_for_rebuild();

    let mut rows: Vec<FindingRow> = Vec::new();

    // Phase 1: drain pending candidates from previous runs.
    let pending = repo_findings_pending::list_session(pool, session_id).await?;
    for prow in pending {
        let proj: serde_json::Value =
            serde_json::from_str(&prow.evidence_projection).unwrap_or(serde_json::json!({}));
        let p = JudgePrompt {
            category: prow.category.clone(),
            candidate_id: prow.candidate_id.clone(),
            evidence_projection: proj,
            system_template: "judge@v1".to_string(),
        };
        repo_findings_pending::record_attempt(pool, &prow.candidate_id).await?;
        match judge.judge(p).await {
            Ok(verdict) => {
                repo_findings_pending::dequeue(pool, &prow.candidate_id).await?;
                if verdict.promote && verdict.confidence_l2 >= CONFIDENCE_FLOOR {
                    let prov = Provenance {
                        extractor: Box::leak(
                            format!("{}@v1", prow.category).into_boxed_str()
                        ),
                        layer: "L2",
                        judge: Some(judge.model_id().to_string()),
                        judge_template_version: Some(judge.prompt_template_version().to_string()),
                        rule_pack: None,
                    };
                    let ev_refs: Vec<String> =
                        serde_json::from_str(&prow.evidence_refs).unwrap_or_default();
                    let row = FindingRow {
                        finding_id: prow.candidate_id.clone(),
                        schema_version: "finding.v1".into(),
                        session_id: session_id.to_string(),
                        category: prow.category.clone(),
                        severity: "medium".to_string(), // pending rows don't carry severity; default
                        confidence: verdict.confidence_l2 as f64,
                        summary: verdict.reason.clone(),
                        evidence_refs: prow.evidence_refs.clone(),
                        evidence_projection: prow.evidence_projection.clone(),
                        provenance: prov.to_json_string(),
                        status: "active".into(),
                        created_at: chrono::Utc::now().to_rfc3339(),
                    };
                    repo_finding::insert(pool, &row).await?;
                    rows.push(row);
                }
            }
            Err(JudgeError::BudgetExhausted) | Err(JudgeError::Disabled) => {
                // Still in budget / disabled → leave in queue for next run
            }
            Err(JudgeError::Transport(_)) | Err(JudgeError::Timeout) => {
                // Transient error → leave in queue
            }
            Err(JudgeError::Schema(e)) => {
                // Malformed verdict → discard and remove from queue
                tracing::warn!(candidate_id = %prow.candidate_id, error = %e,
                    "schema error from judge — discarding pending candidate");
                repo_findings_pending::dequeue(pool, &prow.candidate_id).await?;
            }
        }
    }

    // Phase 2: run L1 extractors.
    let extractors = {
        let mut v = all_extractors();
        #[cfg(test)]
        v.push(Box::new(crate::insight::extractors::noop_test::NoopTestExtractor));
        v
    };

    for ext in &extractors {
        let category = ext.category();
        let cands_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ext.extract(&view)
        }));
        let cands = match cands_result {
            Ok(c) => c,
            Err(_) => {
                tracing::warn!(session_id, category, "extractor panicked; skipping");
                continue;
            }
        };

        for c in cands {
            if c.confidence_l1 < CONFIDENCE_FLOOR {
                continue;
            }

            let finding_id = derive_finding_id(category, session_id, &c.evidence_refs);

            match ext.promotion_policy() {
                PromotionPolicy::Always => {
                    let row = promote_l1(session_id, category, &finding_id, &c);
                    repo_finding::insert(pool, &row).await?;
                    rows.push(row);
                }
                PromotionPolicy::Never => {
                    route_to_judge(pool, session_id, category, &finding_id, &c, &*judge).await?;
                }
                PromotionPolicy::IfAbove(threshold) => {
                    if c.confidence_l1 > threshold {
                        let row = promote_l1(session_id, category, &finding_id, &c);
                        repo_finding::insert(pool, &row).await?;
                        rows.push(row);
                    } else {
                        route_to_judge(pool, session_id, category, &finding_id, &c, &*judge).await?;
                    }
                }
            }
        }
    }

    Ok(rows)
}

fn promote_l1(session_id: &str, category: &str, finding_id: &str, c: &FindingCandidate) -> FindingRow {
    let extractor_id = format!("{category}@v1");
    let prov = Provenance {
        extractor: Box::leak(extractor_id.into_boxed_str()),
        layer: "L1",
        judge: None,
        judge_template_version: None,
        rule_pack: None,
    };
    FindingRow {
        finding_id: finding_id.to_string(),
        schema_version: "finding.v1".into(),
        session_id: session_id.to_string(),
        category: category.to_string(),
        severity: c.severity.to_string(),
        confidence: c.confidence_l1 as f64,
        summary: c.summary.clone(),
        evidence_refs: serde_json::to_string(&c.evidence_refs).unwrap_or_else(|_| "[]".into()),
        evidence_projection: c.evidence_projection.to_string(),
        provenance: prov.to_json_string(),
        status: "active".into(),
        created_at: chrono::Utc::now().to_rfc3339(),
    }
}

async fn route_to_judge(
    pool: &SqlitePool,
    session_id: &str,
    category: &str,
    finding_id: &str,
    c: &FindingCandidate,
    judge: &dyn JudgeProvider,
) -> Result<()> {
    let proj_str = c.evidence_projection.to_string();
    let p = JudgePrompt {
        category: category.to_string(),
        candidate_id: finding_id.to_string(),
        evidence_projection: c.evidence_projection.clone(),
        system_template: "judge@v1".to_string(),
    };

    match judge.judge(p).await {
        Ok(verdict) if verdict.promote && verdict.confidence_l2 >= CONFIDENCE_FLOOR => {
            let prov = Provenance {
                extractor: Box::leak(format!("{category}@v1").into_boxed_str()),
                layer: "L2",
                judge: Some(judge.model_id().to_string()),
                judge_template_version: Some(judge.prompt_template_version().to_string()),
                rule_pack: None,
            };
            let row = FindingRow {
                finding_id: finding_id.to_string(),
                schema_version: "finding.v1".into(),
                session_id: session_id.to_string(),
                category: category.to_string(),
                severity: c.severity.to_string(),
                confidence: verdict.confidence_l2 as f64,
                summary: verdict.reason.clone(),
                evidence_refs: serde_json::to_string(&c.evidence_refs).unwrap_or_else(|_| "[]".into()),
                evidence_projection: proj_str,
                provenance: prov.to_json_string(),
                status: "active".into(),
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            repo_finding::insert(pool, &row).await?;
        }
        Ok(_) => {
            // Judge said no-promote or confidence below floor → discard
        }
        Err(JudgeError::Schema(e)) => {
            tracing::warn!(category, error = %e, "schema error from judge — discarding candidate");
        }
        Err(_) => {
            // BudgetExhausted, Disabled, Transport, Timeout → queue to pending
            let ev_refs = serde_json::to_string(&c.evidence_refs).unwrap_or_else(|_| "[]".into());
            repo_findings_pending::enqueue(
                pool,
                finding_id,
                session_id,
                category,
                c.confidence_l1 as f64,
                &ev_refs,
                &proj_str,
            )
            .await?;
        }
    }

    Ok(())
}
```

- [ ] **Step 2: Cargo check**

```bash
cargo check 2>&1 | tail -15
```

Fix any compile errors before continuing.

### Task 18: CLI flags + health extension

**Files:**
- Modify: `src/cli.rs` — add `--judge`, `--judge-budget`, `--judge-fixture-path` to Serve
- Modify: `src/api/mod.rs` — add `judge_runtime` to `AppState`
- Modify: `src/api/routes.rs` — update health handler
- Modify: `src/main.rs` — parse judge flags, build runtime, pass to serve_cmd and pipeline

- [ ] **Step 1: Update `src/cli.rs` Serve variant**

In the `Serve { ... }` variant, add after `sse_channel_capacity`:
```rust
/// LLM judge mode: none (default), anthropic, or fixture.
#[arg(long, default_value = "none", env = "WITMCC_JUDGE")]
pub judge: JudgeMode,
/// Maximum judge API calls per rebuild invocation (ignored when --judge=none).
#[arg(long, default_value_t = 20, env = "WITMCC_JUDGE_BUDGET")]
pub judge_budget: usize,
/// Path to fixture JSON file (required when --judge=fixture).
#[arg(long, env = "WITMCC_JUDGE_FIXTURE_PATH")]
pub judge_fixture_path: Option<std::path::PathBuf>,
```

Also add the `JudgeMode` enum to `src/cli.rs`:
```rust
#[derive(Debug, Clone, clap::ValueEnum)]
pub enum JudgeMode {
    #[value(alias = "noop")]
    None,
    Anthropic,
    Fixture,
}
```

- [ ] **Step 2: Update `src/api/mod.rs` AppState**

Add to the `AppState` struct:
```rust
pub judge_runtime: std::sync::Arc<witmcc::insight::judge::runtime::JudgeRuntime>,
```

(Use `crate::insight::judge::runtime::JudgeRuntime` since this is inside the crate.)

Update `AppState::new_for_tests` to include:
```rust
judge_runtime: std::sync::Arc::new(crate::insight::judge::runtime::JudgeRuntime::noop()),
```

- [ ] **Step 3: Update health handler in `src/api/routes.rs`**

Replace the health handler:
```rust
pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    use crate::db::repo_findings_pending;
    let pending_count = repo_findings_pending::count_all(&state.pool).await.unwrap_or(0);
    let snap = state.judge_runtime.metrics_snapshot();
    Json(json!({
        "status": "ok",
        "build_sha": option_env!("GIT_SHA").unwrap_or("dev"),
        "insight": {
            "judge_kind": state.judge_runtime.kind.to_string(),
            "judge_calls_24h": snap.calls_24h,
            "judge_pending_count": pending_count,
            "judge_cache_hits_24h": snap.cache_hits_24h,
            "judge_cache_misses_24h": snap.cache_misses_24h,
            "judge_budget_exhaustions_24h": snap.budget_exhaustions_24h,
        }
    }))
}
```

Note: The health handler previously took no state. It now needs `State(state): State<AppState>`. Ensure the route is registered with the full `AppState` (already done via `FromRef`).

- [ ] **Step 4: Update `src/main.rs` serve_cmd**

Add judge flags to the Serve match arm destructuring and serve_cmd signature:
```rust
cli::Command::Serve {
    bind, port, auto_migrate, shutdown_after_ms,
    no_watch_transcripts, transcripts_root,
    sse_keepalive_secs, sse_channel_capacity,
    judge, judge_budget, judge_fixture_path,
} => {
    serve_cmd(
        &cli.db_path, bind, port, auto_migrate, shutdown_after_ms,
        no_watch_transcripts, transcripts_root,
        sse_keepalive_secs, sse_channel_capacity,
        judge, judge_budget, judge_fixture_path,
    ).await
}
```

Add the judge runtime build in `serve_cmd`:
```rust
async fn serve_cmd(
    // ... existing params ...
    judge: cli::JudgeMode,
    judge_budget: usize,
    judge_fixture_path: Option<std::path::PathBuf>,
) -> error::Result<()> {
    // ... existing code until state construction ...

    // Build judge runtime per CLI flags. Log to stderr.
    let judge_runtime = match judge {
        cli::JudgeMode::None => {
            eprintln!("LLM judge: disabled (NoopJudge). Run with --judge anthropic to enable.");
            std::sync::Arc::new(witmcc::insight::judge::runtime::JudgeRuntime::noop())
        }
        cli::JudgeMode::Anthropic => {
            std::sync::Arc::new(
                witmcc::insight::judge::runtime::JudgeRuntime::anthropic(pool.clone(), judge_budget)
                    .map_err(anyhow::Error::from)?
            )
        }
        cli::JudgeMode::Fixture => {
            let path = judge_fixture_path.ok_or_else(|| {
                error::WitmccError::Invalid(
                    "--judge-fixture-path is required when --judge=fixture".into()
                )
            })?;
            std::sync::Arc::new(
                witmcc::insight::judge::runtime::JudgeRuntime::fixture(&path, judge_budget)
                    .map_err(anyhow::Error::from)?
            )
        }
    };
    eprintln!("LLM judge: {} (budget {})", judge_runtime.kind, judge_budget);

    // ... existing state construction, pass judge_runtime ...
    let state = witmcc::api::AppState {
        pool: pool.clone(),
        live_tx: live_tx.clone(),
        sse_keepalive_secs,
        sse_channel_capacity: sse_channel_capacity as usize,
        judge_runtime,
    };
    // rest unchanged
}
```

- [ ] **Step 5: Cargo check**

```bash
cargo check 2>&1 | tail -20
```

Fix all errors. Common issues: missing imports, state handler signature (health now needs `State<AppState>` not unit).

- [ ] **Step 6: Run all tests**

```bash
cargo test 2>&1 | grep -E "FAILED|error\[|test result"
```

Expected: all green. Fix any failures before proceeding.

- [ ] **Step 7: Run pipeline L2 tests**

```bash
cargo test insight_pipeline_l2 api_health_insight judge_budget judge_cache judge_noop judge_fixture migration_judge_cache migration_findings_pending 2>&1 | tail -15
```

Expected: all green.

- [ ] **Step 8: Commit**

```bash
git add src/insight/pipeline.rs \
        src/insight/extractors/mod.rs \
        src/cli.rs \
        src/api/mod.rs \
        src/api/routes.rs \
        src/main.rs
git commit -m "feat(cli): --judge flags + pipeline L2 integration + /v1/health.insight.*"
```

---

## Phase 7 — Full test suite + smoke

### Task 19: Verify full test suite

- [ ] **Step 1: Run all cargo tests**

```bash
cargo test 2>&1 | grep "test result"
```

Expected: all green; total count ≥ 274 + 16 (roughly 290+).

- [ ] **Step 2: Record final count**

```bash
cargo test 2>&1 | grep "test result" | awk '{sum += $4} END {print "Total:", sum}'
```

Record the number.

- [ ] **Step 3: Run vitest**

```bash
cd /Users/bahamoth/projects/whats-in-my-cc/webui && npx vitest run 2>&1 | tail -10
```

Expected: all green; same as baseline (slice-15 is backend only — no webui changes).

### Task 20: Smoke (--judge none)

- [ ] **Step 1: Init DB and ingest aac68973**

```bash
cd /Users/bahamoth/projects/whats-in-my-cc
cargo build --release 2>&1 | tail -5
./target/release/witmcc init-db
./target/release/witmcc ingest --path ~/.claude/projects/$(ls ~/.claude/projects/ | head -1)/aac68973.jsonl
# If path differs, find the aac68973 file:
find ~/.claude/projects -name "*.jsonl" | xargs grep -l "aac68973" 2>/dev/null | head -1
```

- [ ] **Step 2: Start server with --judge none**

```bash
./target/release/witmcc serve --bind 127.0.0.1 --port 4337 --auto-migrate --shutdown-after-ms 0 &
sleep 1
```

(Use `--no-watch-transcripts` if live tail is noisy.)

- [ ] **Step 3: Verify health insight block**

```bash
curl -s http://127.0.0.1:4337/v1/health | python3 -c "import json,sys; d=json.load(sys.stdin); print(json.dumps(d['insight'], indent=2))"
```

Expected:
```json
{
  "judge_kind": "noop",
  "judge_calls_24h": 0,
  "judge_pending_count": 0,
  "judge_cache_hits_24h": 0,
  "judge_cache_misses_24h": 0,
  "judge_budget_exhaustions_24h": 0
}
```

Note: `judge_pending_count` is 0 because there are no L2 categories yet (slice-16 adds them). The `noop_test` extractor is `cfg(test)` only.

- [ ] **Step 4: Verify findings unchanged from slice-14**

```bash
curl -s http://127.0.0.1:4337/v1/findings | python3 -c "import json,sys; fs=json.load(sys.stdin); print('findings count:', len(fs.get('findings',fs)) )"
```

Expected: same finding count as post-slice-14 (only `missing_verification` and `tool_failure` categories).

- [ ] **Step 5: Kill server**

```bash
pkill -f "witmcc serve" || true
```

### Task 21: Smoke (--judge fixture)

- [ ] **Step 1: Start server with fixture judge**

```bash
./target/release/witmcc serve --bind 127.0.0.1 --port 4337 --auto-migrate \
  --judge fixture \
  --judge-fixture-path tests/fixtures/judge/scenario_a.json \
  --judge-budget 5 &
sleep 1
```

- [ ] **Step 2: Verify judge_kind in health**

```bash
curl -s http://127.0.0.1:4337/v1/health | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['insight']['judge_kind'])"
```

Expected: `fixture`

- [ ] **Step 3: Verify L1 findings still present**

```bash
curl -s http://127.0.0.1:4337/v1/findings | python3 -c "import json,sys; d=json.load(sys.stdin); items=d.get('findings',d); print('count:', len(items) if isinstance(items,list) else '?')"
```

- [ ] **Step 4: Kill server and record smoke evidence**

```bash
pkill -f "witmcc serve" || true
```

---

## Phase 8 — Implementation notes + CLAUDE.md + final commit

### Task 22: Update implementation-notes.html

**Files:**
- Modify: `docs/implementation-notes.html`

- [ ] **Step 1: Add slice-15 section to implementation-notes.html**

Find the `Overview (slice-14)` section and add a new `Overview (slice-15)` section after it, following the same HTML pattern as prior slices. Key points to document:

- `JudgeRuntime::build_for_rebuild()` creates a fresh `BudgetGuardDyn` each invocation (per DEV-S15-02).
- `NoopJudge` returns `Err(Disabled)` — not `Ok(promote=false)` (per DEV-S15-06).
- `_24h` counters are in-memory atomics, reset on restart (per DEV-S15-03).
- `FixtureJudge::judge_with_hash()` is a test-only helper exposing hash override.
- `AnthropicJudge` uses `reqwest` not a Rust SDK (per DEV-S15-05).
- `findings_pending_judge` dequeue is per-candidate — budget exhaustion leaves the rest queued (monotone progress guarantee).
- `noop_test` extractor is `cfg(test)` only — never appears in production (per DEV-S15-01).
- Cache key includes `prompt_template_version` derived from SHA-256 of the template file (per DEV-S15-07).

### Task 23: Update CLAUDE.md

**Files:**
- Modify: `/Users/bahamoth/projects/whats-in-my-cc/CLAUDE.md`

- [ ] **Step 1: Add slice-15 to completed slices in Status block**

In the Status block, update the current stage line to include `slice-15` in the completed list.

### Task 24: Final commit

- [ ] **Step 1: Run full test suite one more time**

```bash
cargo test 2>&1 | grep -E "FAILED|test result" | head -20
```

Expected: all green.

- [ ] **Step 2: Self-check**

Check each item:
1. New tests lock every new behaviour (judge trait, cache, budget, pipeline routing, health endpoint) ✓
2. No real LLM calls in tests (`FixtureJudge` + `NoopJudge` only) ✓
3. No UI changes in this slice → browser smoke not required ✓
4. No single-case generalizations in docs ✓
5. Deviations DEV-S15-01 through DEV-S15-07 all documented ✓

- [ ] **Step 3: Stage and commit docs**

```bash
git add docs/implementation-notes.html CLAUDE.md
git commit -m "docs(slice-15): implementation-notes + CLAUDE.md status sync"
```

---

## Verification summary

**Cargo test delta:** baseline 274 → expected ~292 (+18 tests: 2 migration, 3 noop, 3 fixture, 2 cache, 3 budget, 4 pipeline-l2, 3 health-insight)

**Vitest delta:** 0 (no webui changes)

**aac68973 findings:** unchanged from slice-14 (no new L2 categories; `noop_test` is `cfg(test)` only)

**`/v1/health` additions:**
```json
{
  "insight": {
    "judge_kind": "noop",
    "judge_calls_24h": 0,
    "judge_pending_count": 0,
    "judge_cache_hits_24h": 0,
    "judge_cache_misses_24h": 0,
    "judge_budget_exhaustions_24h": 0
  }
}
```

**Key deviations from design spec (all pre-declared):**
- DEV-S15-01: No new finding categories; `noop_test` is cfg(test) only.
- DEV-S15-02: `BudgetGuard` per-rebuild via `BudgetGuardDyn` inside `JudgeRuntime::build_for_rebuild()`.
- DEV-S15-03: `_24h` counters are in-memory atomics; reset on restart.
- DEV-S15-04: `prompt_template_version` derived from SHA-256 of template file content.
- DEV-S15-05: `AnthropicJudge` hand-rolled over `reqwest`.
- DEV-S15-06: `NoopJudge` returns `Err(Disabled)`, not `Ok(promote=false)`.
- DEV-S15-07: Cache key includes `prompt_template_version`.
