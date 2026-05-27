# Slice-13 Implementation Plan — Causal-edge Inference

**Spec:** `docs/superpowers/specs/2026-05-27-witmcc-slice13-causal-edge-inference-design.md`
**Branch:** `slice13-causal-edge-inference`

---

## Phase 0 — Branch & baseline

| 0a | `git checkout -b slice13-causal-edge-inference` after slice-12 merged |
| 0b | Record baseline counts; specifically record current edge count for aac68973 (`compute()` output) |

---

## Phase 1 — Red-locking tests

### Task 1 — Rule registry

**Files:** `tests/edge_inference_registry.rs`.

```rust
use witmcc::insight::edge_inference::RULE_IDS;

#[test]
fn rule_ids_are_canonical() {
    let expected = &[
        "caused_repair@v1",
        "triggered_by_user_message@v1",
        "large_output_to_next_action@v1",
    ];
    assert_eq!(RULE_IDS, expected);
}
```

Stub `src/insight/edge_inference/mod.rs` with `pub const RULE_IDS: &[&str] = &[];`. Test fails.

### Task 2 — Per-rule unit tests

**Files:** `tests/rule_caused_repair.rs`, `tests/rule_triggered_by_user_message.rs`, `tests/rule_large_output.rs`.

Each test file constructs a small synthetic event stream and asserts the rule produces exactly the expected edge(s).

```rust
// tests/rule_caused_repair.rs (abbrev)
#[test]
fn emits_edge_when_error_text_overlaps_next_call_input() {
    let view = synth_view_with_error_then_repair(
        "AttributeError: 'User' object has no attribute 'is_admin'",
        "fix the is_admin attribute on User",
    );
    let edges = witmcc::insight::edge_inference::rules::caused_repair_v1::CausedRepairV1
        .infer(&view);
    assert_eq!(edges.len(), 1);
    let e = &edges[0];
    assert_eq!(e.inference_rule_id.as_deref(), Some("caused_repair@v1"));
    let c = e.confidence.unwrap();
    assert!((c - 0.7..0.95).contains(&c), "confidence {c} out of range");
}

#[test]
fn does_not_emit_when_no_token_overlap() {
    let view = synth_view_with_error_then_repair(
        "boom",
        "list files in /tmp",
    );
    let edges = witmcc::insight::edge_inference::rules::caused_repair_v1::CausedRepairV1
        .infer(&view);
    assert!(edges.is_empty());
}

#[test]
fn does_not_emit_when_repair_too_late() {
    let view = synth_view_with_error_then_delayed_repair(120);  // 120 s gap
    let edges = witmcc::insight::edge_inference::rules::caused_repair_v1::CausedRepairV1
        .infer(&view);
    assert!(edges.is_empty(), "delayed repair must be ignored");
}
```

```rust
// tests/rule_triggered_by_user_message.rs (abbrev)
#[test]
fn emits_edge_when_assistant_skipped_text() {
    let view = synth_user_then_tool_no_assistant_text();
    let edges = witmcc::insight::edge_inference::rules::triggered_by_user_message_v1
        ::TriggeredByUserMessageV1.infer(&view);
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].confidence.unwrap(), 0.85);
}

#[test]
fn does_not_emit_when_assistant_text_preceded() {
    let view = synth_user_then_assistant_text_then_tool();
    let edges = witmcc::insight::edge_inference::rules::triggered_by_user_message_v1
        ::TriggeredByUserMessageV1.infer(&view);
    assert!(edges.is_empty());
}
```

```rust
// tests/rule_large_output.rs (abbrev)
#[test]
fn emits_edge_when_payload_exceeds_threshold() {
    let view = synth_tool_result_then_assistant_msg(payload_bytes: 100_000);
    let edges = witmcc::insight::edge_inference::rules::large_output_to_next_action_v1
        ::LargeOutputToNextActionV1.infer(&view);
    assert_eq!(edges.len(), 1);
    let attrs = &edges[0].attributes;
    assert!(attrs.get("tool_result_size_bytes").and_then(|v| v.as_i64()).unwrap() == 100_000);
}

#[test]
fn does_not_emit_when_payload_below_threshold() {
    let view = synth_tool_result_then_assistant_msg(payload_bytes: 10);
    let edges = witmcc::insight::edge_inference::rules::large_output_to_next_action_v1
        ::LargeOutputToNextActionV1.infer(&view);
    assert!(edges.is_empty());
}
```

Helpers go into `tests/common/edge_inference.rs`.

Stub all three rule files with skeletons returning `Vec::new()`. Tests fail.

### Task 3 — Migration schema test

**Files:** `tests/migration_inference_columns.rs`.

```rust
#[tokio::test]
async fn migration_adds_inference_columns_to_graph_edge() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let cols: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM pragma_table_info('graph_edge')"
    ).fetch_all(&pool).await.unwrap();
    for c in ["inference_rule_id", "confidence"] {
        assert!(cols.iter().any(|n| n == c), "missing column {c}");
    }
}
```

Migration file not yet present — test panics.

### Task 4 — Counts-golden test (empty golden — armed at commit 4)

**Files:** `tests/fixtures/inferred_edge_counts.json` (with empty `by_session_and_rule: {}`), `tests/inferred_edge_counts.rs`.

