# Telemetry Fold — Slice 1 (Group A: fold into owner payload) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fold deterministically-correlated telemetry (llm_request span + api_request log → assistant turn by `request_id`; tool_result/tool_decision log → tool_call by `tool_use_id`) into the owner graph node's `payload.facets`, remove the standalone telemetry nodes for the folded events, and replace the `facet_of` edge mechanism — so the same data is no longer double-represented as a node AND a facet.

**Architecture:** `graph::build::compute()` is a pure function: `(session_id, events, hunks, runs) -> (nodes, edges)`. It materializes one node per event in a left-to-right loop (build.rs:130-273), then post-passes wire edges including `facet_of` (build.rs:677-726) using two correlation HashMaps already built during materialization: `assistant_nid_by_request_id` (build.rs:127, populated 255-259) and `tool_call_nid_by_tid` (snapshot 280-283). This slice adds a **fold pass** after those maps are complete: for each foldable telemetry node, append a facet entry to the owner node's `payload.facets` array, collect the folded node_ids, then exclude them from the returned `nodes` and stop emitting `facet_of` edges for them. Frontend `buildEntityFacets`/`buildToolMetrics` switch from walking `facet_of` edges to reading `owner.payload.facets`.

**Scope boundary (later slices):** Group B (metric_sample + session_state.permissionMode → session-level facet) and Group C (drop orphan hook/mcp/non-correlated telemetry from nodes) are **Slice 2**. Episode redesign (Tier1 keep / Tier2 delete, missing_verification raw-derivation) is **Slice 3**. This slice ONLY folds the four Group-A telemetry types and leaves all other nodes untouched — minimal diff, independently testable. The raw_event table is never modified (SSOT).

**Tech Stack:** Rust (serde_json, sqlx, chrono), TypeScript/React (Vitest), in-memory SQLite for tests.

---

## File structure

| File | Responsibility | Create/Modify |
|---|---|---|
| `src/graph/build.rs` | add fold pass; remove facet_of edge gen for Group A; exclude folded nodes | Modify |
| `tests/graph_telemetry_fold.rs` | unit: tool-log fold + span/api fold + node-absence + no facet_of | Create |
| `tests/graph_facet_edges.rs` | existing facet_of unit tests → convert to fold assertions | Modify |
| `tests/graph_facet_real.rs` | real-fixture: fold against frozen `facet_correlation_v01.json` | Modify |
| `webui/src/components/replay/facets/entityFacets.ts` | read `payload.facets` instead of `facet_of` edges | Modify |
| `webui/src/components/replay/detail/toolMetrics.ts` | read folded facet `data.attributes` | Modify |
| `webui/src/routes/SessionDetailPage.tsx` | wire facet groups from node payload | Modify |
| `webui/src/components/replay/.../__tests__/*.test.ts` | update facet tests | Modify |
| `docs/implementation-notes.html` | record fold design + facet payload shape + re-ingest note | Modify |

No migration: `graph_node.payload` is already a `TEXT` JSON column (repo_graph.rs:65 serializes `payload.to_string()`); the fold only changes what nests inside the JSON. Existing dev rows carry the pre-fold shape → re-ingest required (operational note in Task 7).

---

## Facet payload contract (locked here, used by every task)

A folded telemetry event becomes one entry appended to the owner node's `payload.facets` array:

```json
{
  "facet_kind": "tool_result_log" | "tool_decision_log" | "llm_request_span" | "api_request_log",
  "basis": "tool_use_id" | "request_id",
  "source_event_id": "<the folded telemetry event_id>",
  "data": { /* the folded event's original payload Value, verbatim (Source-preserving) */ }
}
```

- Owner nodes: `assistant_message` (for `llm_request_span`, `api_request_log`) and `tool_call` (for `tool_result_log`, `tool_decision_log`).
- The owner's `source_event_ids` is left unchanged (it lists the owner's own event); folded provenance lives in each facet entry's `source_event_id`.
- Order within `facets`: stable by the folded events' stream order (we fold in a single pass over `nodes`, which preserve materialization order).

Detection rules (deterministic):
- `tool_result_log`: `node_kind == "log_record"` AND `payload.event_name == "tool_result"` AND `payload.attributes.tool_use_id` present.
- `tool_decision_log`: `node_kind == "log_record"` AND `payload.event_name == "tool_decision"` AND `payload.attributes.tool_use_id` present.
- `api_request_log`: `node_kind == "log_record"` AND `payload.event_name == "api_request"` AND `payload.attributes.request_id` present.
- `llm_request_span`: `node_kind == "otel_span"` AND `payload.raw_span.name == "claude_code.llm_request"` AND `flatten_attrs(payload.raw_span.attributes).request_id` present.

