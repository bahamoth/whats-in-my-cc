# Slice-16 Implementation Plan — Insight L2 Categories

**Spec:** `docs/superpowers/specs/2026-05-27-witmcc-slice16-insight-l2-categories-design.md`
**Branch:** `slice16-insight-l2-categories`

---

## Phase 0 — Branch & baseline

| 0a | Cut `slice16-insight-l2-categories` off slice-15 merge |
| 0b | Record cargo + vitest baseline counts |
| 0c | Verify real-data fixtures for context_bloat / final_state_mismatch are still present in `~/.claude/projects/`. If not, record DEV note. |

---

## Phase 1 — Red-locking tests + gold fixtures

### Task 1 — Updated registry shape

```rust
// tests/insight_registry.rs (replace previous content)
#[test]
fn registry_contains_all_mvp_categories_in_locked_order() {
    let cats: Vec<&str> = witmcc::insight::registry::all_extractors()
        .iter().map(|e| e.category()).collect();
    assert_eq!(cats, vec![
        "missing_verification",
        "tool_failure",
        "risky_action",
        "context_bloat",
        "final_state_mismatch",
    ]);
}
```

### Task 2 — Per-category unit tests

Three test files, each mirroring slice-14's structure:

```rust
// tests/extractor_risky_action.rs
#[test]
fn fires_on_destructive_bash() {
    let view = synth_view_with_bash("rm -rf /tmp/foo");
    let c = RiskyAction.extract(&view);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].confidence_l1, 0.7);
}
#[test]
fn fires_on_user_modified_hunk() {
    let view = synth_view_with_user_modified_hunk();
    let c = RiskyAction.extract(&view);
    assert_eq!(c.len(), 1);
}
#[test]
fn does_not_fire_on_safe_bash() {
    let view = synth_view_with_bash("ls -la");
    let c = RiskyAction.extract(&view);
    assert!(c.is_empty());
}
```

```rust
// tests/extractor_context_bloat.rs
#[test]
fn fires_on_large_tool_result_with_no_downstream_use() {
    let view = synth_view_with_bloat(payload_size: 100_000, downstream_overlap: 0);
    let c = ContextBloat.extract(&view);
    assert_eq!(c.len(), 1);
}
#[test]
fn does_not_fire_when_downstream_uses_content() {
    let view = synth_view_with_bloat(payload_size: 100_000, downstream_overlap: 5);
    let c = ContextBloat.extract(&view);
    assert!(c.is_empty());
}
#[test]
fn does_not_fire_below_threshold() {
    let view = synth_view_with_bloat(payload_size: 10_000, downstream_overlap: 0);
    let c = ContextBloat.extract(&view);
    assert!(c.is_empty());
}
```

```rust
// tests/extractor_final_state_mismatch.rs
#[test]
fn fires_when_goal_unmet_and_no_completion_marker() {
    let view = synth_view_user_goal_then_failed_test();
    let c = FinalStateMismatch.extract(&view);
    assert_eq!(c.len(), 1);
}
#[test]
fn does_not_fire_when_closing_verification_passed() {
    let view = synth_view_user_goal_then_passing_test();
    let c = FinalStateMismatch.extract(&view);
    assert!(c.is_empty());
}
#[test]
fn fires_at_most_once_per_session() {
    let view = synth_view_with_two_goals_and_two_failures();
    let c = FinalStateMismatch.extract(&view);
    assert!(c.len() <= 1);
}
```

### Task 3 — Projection unit tests

```rust
// tests/projection_risky_action.rs
#[test]
fn projection_includes_required_fields() {
    let view = synth_view_with_bash("rm -rf /tmp/foo");
    let c = RiskyAction.extract(&view).into_iter().next().unwrap();
    let proj = RiskyAction.project(&c, &view);
    assert_eq!(proj["category"], "risky_action");
    assert!(proj["trigger"]["command_redacted"].is_string());
    assert!(proj["context"]["episode_phase"].is_string());
}
```

(One projection test per category — same shape.)

### Task 4 — Gold fixtures

Place these files (positive + negative for each category):

```
tests/fixtures/transcripts/curated/risky_action_positive.jsonl
tests/fixtures/transcripts/curated/risky_action_negative.jsonl
tests/fixtures/transcripts/real/context_bloat_v01.jsonl       (if real available)
tests/fixtures/transcripts/curated/context_bloat_negative.jsonl
tests/fixtures/transcripts/real/final_state_mismatch_v01.jsonl  (if real available)
tests/fixtures/transcripts/curated/final_state_mismatch_negative.jsonl

tests/fixtures/judge/risky_action_gold.json
tests/fixtures/judge/context_bloat_gold.json
tests/fixtures/judge/final_state_mismatch_gold.json

tests/fixtures/insight_gold/risky_action_positive.json
tests/fixtures/insight_gold/risky_action_negative.json
tests/fixtures/insight_gold/context_bloat_positive.json
tests/fixtures/insight_gold/context_bloat_negative.json
tests/fixtures/insight_gold/final_state_mismatch_positive.json
tests/fixtures/insight_gold/final_state_mismatch_negative.json
```

### Task 5 — End-to-end test

