# Slice-14 Implementation Plan — Insight L1

**Spec:** `docs/superpowers/specs/2026-05-27-witmcc-slice14-insight-l1-design.md`
**Architecture:** `docs/superpowers/specs/2026-05-27-witmcc-insight-engine-architecture.md`
**Branch:** `slice14-insight-l1`

---

## Phase 0 — Branch & baseline

| 0a | Cut `slice14-insight-l1` off slice-13 merge |
| 0b | Record baseline counts and `aac68973` rebuild latency |

---

## Phase 1 — Red-locking tests

### Task 1 — Schema invariant

**Files:** `tests/migration_finding_schema.rs`.

```rust
#[tokio::test]
async fn migration_creates_finding_table() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let cols: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM pragma_table_info('finding')"
    ).fetch_all(&pool).await.unwrap();
    for c in ["finding_id","schema_version","session_id","category","severity",
              "confidence","summary","evidence_refs","evidence_projection",
              "provenance","status","created_at"] {
        assert!(cols.iter().any(|n| n == c), "missing column {c}");
    }
}

#[tokio::test]
async fn finding_default_status_is_active() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO finding (finding_id,session_id,category,severity,confidence,summary,evidence_refs,evidence_projection,provenance) VALUES (?,?,?,?,?,?,?,?,?)"
    )
    .bind("find_x").bind("sess_x").bind("missing_verification").bind("medium")
    .bind(0.9).bind("").bind("[]").bind("{}").bind("{}").execute(&pool).await.unwrap();
    let status: String = sqlx::query_scalar("SELECT status FROM finding WHERE finding_id='find_x'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(status, "active");
}
```

### Task 2 — Extractor trait + registry invariant

**Files:** `tests/insight_registry.rs`.

```rust
use witmcc::insight::registry::all_extractors;

#[test]
fn registry_contains_l1_categories_only_for_slice14() {
    let cats: Vec<&str> = all_extractors().iter().map(|e| e.category()).collect();
    let expected = vec!["missing_verification", "tool_failure"];
    assert_eq!(cats, expected, "registry order/content must match expected");
}
```

Stub `src/insight/registry.rs` with empty Vec. Test fails.

### Task 3 — `MissingVerification` extractor unit test

**Files:** `tests/extractor_missing_verification.rs`.

```rust
#[test]
fn fires_when_action_has_no_following_verification() {
    let view = synth_view_action_episode_without_verification();
    let cands = witmcc::insight::extractors::missing_verification::MissingVerification
        .extract(&view);
    assert_eq!(cands.len(), 1);
    let c = &cands[0];
    assert_eq!(c.category, "missing_verification");
    assert!(c.confidence_l1 >= 0.9);
    assert!(c.evidence_refs.len() >= 2);
}

#[test]
fn does_not_fire_when_verification_follows() {
    let view = synth_view_action_followed_by_verification();
    let cands = witmcc::insight::extractors::missing_verification::MissingVerification
        .extract(&view);
    assert!(cands.is_empty());
}

#[test]
fn does_not_fire_for_read_only_session() {
    let view = synth_view_read_only();
    let cands = witmcc::insight::extractors::missing_verification::MissingVerification
        .extract(&view);
    assert!(cands.is_empty());
}

#[test]
fn fires_for_trailing_action_at_session_end() {
    let view = synth_view_action_at_end();
    let cands = witmcc::insight::extractors::missing_verification::MissingVerification
        .extract(&view);
    assert_eq!(cands.len(), 1);
}
```

### Task 4 — `ToolFailure` extractor unit test

**Files:** `tests/extractor_tool_failure.rs`.

```rust
#[test]
fn fires_on_is_error_true_with_no_retry() {
    let view = synth_view_one_error_no_retry();
    let cands = witmcc::insight::extractors::tool_failure::ToolFailure.extract(&view);
    assert_eq!(cands.len(), 1);
    assert_eq!(cands[0].confidence_l1, 1.0);
}

#[test]
fn does_not_fire_if_same_tool_use_succeeds_within_5_events() {
    let view = synth_view_error_then_success();
    let cands = witmcc::insight::extractors::tool_failure::ToolFailure.extract(&view);
    assert!(cands.is_empty());
}

#[test]
fn fires_if_no_is_error_field() {
    let view = synth_view_tool_result_without_is_error();
    let cands = witmcc::insight::extractors::tool_failure::ToolFailure.extract(&view);
    // Spec §5 edge case: missing field treated as false → no fire
    assert!(cands.is_empty());
}
```