(Real fixture `tests/fixtures/facet/real/facet_correlation_v01.json` uses `event_name` for logs and `raw_span.name` for spans — confirmed by the existing `graph_facet_real.rs`. Note: earlier build.rs facet_of code read `payload.pointer("/attributes/tool_use_id")` without filtering on `event_name`; the fold tightens this to the four named kinds so non-tool logs are NOT folded.)

---

## Task 1: Fold tool logs (`tool_use_id`) into tool_call payload (RED→GREEN)

**Files:**
- Create: `tests/graph_telemetry_fold.rs`
- Modify: `src/graph/build.rs`

- [ ] **Step 1: Write the failing test**

Create `tests/graph_telemetry_fold.rs`:

```rust
//! Slice 1 (Group A) — telemetry fold into owner node payload.
//! tool_result/tool_decision log_record → tool_call by tool_use_id;
//! llm_request span + api_request log → assistant_message by request_id.
//! Folded events MUST NOT remain as standalone nodes and MUST NOT get facet_of edges.

mod common;

use serde_json::{json, Value};
use witmcc::graph::build::compute;
use witmcc::model::observed::{EventKind, ObservedEvent};

fn tool_call_ev(event_id: &str, tuid: &str) -> ObservedEvent {
    let mut e = common::base_event(EventKind::ToolCall, event_id);
    e.tool_use_id = Some(tuid.into());
    e.tool_name = Some("Bash".into());
    e.payload = json!({"tool_name":"Bash","input":{"command":"ls"}});
    e
}

fn tool_result_log_ev(event_id: &str, tuid: &str) -> ObservedEvent {
    let mut e = common::base_event(EventKind::LogRecord, event_id);
    e.payload = json!({
        "event_name":"tool_result",
        "attributes":{"tool_use_id":tuid,"duration_ms":"57","success":"true"}
    });
    e
}

fn tool_decision_log_ev(event_id: &str, tuid: &str) -> ObservedEvent {
    let mut e = common::base_event(EventKind::LogRecord, event_id);
    e.payload = json!({
        "event_name":"tool_decision",
        "attributes":{"tool_use_id":tuid,"decision":"accept","source":"config"}
    });
    e
}

fn facets_of<'a>(nodes: &'a [witmcc::model::graph::GraphNode], kind: &str) -> Vec<&'a Value> {
    let n = nodes.iter().find(|n| n.node_kind == kind).expect("owner node");
    n.payload
        .get("facets")
        .and_then(|f| f.as_array())
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

#[test]
fn tool_logs_fold_into_tool_call_payload() {
    let evs = vec![
        tool_call_ev("evt-call", "toolu_X"),
        tool_result_log_ev("evt-res", "toolu_X"),
        tool_decision_log_ev("evt-dec", "toolu_X"),
    ];
    let (nodes, edges) = compute("sess_t", &evs, &[], &[]);

    // 1. tool_call node carries both logs as facets.
    let facets = facets_of(&nodes, "tool_call");
    assert_eq!(facets.len(), 2, "two tool logs folded; got {facets:?}");
    let kinds: Vec<&str> = facets
        .iter()
        .filter_map(|f| f.get("facet_kind").and_then(|v| v.as_str()))
        .collect();
    assert!(kinds.contains(&"tool_result_log"));
    assert!(kinds.contains(&"tool_decision_log"));
    // facet carries provenance + verbatim data.
    let res = facets.iter().find(|f| f["facet_kind"] == "tool_result_log").unwrap();
    assert_eq!(res["basis"], "tool_use_id");
    assert_eq!(res["source_event_id"], "evt-res");
    assert_eq!(res["data"]["attributes"]["duration_ms"], "57");

    // 2. No standalone log_record nodes remain.
    assert!(
        !nodes.iter().any(|n| n.node_kind == "log_record"),
        "folded tool logs must not remain as nodes"
    );

    // 3. No facet_of edges for the folded logs.
    assert!(
        !edges.iter().any(|e| e.edge_kind == "facet_of"),
        "fold replaces facet_of edges"
    );
}
```

- [ ] **Step 2: Run the test to confirm RED**

Run: `cargo test --test graph_telemetry_fold tool_logs_fold 2>&1 | tail -30`
Expected: FAIL — `tool_call` has no `payload.facets`; standalone `log_record` nodes still exist; a `facet_of` edge is emitted (current behaviour, build.rs:677-697).

- [ ] **Step 3: Add the fold pass + node exclusion in `compute()`**

In `src/graph/build.rs`, AFTER `tool_call_nid_by_tid` is built (the snapshot at ~line 280-283) and `assistant_nid_by_request_id` is complete, but BEFORE the `facet_of` edge passes (line 677), insert a fold pass. Use a helper to detect + build facet entries:

```rust
// --- Group A telemetry fold (Slice 1) ---------------------------------------
// Append each foldable telemetry node as a facet entry on its owner node's
// payload.facets array, then collect the folded node_ids so they are excluded
// from the returned nodes and from facet_of edge generation.
use std::collections::HashSet;
let mut folded_node_ids: HashSet<String> = HashSet::new();
// owner node_id -> Vec<facet entry Value>, applied after the scan to avoid
// mutating `nodes` while iterating it.
let mut facets_by_owner: HashMap<String, Vec<Value>> = HashMap::new();

for n in &nodes {
    // tool_result_log / tool_decision_log → tool_call by tool_use_id
    if n.node_kind == "log_record" {
        let ename = n.payload.get("event_name").and_then(|v| v.as_str());
        let facet_kind = match ename {
            Some("tool_result") => Some("tool_result_log"),
            Some("tool_decision") => Some("tool_decision_log"),
            Some("api_request") => None, // handled in the request_id branch below
            _ => None,
        };
        if let Some(fk) = facet_kind {
            if let Some(tid) = n.payload.pointer("/attributes/tool_use_id").and_then(|v| v.as_str()) {
                if let Some(owner) = tool_call_nid_by_tid.get(tid) {
                    facets_by_owner.entry(owner.clone()).or_default().push(json!({
                        "facet_kind": fk,
                        "basis": "tool_use_id",
                        "source_event_id": n.source_event_ids.first().cloned().unwrap_or_default(),
                        "data": n.payload.clone(),
                    }));
                    folded_node_ids.insert(n.node_id.clone());
                }
            }
        }
    }
}
```

(The `request_id` fold for spans + api_request logs lands in Task 2 — keep this commit tool-only so the RED→GREEN is tight.)

Then, AFTER the loop, apply facets and exclude folded nodes. Add right before the function returns `(nodes, edges)`:

```rust
// Apply collected facets onto owner node payloads.
for n in nodes.iter_mut() {
    if let Some(fs) = facets_by_owner.get(&n.node_id) {
        if let Value::Object(map) = &mut n.payload {
            map.insert("facets".to_string(), Value::Array(fs.clone()));
        }
    }
}
// Drop folded telemetry nodes from the graph.
nodes.retain(|n| !folded_node_ids.contains(&n.node_id));
// Drop any edges that referenced a folded node (incl. the old facet_of edges).
edges.retain(|e| !folded_node_ids.contains(&e.from_node_id) && !folded_node_ids.contains(&e.to_node_id));
```

Also guard the existing tool-log `facet_of` pass (build.rs:677-697) so it skips folded nodes — simplest is to delete that pass entirely (the fold replaces it). Delete lines 677-697 (the `// 3e. facet_of (도구)` block).

Note: `nodes` must be `mut` (it already is — `let mut nodes`). `edges` must be `mut`. If `edges` is currently built and returned without `mut`, change its binding to `let mut edges`.

- [ ] **Step 4: Run the test → GREEN**

Run: `cargo test --test graph_telemetry_fold tool_logs_fold 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add tests/graph_telemetry_fold.rs src/graph/build.rs
git commit -m "feat(graph): fold tool_result/tool_decision logs into tool_call payload.facets

Replace the log→tool_call facet_of edge with an in-payload fold keyed by
tool_use_id. Folded logs no longer appear as standalone nodes. Slice 1/3."
```

---

## Task 2: Fold llm_request span + api_request log (`request_id`) into assistant (RED→GREEN)

**Files:**
- Modify: `tests/graph_telemetry_fold.rs`
- Modify: `src/graph/build.rs`

- [ ] **Step 1: Add the failing test**

Append to `tests/graph_telemetry_fold.rs`:

```rust
fn assistant_ev(event_id: &str, rid: &str) -> ObservedEvent {
    let mut e = common::base_event(EventKind::AssistantMessage, event_id);
    e.request_id = Some(rid.into());
    e.payload = json!({"role":"assistant","content":[]});
    e
}

fn llm_span_ev(event_id: &str, rid: &str) -> ObservedEvent {
    let mut e = common::base_event(EventKind::OtelSpan, event_id);
    e.trace_id = Some("trace-1".into());
    e.span_id = Some("span-1".into());
    e.payload = json!({
        "raw_span":{
            "name":"claude_code.llm_request",
            "attributes":[
                {"key":"request_id","value":{"stringValue":rid}},
                {"key":"duration_ms","value":{"stringValue":"1521"}}
            ]
        }
    });
    e
}

fn api_request_log_ev(event_id: &str, rid: &str) -> ObservedEvent {
    let mut e = common::base_event(EventKind::LogRecord, event_id);
    e.payload = json!({
        "event_name":"api_request",
        "attributes":{"request_id":rid,"cost_usd":0.000906,"duration_ms":"1521","model":"claude-haiku-4-5-20251001"}
    });
    e
}

#[test]
fn span_and_api_log_fold_into_assistant_payload() {
    let evs = vec![
        assistant_ev("evt-asst", "req_A"),
        llm_span_ev("evt-span", "req_A"),
        api_request_log_ev("evt-api", "req_A"),
    ];
    let (nodes, edges) = compute("sess_t", &evs, &[], &[]);

    let facets = facets_of(&nodes, "assistant_message");
    let kinds: Vec<&str> = facets
        .iter()
        .filter_map(|f| f.get("facet_kind").and_then(|v| v.as_str()))
        .collect();
    assert!(kinds.contains(&"llm_request_span"), "span folded; got {kinds:?}");
    assert!(kinds.contains(&"api_request_log"), "api log folded; got {kinds:?}");

    // cost/latency reachable from the assistant node now.
    let api = facets.iter().find(|f| f["facet_kind"] == "api_request_log").unwrap();
    assert_eq!(api["data"]["attributes"]["cost_usd"], 0.000906);

    // folded telemetry gone from nodes; no facet_of edges.
    assert!(!nodes.iter().any(|n| n.node_kind == "otel_span"));
    assert!(!nodes.iter().any(|n| n.node_kind == "log_record"));
    assert!(!edges.iter().any(|e| e.edge_kind == "facet_of"));
}
```

- [ ] **Step 2: Run → RED**

Run: `cargo test --test graph_telemetry_fold span_and_api 2>&1 | tail -30`
Expected: FAIL — assistant node has no `llm_request_span`/`api_request_log` facets; `otel_span` node still present; api_request log still a node.

- [ ] **Step 3: Extend the fold pass for request_id**

In the fold scan loop in `build.rs` (added in Task 1), extend the `log_record` branch to handle `api_request`, and add an `otel_span` branch. Inside the same `for n in &nodes` loop:

```rust
    // api_request_log → assistant_message by request_id (in the log_record branch)
    if n.node_kind == "log_record"
        && n.payload.get("event_name").and_then(|v| v.as_str()) == Some("api_request")
    {
        if let Some(rid) = n.payload.pointer("/attributes/request_id").and_then(|v| v.as_str()) {
            if let Some(owner) = assistant_nid_by_request_id.get(rid) {
                facets_by_owner.entry(owner.clone()).or_default().push(json!({
                    "facet_kind": "api_request_log",
                    "basis": "request_id",
                    "source_event_id": n.source_event_ids.first().cloned().unwrap_or_default(),
                    "data": n.payload.clone(),
                }));
                folded_node_ids.insert(n.node_id.clone());
            }
        }
    }
    // llm_request span → assistant_message by request_id
    if n.node_kind == "otel_span"
        && n.payload.pointer("/raw_span/name").and_then(|v| v.as_str()) == Some("claude_code.llm_request")
    {
        let attrs = flatten_attrs(n.payload.pointer("/raw_span/attributes"));
        if let Some(rid) = attrs.get("request_id").and_then(|v| v.as_str()) {
            if let Some(owner) = assistant_nid_by_request_id.get(rid) {
                facets_by_owner.entry(owner.clone()).or_default().push(json!({
                    "facet_kind": "llm_request_span",
                    "basis": "request_id",
                    "source_event_id": n.source_event_ids.first().cloned().unwrap_or_default(),
                    "data": n.payload.clone(),
                }));
                folded_node_ids.insert(n.node_id.clone());
            }
        }
    }
```

`flatten_attrs` is already imported (build.rs:11). Then delete the second `facet_of` pass (build.rs:699-726, the `// 3e(cont). facet_of (응답)` block) — the fold replaces it.

- [ ] **Step 4: Run → GREEN (and the tool-fold test still passes)**

Run: `cargo test --test graph_telemetry_fold 2>&1 | tail -20`
Expected: both `tool_logs_fold_into_tool_call_payload` and `span_and_api_log_fold_into_assistant_payload` PASS.

- [ ] **Step 5: Commit**

```bash
git add tests/graph_telemetry_fold.rs src/graph/build.rs
git commit -m "feat(graph): fold llm_request span + api_request log into assistant payload.facets

request_id-keyed fold replaces the span→assistant facet_of edge and pulls
cost_usd/duration_ms onto the assistant turn. Folded telemetry leaves the node
stream. Slice 1/3."
```

---