```rust
// tests/insight_e2e_l2.rs
async fn run_e2e_gold_check(category: &str, positive: bool) {
    let suffix = if positive { "positive" } else { "negative" };
    let gold: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(
        format!("tests/fixtures/insight_gold/{category}_{suffix}.json")
    ).unwrap()).unwrap();

    let pool = test_pool().await;
    ingest_transcript_fixture(&pool, gold["session_fixture"].as_str().unwrap()).await;
    rebuild_session(&pool, "sess_e2e").await.unwrap();
    let runtime = build_runtime_with_fixture_judge(gold["judge_fixture"].as_str().unwrap());
    pipeline::run_extractors_with_runtime(&pool, "sess_e2e", &runtime).await.unwrap();

    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM finding WHERE session_id='sess_e2e' AND category=?"
    ).bind(category).fetch_one(&pool).await.unwrap();
    let want = gold["expected_findings"].as_array().unwrap().len() as i64;
    assert_eq!(n, want, "{category} {suffix}: got {n} want {want}");
}

#[tokio::test] async fn risky_action_positive() { run_e2e_gold_check("risky_action", true).await; }
#[tokio::test] async fn risky_action_negative() { run_e2e_gold_check("risky_action", false).await; }
#[tokio::test] async fn context_bloat_positive() { run_e2e_gold_check("context_bloat", true).await; }
#[tokio::test] async fn context_bloat_negative() { run_e2e_gold_check("context_bloat", false).await; }
#[tokio::test] async fn final_state_mismatch_positive() { run_e2e_gold_check("final_state_mismatch", true).await; }
#[tokio::test] async fn final_state_mismatch_negative() { run_e2e_gold_check("final_state_mismatch", false).await; }
```

**Commit 1:** `test(slice-16): red-locking tests + gold fixtures for 3 L2 categories`

---

## Phase 2 — RiskyAction

| 6 | `src/insight/extractors/risky_action.rs` per spec §3 | unit tests green |
| 7 | `EvidenceProjection` impl for RiskyAction | projection test green |
| 8 | Append `risky_action` prompt block to `judge_v1.txt`; bump `prompt_template_version` if content hash changes (slice-15's `judge@v1#{hash}` derivation auto-handles it) | n/a |

**Commit 2:** `feat(insight): RiskyAction extractor + projection + prompt block`

---

## Phase 3 — ContextBloat

| 9  | `src/insight/extractors/context_bloat.rs` per spec §4 | unit tests green |
| 10 | Projection + prompt block | green |

**Commit 3:** `feat(insight): ContextBloat extractor + projection + prompt block`

---

## Phase 4 — FinalStateMismatch

| 11 | `src/insight/extractors/final_state_mismatch.rs` per spec §5 | unit tests green |
| 12 | Projection + prompt block | green |

**Commit 4:** `feat(insight): FinalStateMismatch extractor + projection + prompt block`

---

## Phase 5 — Redaction shim

| 13 | `src/insight/redaction_shim.rs` — `apply_text(text: &str) -> String` no-op + tracing warn `redaction_shim_invoked` (per spec §9 DEV-S16-05) |
| 14 | Wire shim into each projection's `_redacted` field generation |
| 15 | Add `tests/redaction_shim_lock.rs` asserting shim is a no-op (so slice-18's replacement is detectable) |

**Commit 5:** `feat(insight): redaction_shim placeholder + projection wiring`

---

## Phase 6 — Registry + e2e

| 16 | Update `src/insight/registry.rs` to include the three new extractors in the spec-locked order | registry test green |
| 17 | E2E tests in `tests/insight_e2e_l2.rs` all green (with `FixtureJudge`) |

**Commit 6:** `feat(insight): registry update + e2e L2 category tests`

---

## Phase 7 — Smoke + verification

```
Smoke — slice-16 (fixture path)

[ ] witmcc serve --judge fixture --judge-fixture-path tests/fixtures/judge/scenario_combined.json --port 4337 &
[ ] ingest the three slice-16 curated/real transcripts as separate sessions
[ ] curl -s http://127.0.0.1:4337/v1/findings | jq '[.data[] | .category] | group_by(.) | map({c: .[0], n: length})'
    # Expected: 5 categories, including the 3 new ones with non-zero counts on the positive fixtures
```

```
Smoke — slice-16 (noop judge)

[ ] witmcc serve --judge none --port 4337 &
[ ] ingest the three positive fixtures
[ ] curl -s http://127.0.0.1:4337/v1/findings?status=pending | jq 'length'
    # Expected: ≥ 3 (one per positive fixture, all queued)
[ ] curl -s http://127.0.0.1:4337/v1/findings?status=active | jq '[.data[] | .category] | unique'
    # Expected: subset of ["missing_verification", "tool_failure"] (L1 only)
```

```
Smoke — slice-16 (anthropic, optional, costs money)

[ ] export ANTHROPIC_API_KEY=...
[ ] witmcc serve --judge anthropic --judge-budget 3 --port 4337 &
[ ] ingest the three positive fixtures
[ ] curl -s http://127.0.0.1:4337/v1/health | jq '.insight.judge_calls_24h'
    # Expected: ≤ 3 (one per category)
[ ] Cost estimate: ≤ $0.05 total for this smoke
```

```
Verification — slice-16

- cargo test count: baseline (post slice-15) → expected + 18..22
- aac68973 findings: post-slice-14 + 0~3 new (depending on what aac68973 contains)
- All 6 e2e gold tests green (3 categories × {positive, negative})
- M5 (Insight engine) is now closed
```

---

## Phase 8 — PR

Title: `feat(slice-16): Insight L2 categories — risky_action + context_bloat + final_state_mismatch`. Implementation-notes update. CLAUDE.md update marking M5 closed.