### Task 5 — Pipeline integration

**Files:** `tests/insight_pipeline.rs`.

```rust
#[tokio::test]
async fn pipeline_writes_finding_rows() {
    let pool = test_pool_seeded_with_failing_session().await;
    let findings = witmcc::insight::pipeline::run_extractors(&pool, "sess_t").await.unwrap();
    assert!(!findings.is_empty());
    // Re-run is idempotent
    let again = witmcc::insight::pipeline::run_extractors(&pool, "sess_t").await.unwrap();
    assert_eq!(findings.len(), again.len());
}

#[tokio::test]
async fn pipeline_dedupes_via_finding_id() {
    let pool = test_pool_seeded_with_failing_session().await;
    witmcc::insight::pipeline::run_extractors(&pool, "sess_t").await.unwrap();
    let count_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM finding")
        .fetch_one(&pool).await.unwrap();
    witmcc::insight::pipeline::run_extractors(&pool, "sess_t").await.unwrap();
    let count_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM finding")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(count_before, count_after);
}
```

### Task 6 — API endpoints (red)

**Files:** `tests/api_findings.rs`.

```rust
#[tokio::test]
async fn list_findings_endpoint() {
    let pool = pool_with_seeded_findings().await;
    let server = axum_test::TestServer::new(witmcc::api::build_router(pool)).unwrap();
    let r = server.get("/v1/findings").await;
    r.assert_status_ok();
    let body: serde_json::Value = r.json();
    assert!(body["data"].as_array().unwrap().len() >= 2);
}

#[tokio::test]
async fn finding_detail_includes_evidence_projection_and_provenance() {
    let pool = pool_with_seeded_findings().await;
    let server = axum_test::TestServer::new(witmcc::api::build_router(pool)).unwrap();
    let r = server.get("/v1/findings/find_demo_001").await;
    r.assert_status_ok();
    let body: serde_json::Value = r.json();
    assert_eq!(body["data"]["provenance"]["layer"], "L1");
    assert!(body["data"]["evidence_projection"].is_object());
    assert!(body["data"]["provenance"]["judge"].is_null());
}

#[tokio::test]
async fn evidence_endpoint_returns_subgraph_and_raw_refs() {
    let pool = pool_with_seeded_findings_and_graph().await;
    let server = axum_test::TestServer::new(witmcc::api::build_router(pool)).unwrap();
    let r = server.get("/v1/findings/find_demo_001/evidence").await;
    r.assert_status_ok();
    let body: serde_json::Value = r.json();
    assert!(body["data"]["subgraph"]["nodes"].as_array().unwrap().len() >= 1);
    assert!(body["data"]["raw_source_refs"].as_array().unwrap().len() >= 1);
}

#[tokio::test]
async fn session_findings_alias_returns_same_rows() {
    let pool = pool_with_seeded_findings().await;
    let server = axum_test::TestServer::new(witmcc::api::build_router(pool)).unwrap();
    let r1 = server.get("/v1/sessions/sess_demo/findings").await.json::<serde_json::Value>();
    let r2 = server.get("/v1/findings?session_id=sess_demo").await.json::<serde_json::Value>();
    assert_eq!(r1["data"].as_array().unwrap().len(), r2["data"].as_array().unwrap().len());
}
```

### Task 7 — Provenance lock

**Files:** `tests/insight_provenance.rs`.

```rust
#[tokio::test]
async fn l1_finding_has_null_judge() {
    let pool = pool_with_seeded_l1_finding().await;
    let row: (String,) = sqlx::query_as("SELECT provenance FROM finding WHERE finding_id='find_demo_001'")
        .fetch_one(&pool).await.unwrap();
    let prov: serde_json::Value = serde_json::from_str(&row.0).unwrap();
    assert_eq!(prov["layer"], "L1");
    assert!(prov["judge"].is_null());
    assert_eq!(prov["extractor"], "missing_verification@v1");
}
```