## Task 3: Convert existing facet_of tests to fold assertions (RED→GREEN)

**Files:**
- Modify: `tests/graph_facet_edges.rs`
- Modify: `tests/graph_facet_real.rs`

The old tests assert `facet_of` EDGES exist. After Tasks 1-2 those edges are gone, so these tests now FAIL — they must be rewritten to assert the fold instead (they are the regression net against real frozen payloads).

- [ ] **Step 1: Run the old tests to see them fail**

Run: `cargo test --test graph_facet_edges --test graph_facet_real 2>&1 | tail -30`
Expected: FAIL — `facet_of` edge count assertions no longer hold.

- [ ] **Step 2: Rewrite `graph_facet_edges.rs` assertions**

Replace each `facet_of` edge assertion with a payload-facet assertion. Example for the tool-log test (mirror for the span test):

```rust
#[test]
fn tool_log_folds_into_tool_call_by_tool_use_id() {
    let evs = vec![tool_call_ev("evt-call", "toolu_X"), tool_log_ev("evt-log", "toolu_X")];
    let (nodes, _edges) = compute("sess_t", &evs, &[], &[]);
    let call = nodes.iter().find(|n| n.node_kind == "tool_call").unwrap();
    let facets = call.payload.get("facets").and_then(|f| f.as_array()).expect("facets");
    assert_eq!(facets.len(), 1);
    assert_eq!(facets[0]["facet_kind"], "tool_result_log");
    assert_eq!(facets[0]["basis"], "tool_use_id");
    assert!(!nodes.iter().any(|n| n.node_kind == "log_record"), "log folded out of nodes");
}
```

(Keep the existing event-builder helpers `tool_call_ev`/`tool_log_ev`/`assistant_ev`/`llm_span_ev` at the top of the file — only the assertion bodies change. Note `tool_log_ev` in this file uses `event_name:"tool_result"`, which the fold detection requires.)

- [ ] **Step 3: Rewrite `graph_facet_real.rs`**

The frozen fixture `tests/fixtures/facet/real/facet_correlation_v01.json` has a tool_result log, a tool_decision log, and an llm_request span. Replace the "3 facet_of edges" assertion with: tool_call carries 2 folded tool logs, assistant carries 1 folded span, and no `log_record`/`otel_span` nodes remain for those events.

```rust
#[test]
fn real_payloads_fold_into_owner_nodes() {
    let fx = load_fixture(); // existing helper, reads facet_correlation_v01.json
    let tuid = fx["tool_use_id"].as_str().unwrap();
    let rid = fx["request_id"].as_str().unwrap();
    // ... existing event construction (tool_call, log_result, log_decision, assistant, span) ...
    let evs = vec![tool_call, log_result, log_decision, assistant, span];
    let (nodes, _edges) = compute("sess-real-facet", &evs, &[], &[]);

    let call = nodes.iter().find(|n| n.node_kind == "tool_call").unwrap();
    let call_facets = call.payload.get("facets").and_then(|f| f.as_array()).expect("tool facets");
    assert_eq!(call_facets.len(), 2, "tool_result + tool_decision folded");

    let asst = nodes.iter().find(|n| n.node_kind == "assistant_message").unwrap();
    let asst_facets = asst.payload.get("facets").and_then(|f| f.as_array()).expect("asst facets");
    assert!(asst_facets.iter().any(|f| f["facet_kind"] == "llm_request_span"));

    assert!(!nodes.iter().any(|n| n.node_kind == "otel_span" || n.node_kind == "log_record"),
        "folded telemetry leaves the node stream");
}
```

- [ ] **Step 4: Run → GREEN**

Run: `cargo test --test graph_facet_edges --test graph_facet_real --test graph_telemetry_fold 2>&1 | tail -20`
Expected: all PASS.

- [ ] **Step 5: Full backend suite — no regressions**

Run: `cargo test 2>&1 | tail -30`
Expected: green. If `graph_build.rs` or episode/missing_verification tests reference `facet_of` edges or expect telemetry nodes, fix those assertions to match the fold (telemetry nodes for folded kinds are gone; non-folded telemetry — orphan logs/metrics/spans — still present, untouched by this slice). If `missing_verification` finding count changes, STOP — this slice must not alter findings (it only touches telemetry representation); investigate before editing the extractor.

- [ ] **Step 6: Commit**

```bash
git add tests/graph_facet_edges.rs tests/graph_facet_real.rs
git commit -m "test(graph): lock telemetry fold (Group A) against synthetic + frozen real payloads

Replace facet_of-edge assertions with payload.facets fold assertions. Slice 1/3."
```

---

## Task 4: Frontend reads facets from `payload.facets` (RED→GREEN)