```rust
#[test]
fn counts_match_golden_for_each_session() {
    let want: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string("tests/fixtures/inferred_edge_counts.json").unwrap()
    ).unwrap();
    let by_session = want["by_session_and_rule"].as_object().unwrap();
    for (sid, rules) in by_session {
        let evs = load_real_transcript(sid);
        let view = build_view(sid, &evs);
        for (rule_id, want_count) in rules.as_object().unwrap() {
            let got_count = run_rule_by_id(rule_id, &view).len();
            assert_eq!(got_count as u64, want_count.as_u64().unwrap(),
                       "{sid} rule {rule_id}: got {got_count} want {want_count}");
        }
    }
}
```

Empty golden ⇒ test passes trivially at commit 1. It only arms at commit 4.

### Task 5 — `compute()` integration test

**Files:** `tests/graph_inferred_edges.rs`.

```rust
#[test]
fn inferred_edges_carry_rule_id_and_confidence() {
    let evs = synth_session_with_known_repair_pattern();
    let (_, edges) = witmcc::graph::build::compute("sess_t", &evs, &[], &[]);
    let inferred: Vec<_> = edges.iter()
        .filter(|e| e.inference_rule_id.is_some())
        .collect();
    assert!(!inferred.is_empty());
    for e in inferred {
        assert!(e.confidence.is_some());
        let conf = e.confidence.unwrap();
        assert!(conf >= 0.0 && conf <= 1.0);
    }
}
```

`compute()` does not yet append inferred edges → test fails.

**Commit 1:** `test(slice-13): red-locking tests for inferred edges + rules + schema`

---

## Phase 2 — Migration

| 6 | Write `migrations/20260529120000_0007_graph_edge_inference.sql` per spec §4 | `migration_inference_columns` green |
| 7 | Update `src/model/graph.rs` `GraphEdge` struct: add `inference_rule_id: Option<String>` + `confidence: Option<f32>` | Compile |
| 8 | Update `src/db/repo_graph.rs::insert_edges_in_tx` to write the new columns | Roundtrip test |

**Commit 2:** `feat(db): 0007 inference columns on graph_edge`

---

## Phase 3 — Three rule bodies

| 9  | Implement `caused_repair_v1.rs` per spec §3.1. Token tokeniser is a simple regex `[A-Za-z_][A-Za-z0-9_]*` minus a tiny stop-word set; encoded as a constant array. | All `caused_repair` tests green |
| 10 | Implement `triggered_by_user_message_v1.rs` per spec §3.2 | Tests green |
| 11 | Implement `large_output_to_next_action_v1.rs` per spec §3.3 | Tests green |
| 12 | Populate `RULE_IDS` per spec §4; add `pub fn all_rules() -> Vec<Box<dyn EdgeInferenceRule>>` | Registry test green |

**Commit 3:** `feat(insight): three edge inference rules v1`

---

## Phase 4 — Golden bootstrap

| 13 | Run a small bootstrap binary (`cargo run --bin write_inferred_edge_counts`) that ingests every real transcript and writes `tests/fixtures/inferred_edge_counts.json`. The binary is committed under `examples/bootstrap_inferred_edge_counts.rs` for reproducibility, not under `src/bin`. | `inferred_edge_counts` test green |
| 14 | Re-run the test; commit the populated golden. | n/a |

**Commit 4:** `feat(insight): inferred_edge_counts golden bootstrap`

---

## Phase 5 — `compute()` wiring

| 15 | In `src/graph/build.rs::compute`, after the deterministic-edge loop, build `SessionGraphView` and call each rule in `all_rules()`. Append returned edges. Dedupe per spec §7. | `graph_inferred_edges` test green |
| 16 | Update `rebuild_session` to pass nothing new (the rules read events directly from `view`). | Sanity test: rebuild aac68973, verify edge count > pre-slice-13. |

**Commit 5:** `feat(graph): wire inferred edges into compute() + rebuild_session`

---

## Phase 6 — Smoke + verification

```
Smoke — slice-13

[ ] witmcc init-db; ingest aac68973
[ ] witmcc serve --port 4337 &
[ ] curl -s http://127.0.0.1:4337/v1/sessions/aac68973/graph | \
      jq '.data.edges | map(select(.inference_rule_id != null)) | group_by(.inference_rule_id) | map({rule: .[0].inference_rule_id, n: length})'
    # Expected: three rule entries, counts match golden
[ ] curl -s http://127.0.0.1:4337/v1/sessions/aac68973/graph | \
      jq '.data.edges | map(select(.inference_rule_id == "caused_repair@v1")) | .[0].confidence'
    # Expected: number in [0.0, 1.0]
[ ] cargo test --test edge_inference_registry --test rule_caused_repair --test rule_triggered_by_user_message --test rule_large_output --test inferred_edge_counts --test graph_inferred_edges -- --nocapture
```

```
Verification — slice-13

- cargo test count: baseline (post slice-12) → expected + 12..16
- aac68973 edge count: baseline (post slice-11) + golden's total inferred count
- Rebuild latency: ≤ 1.25 × baseline; record actual
```

---

## Phase 7 — PR

Title: `feat(slice-13): inferred edges v1 (caused_repair, triggered_by_user_message, large_output_to_next_action)`. Implementation notes + CLAUDE.md status update.