**Commit 1:** `test(slice-14): red-locking tests for insight pipeline + finding schema`

---

## Phase 2 — DB migration + repo

| 8 | `migrations/20260530120000_0008_finding.sql` per spec §3 | schema test green |
| 9 | `src/db/repo_finding.rs`: `insert(row) / list(filter) / get(id) / list_by_session(sid)` | roundtrip tests green |

**Commit 2:** `feat(db): 0008_finding migration + repo_finding`

---

## Phase 3 — Trait + registry + view loader

| 10 | `src/insight/types.rs` — `FindingCandidate`, `Provenance`, `Layer` enums |
| 11 | `src/insight/extractor.rs` — `InsightExtractor` trait + `PromotionPolicy` enum per arch spec §4 |
| 12 | `src/insight/view.rs` — `SessionInsightView::load(pool, session_id) -> Self` |
| 13 | `src/insight/registry.rs` — `all_extractors() -> Vec<Box<dyn InsightExtractor>>` returning the two stubs |

Insight registry test now compiles but still fails (returns empty).

**Commit 3:** `feat(insight): InsightExtractor trait + SessionInsightView + registry (stubs)`

---

## Phase 4 — Two extractor bodies

| 14 | `src/insight/extractors/missing_verification.rs` — rule per spec §4 | unit tests green |
| 15 | `src/insight/extractors/tool_failure.rs` — rule per spec §5 | unit tests green |
| 16 | Update registry with both extractors; insight_registry test green | green |

**Commit 4:** `feat(insight): MissingVerification + ToolFailure extractors (L1)`

---

## Phase 5 — Pull API + Pipeline

| 17 | `src/insight/pipeline.rs::run_extractors(pool, session_id)` per spec §6 | pipeline tests green |
| 18 | Route handlers in `src/api/routes.rs`: `list_findings`, `finding_detail`, `finding_evidence`, `session_findings` | api tests green |
| 19 | Register routes in `src/api/mod.rs` | n/a |

**Commit 5:** `feat(api): /v1/findings* + pipeline runner`

---

## Phase 6 — Wire into rebuild_session

| 20 | Widen `rebuild_session` return tuple to include `findings.len()` per spec §6 | update tests in `tests/graph_build.rs`, `tests/graph_diff_hunk_node.rs`, `tests/graph_atomicity.rs` to ignore the third member or assert ≥ 0 |
| 21 | Smoke: re-ingest aac68973, rebuild, count findings | ≥ 1 expected post-slice-11 (aac68973 has known Bash failures) |

**Commit 6:** `feat(graph): wire insight pipeline into rebuild_session`

---

## Phase 7 — Smoke + verification

```
Smoke — slice-14

[ ] init-db; ingest aac68973
[ ] witmcc serve --port 4337 &
[ ] curl -s http://127.0.0.1:4337/v1/sessions/aac68973/findings | jq 'length'
    # Expected: ≥ 1, includes at least one missing_verification or tool_failure
[ ] curl -s http://127.0.0.1:4337/v1/findings | jq '[.data[] | .category] | group_by(.) | map({c: .[0], n: length})'
    # Expected: counts per category
[ ] curl -s http://127.0.0.1:4337/v1/findings/<id>/evidence | jq '.data | {subgraph_nodes: (.subgraph.nodes|length), raw_refs: (.raw_source_refs|length)}'
    # Expected: both non-zero
```

```
Verification — slice-14

- cargo test count: baseline (post slice-13) → expected + 16..20
- aac68973 findings: ≥ 1 per category that fires (record actual counts)
- aac68973 rebuild latency: ≤ 1.25 × baseline
```

---

## Phase 8 — PR

Title: `feat(slice-14): Insight engine L1 (missing_verification + tool_failure)`. Implementation-notes entry. CLAUDE.md status update.