**Files:**
- Modify: `webui/src/components/replay/facets/entityFacets.ts`
- Modify: `webui/src/components/replay/detail/toolMetrics.ts`
- Modify: relevant `__tests__/*.test.ts`

`buildEntityFacets` currently walks `facet_of` edges (entityFacets.ts:10-27); those edges no longer exist. It must read `node.payload.facets` instead. `buildToolMetrics` reads `node.payload.attributes` on a facet node (toolMetrics.ts:34-52); now the data is at `owner.payload.facets[].data.attributes`.

- [ ] **Step 1: Write the failing test**

Add to `webui/src/components/replay/facets/__tests__/entityFacets.test.ts` (create if absent, mirror existing test style):

```ts
import { describe, it, expect } from 'vitest';
import { buildEntityFacets } from '../entityFacets';
import type { GraphNodeDto } from '../../../../api/types';

const node = (id: string, kind: string, payload: unknown): GraphNodeDto => ({
  node_id: id, schema_version: 'graph_node.v1', session_id: 's', node_kind: kind,
  started_at: '2026-05-31T00:00:00Z', ended_at: null, merge_keys: {},
  source_event_ids: [id], source_uris: [], payload,
});

describe('buildEntityFacets (payload.facets)', () => {
  it('groups folded facets from owner node payload', () => {
    const nodes: GraphNodeDto[] = [
      node('call-1', 'tool_call', { facets: [
        { facet_kind: 'tool_result_log', basis: 'tool_use_id', source_event_id: 'e1', data: {} },
        { facet_kind: 'tool_decision_log', basis: 'tool_use_id', source_event_id: 'e2', data: {} },
      ] }),
      node('asst-1', 'assistant_message', { facets: [
        { facet_kind: 'llm_request_span', basis: 'request_id', source_event_id: 'e3', data: {} },
      ] }),
    ];
    const groups = buildEntityFacets(nodes);
    expect(groups.get('call-1')?.facets.length).toBe(2);
    expect(groups.get('asst-1')?.facets.length).toBe(1);
  });
});
```

- [ ] **Step 2: Run → RED**

Run: `cd webui && pnpm vitest run src/components/replay/facets 2>&1 | tail -20`
Expected: FAIL — `buildEntityFacets` signature/behaviour still edge-based.

- [ ] **Step 3: Rewrite `buildEntityFacets`**

Replace `entityFacets.ts` body so it reads facets from node payloads (drop the `edges` parameter):

```ts
import type { GraphNodeDto } from '../../../api/types';

export type FacetEntry = {
  facet_kind: string;
  basis: string;
  source_event_id: string;
  data: Record<string, unknown>;
};
export type FacetGroup = {
  entityNodeId: string;
  facets: FacetEntry[];
  byKind: Record<string, number>;
};

export function buildEntityFacets(nodes: GraphNodeDto[]): Map<string, FacetGroup> {
  const out = new Map<string, FacetGroup>();
  for (const n of nodes) {
    const p = (n.payload ?? {}) as Record<string, unknown>;
    const facets = Array.isArray(p.facets) ? (p.facets as FacetEntry[]) : [];
    if (facets.length === 0) continue;
    const byKind: Record<string, number> = {};
    for (const f of facets) byKind[f.facet_kind] = (byKind[f.facet_kind] ?? 0) + 1;
    out.set(n.node_id, { entityNodeId: n.node_id, facets, byKind });
  }
  return out;
}
```

- [ ] **Step 4: Rewrite `buildToolMetrics` to read folded facet data**

Replace its input from facet nodes to facet entries. New signature reads `FacetEntry[]`:

```ts
import type { FacetEntry } from '../facets/entityFacets';

export function buildToolMetrics(facets: FacetEntry[]): ToolMetrics {
  const m: ToolMetrics = {
    durationMs: null, success: null, decisionSource: null, decisionType: null,
    inputBytes: null, resultBytes: null, sequence: null,
  };
  for (const f of facets) {
    if (f.facet_kind !== 'tool_result_log' && f.facet_kind !== 'tool_decision_log') continue;
    const a = ((f.data ?? {}) as Record<string, unknown>).attributes as Record<string, unknown> ?? {};
    if (m.durationMs == null) m.durationMs = num(a.duration_ms);
    if (m.success == null && typeof a.success === 'string') m.success = a.success === 'true';
    if (m.inputBytes == null) m.inputBytes = num(a.tool_input_size_bytes);
    if (m.resultBytes == null) m.resultBytes = num(a.tool_result_size_bytes);
    if (m.decisionSource == null) m.decisionSource = str(a.decision_source);
    if (m.decisionType == null) m.decisionType = str(a.decision_type);
    if (m.sequence == null) m.sequence = num(a['event.sequence']);
  }
  return m;
}
```

(Keep the `num`/`str` helpers already in the file.)

- [ ] **Step 5: Run frontend facet tests → GREEN**

Run: `cd webui && pnpm vitest run src/components/replay 2>&1 | tail -30`
Expected: PASS. Update any `toolMetrics.test.ts` cases to pass `FacetEntry[]` instead of facet nodes.

- [ ] **Step 6: Commit**

```bash
git add webui/src/components/replay
git commit -m "feat(webui): read folded facets from node payload instead of facet_of edges

buildEntityFacets reads payload.facets; buildToolMetrics reads facet.data.attributes.
Mirrors the backend Group-A fold. Slice 1/3."
```

---

## Task 5: Wire SessionDetailPage + browser smoke

**Files:**
- Modify: `webui/src/routes/SessionDetailPage.tsx`

- [ ] **Step 1: Update call sites**

`SessionDetailPage.tsx:108` calls `buildEntityFacets(effectiveGraph.nodes, effectiveGraph.edges)` — drop the second arg:

```ts
const entityFacets = useMemo(
  () => buildEntityFacets(effectiveGraph.nodes),
  [effectiveGraph],
);
```

`SessionDetailPage.tsx:246-254` (`selectedToolMetrics`) currently maps `group.facetNodeIds` → nodes → `buildToolMetrics(facetNodes)`. Replace with the facet entries directly:

```ts
const selectedToolMetrics = useMemo(() => {
  if (selectedNode?.node_kind !== 'tool_call') return null;
  const group = entityFacets.get(selectedNode.node_id);
  if (!group) return null;
  return buildToolMetrics(group.facets);
}, [selectedNode, entityFacets]);
```

For the assistant LLM metrics (`selectedNodeLlmMetrics`, lines 261-268): the request_id→span metrics now live on the assistant node's `payload.facets` (facet_kind `llm_request_span`/`api_request_log`). Read them from the group instead of `metricsByReq`:

```ts
const selectedNodeLlmMetrics = useMemo(() => {
  if (selectedNode?.node_kind !== 'assistant_message') return null;
  const group = entityFacets.get(selectedNode.node_id);
  const api = group?.facets.find((f) => f.facet_kind === 'api_request_log');
  const span = group?.facets.find((f) => f.facet_kind === 'llm_request_span');
  if (!api && !span) return null;
  const a = ((api?.data ?? {}) as Record<string, unknown>).attributes as Record<string, unknown> ?? {};
  return {
    model: typeof a.model === 'string' ? a.model : null,
    costUsd: typeof a.cost_usd === 'number' ? a.cost_usd : null,
    durationMs: a.duration_ms != null ? Number(a.duration_ms) : null,
  };
}, [selectedNode, entityFacets]);
```

(If `metricsByReq` / `buildToolMetrics`'s old node-array call sites are now unused, remove the dead code. If `selectedNodeLlmMetrics` is consumed by a child component expecting a specific shape, match that shape — grep its consumer first.)

- [ ] **Step 2: Typecheck + unit**

Run: `cd webui && pnpm tsc --noEmit && pnpm vitest run 2>&1 | tail -20`
Expected: no type errors; tests green.

- [ ] **Step 3: Rebuild embedded dist + re-ingest a session, then browser smoke**

```bash
# regenerate graph rows with folded payloads for a real session
cargo run --bin witmcc -- init-db && cargo run --bin witmcc -- ingest --all 2>&1 | tail -5
# build webui dist embedded by serve, then serve
cd webui && pnpm build && cd ..
cargo run --bin witmcc -- serve --bind 127.0.0.1 --port 7878 &
sleep 2
```

Then use the claude-in-chrome tools to open `http://127.0.0.1:7878/sessions/<a real session id>`, select a `tool_call` node and an `assistant_message` node, and visually confirm: (a) tool metrics (duration/decision) still render, (b) assistant cost/model/latency render, (c) the graph/timeline no longer shows standalone log_record/otel_span nodes for the folded kinds. Capture before/after. Stop serve with `kill %1` after.

Per CLAUDE.md, WebUI changes require this browser smoke before commit. Also note (memory `witmcc-serve-overwrites-graph-on-otlp`): if a serve was already running during ingest, restart it so it serves the freshly-folded graph, not a stale in-memory rebuild.

- [ ] **Step 4: Commit**

```bash
git add webui/src/routes/SessionDetailPage.tsx
git commit -m "feat(webui): SessionDetailPage consumes folded facets from node payload

tool + LLM metrics read from entity facet groups; folded telemetry no longer
rendered as standalone nodes. Browser-smoked. Slice 1/3."
```

---

## Task 6: Documentation

**Files:**
- Modify: `docs/implementation-notes.html`

- [ ] **Step 1: Add an implementation-notes section**

Add a new `§` entry (follow the existing self-contained HTML markup — no external JS/CSS). Record:
- **Fold design**: Group A telemetry (llm_request span, api_request log → assistant by request_id; tool_result/tool_decision log → tool_call by tool_use_id) is folded into the owner node's `payload.facets` array; the standalone telemetry nodes and the `facet_of` edges are removed. The fold reuses the existing correlation maps (`assistant_nid_by_request_id`, `tool_call_nid_by_tid`) — no new correlation cost.
- **Facet payload shape**: quote the `{facet_kind, basis, source_event_id, data}` contract. `data` is the folded event's verbatim payload (Source-preserving).
- **Detection rules**: the four named kinds and their keys; note the tightening vs the old facet_of code (which folded any `log_record` with a `tool_use_id` regardless of `event_name`).
- **Scope**: this is Slice 1/3. Group B (metric/permissionMode → session facet) and Group C (drop orphan telemetry) are Slice 2; episode redesign is Slice 3. Orphan telemetry (non-correlated logs/metrics/spans, hook_event) is UNCHANGED by this slice and still appears as nodes.
- **Operational note**: no migration (payload is already a JSON column); existing dev DB rows carry the pre-fold shape — run `witmcc init-db` + `witmcc ingest --all` to regenerate folded graph rows. Restart any running `serve` afterward.
- Reference the design spec `docs/superpowers/specs/2026-05-31-telemetry-facet-fold-and-episode-redesign-design.md`.

- [ ] **Step 2: Commit**

```bash
git add docs/implementation-notes.html
git commit -m "docs(graph): implementation notes for Group-A telemetry fold (Slice 1/3)"
```

---

## Task 7: Open PR

- [ ] **Step 1: Final full-suite gate**

Run: `cargo test 2>&1 | tail -20 && cd webui && pnpm vitest run 2>&1 | tail -10 && pnpm tsc --noEmit && cd ..`
Expected: all green, no type errors.

- [ ] **Step 2: Push + PR**

```bash
git push -u origin facet-fold-episode-redesign
gh pr create --title "Telemetry fold Slice 1/3: Group A → owner payload.facets" --body "$(cat <<'EOF'
## What
Fold deterministically-correlated telemetry into the owner graph node's `payload.facets` and remove the standalone telemetry nodes + `facet_of` edges for those events.

- llm_request span + api_request log → `assistant_message` (by `request_id`): pulls cost_usd / duration_ms / model onto the turn.
- tool_result / tool_decision log → `tool_call` (by `tool_use_id`): permission + result metrics on the tool.

## Why
Telemetry was promoted 1:1 to standalone graph nodes (65% of all nodes) and only *linked* by display-only `facet_of` edges (never folded), so the same logical event appeared twice and the episode/timeline stream was dominated by telemetry. This is the root fix; see `docs/superpowers/specs/2026-05-31-telemetry-facet-fold-and-episode-redesign-design.md`.

## Scope
Slice 1/3. Group B (metric/permissionMode → session facet) + Group C (drop orphan telemetry) = Slice 2. Episode Tier1-keep/Tier2-delete + missing_verification raw-derivation = Slice 3. raw_event (SSOT) untouched.

## Tests
- `tests/graph_telemetry_fold.rs` (synthetic), `tests/graph_facet_real.rs` (frozen real payloads), frontend facet tests. Browser-smoked.

## Operational
No migration. Run `witmcc init-db` + `ingest --all` to regenerate folded graph; restart `serve`.
EOF
)"
```

- [ ] **Step 3: Report PR URL to the user.**

---

## Self-review notes (coverage vs design spec §4 Group A, §5, §7)

- §4 Group A four kinds → Tasks 1-2 fold all four (tool_result_log, tool_decision_log, api_request_log, llm_request_span). ✓
- §5 "graph consumes folded backbone; telemetry not standalone nodes" → Tasks 1-2 remove folded nodes; **partial** — orphan/Group-C telemetry still nodes until Slice 2 (explicitly scoped out). ✓ (documented)
- §7 "no new correlation cost; fewer nodes/edges" → fold reuses existing maps; node/edge `retain` reduces output. ✓
- §3 principle 2 (raw=SSOT) → no raw_event change, no migration. ✓
- §3 principle 1 (don't force-merge) → only the four deterministic-key kinds fold; everything else untouched. ✓
- Attachment_meta/session_state → already `_ => continue` (not nodes); no action needed this slice. ✓
- TDD red-first → every task: failing test → run RED → implement → run GREEN → commit. ✓
