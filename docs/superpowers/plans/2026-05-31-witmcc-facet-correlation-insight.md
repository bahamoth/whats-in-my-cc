# Facet 연관 + 지표 중심 Insight 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 상호보완 facet(transcript·log·span)을 신뢰 단일 키로 연관해, 메시지 뷰는 박자만 남기고 Insight 뷰는 수집 지표를 의미와 함께 보여주며 Raw 뷰는 source별 분할 JSON을 보여준다.

**Architecture:** 백엔드 graph-builder가 `facet_of` 엣지를 신뢰 키(`tool_use_id`, `request_id`)로 생성(Layer 1). 프론트는 그 엣지를 소비하는 순수 view-model 위에 세 뷰를 얹는다(Layer 2). metric_sample·tool 계열 span은 박자가 아니므로 행에서 제외(분류, 합치기 아님).

**Tech Stack:** Rust(graph builder, sqlx, serde_json) · React/TypeScript(Vite, vitest, TanStack Query) · SQLite.

**설계 출처:** `docs/superpowers/specs/2026-05-31-witmcc-facet-correlation-insight-design.md`

**검증된 데이터 사실 (Real-data anchoring):**
- `tool_call.tool_use_id` 컬럼 채워짐. `log_record.tool_use_id` **컬럼 미채움** → `payload.attributes.tool_use_id`(flat map)에서 추출.
- `assistant_message`/`thinking.request_id` 컬럼 채워짐(191/191). `otel_span` `request_id` **컬럼 미채움** → `payload.raw_span.attributes[]`(OTLP 배열 `{key,value:{stringValue}}`)에서 추출.
- `log_record` payload 예: `{event_name, attributes:{tool_use_id, duration_ms, tool_input_size_bytes, tool_result_size_bytes, decision_source, decision_type, "event.sequence", success}}`.
- trace_id는 세션당 1개 → 도구 span 연관은 span-tree 필요(이번 비범위, §Task 8 후속 후보).

---

## File Structure

**Backend (Rust)**
- Modify `src/graph/build.rs` — `compute()`에 facet 연관 패스 추가(섹션 "3e. facet_of").
- Create `tests/graph_facet_edges.rs` — facet 엣지 단위 테스트.
- Modify `tests/common/mod.rs`(또는 해당 helper) — 필요한 synth 헬퍼.

**Frontend (TS)**
- Create `webui/src/components/replay/facets/entityFacets.ts` — `buildEntityFacets(nodes, edges)`.
- Create `webui/src/components/replay/facets/__tests__/entityFacets.test.ts`.
- Create `webui/src/components/replay/detail/toolMetrics.ts` — log facet → 도구 지표.
- Create `webui/src/components/replay/detail/toolMetrics.test.ts`.
- Modify `webui/src/components/replay/detail/ResponseMetricsPanel.tsx` → 일반화하여 `EntityMetricsPanel`(또는 kind 분기 추가).
- Modify `webui/src/components/replay/detail/InsightTab.tsx` — FocusedInsightGraph 제거, 지표 패널 + findings.
- Modify `webui/src/components/replay/stream/streamModel.ts` — classify 정교화(facet/telemetry drop, 상태변화 log 유지).
- Modify `webui/src/components/replay/detail/RawTab.tsx` + `DetailPanel.tsx` + `SessionDetailPage.tsx` — facet source별 분할 raw.

---

## Task 1: 백엔드 — `facet_of` 엣지 (도구 ↔ log, `tool_use_id`)

**Files:**
- Modify: `src/graph/build.rs` (compute(), 기존 "3d" 엣지 패스 뒤 ~line 585 이후)
- Create: `tests/graph_facet_edges.rs`

- [ ] **Step 1: 실패하는 테스트 작성** — `tests/graph_facet_edges.rs`

```rust
mod common;
use witmcc::graph::build::compute;
use witmcc::model::event::{EventKind, ObservedEvent};
use serde_json::json;

// 최소 синт: tool_call(컬럼 tool_use_id) + log_record(payload.attributes.tool_use_id)
fn tool_call_ev(tuid: &str) -> ObservedEvent {
    let mut e = common::base_event(EventKind::ToolCall, "evt-call");
    e.tool_use_id = Some(tuid.into());
    e.tool_name = Some("Bash".into());
    e.payload = json!({"tool_name":"Bash","input":{"command":"ls"}});
    e
}
fn tool_log_ev(tuid: &str) -> ObservedEvent {
    let mut e = common::base_event(EventKind::LogRecord, "evt-log");
    e.payload = json!({
        "event_name":"tool_result",
        "attributes":{"tool_use_id":tuid,"duration_ms":"57","success":"true"}
    });
    e
}

#[test]
fn facet_of_links_log_record_to_tool_call_by_tool_use_id() {
    let evs = vec![tool_call_ev("toolu_X"), tool_log_ev("toolu_X")];
    let (nodes, edges) = compute("sess_t", &evs, &[], &[]);
    let call = nodes.iter().find(|n| n.node_kind == "tool_call").unwrap();
    let log = nodes.iter().find(|n| n.node_kind == "log_record").unwrap();
    let f: Vec<_> = edges.iter().filter(|e| e.edge_kind == "facet_of").collect();
    assert_eq!(f.len(), 1, "정확히 하나의 facet_of");
    assert_eq!(f[0].from_node_id, log.node_id, "from=facet(log)");
    assert_eq!(f[0].to_node_id, call.node_id, "to=엔티티(tool_call)");
    assert_eq!(f[0].attributes.get("basis").and_then(|v| v.as_str()), Some("tool_use_id"));
}

#[test]
fn facet_of_not_emitted_when_no_matching_tool_call() {
    let evs = vec![tool_log_ev("toolu_orphan")];
    let (_, edges) = compute("sess_t", &evs, &[], &[]);
    assert!(edges.iter().all(|e| e.edge_kind != "facet_of"));
}
```

- [ ] **Step 2: helper 확인/추가** — `tests/common/mod.rs`에 `base_event(kind, event_id)`가 없으면 추가(기존 테스트의 synth 패턴 재사용; `ObservedEvent` 필수 필드 기본값 채움). 기존 `tests/common/` 모듈을 먼저 읽고 동일 컨벤션 사용.

- [ ] **Step 3: 테스트 실패 확인**

Run: `cargo test --test graph_facet_edges facet_of_links 2>&1 | tail -20`
Expected: FAIL (facet_of 엣지 없음 → `f.len()==0`).

- [ ] **Step 4: 구현** — `src/graph/build.rs` `compute()`에 facet 패스 추가 (3d 뒤, turn_order 전).

```rust
// 3e. facet_of (도구) — log_record facet → tool_call 엔티티. 신뢰 키 tool_use_id.
//     log_record는 tool_use_id 컬럼이 비어 payload.attributes.tool_use_id에서 추출.
for n in &nodes {
    if n.node_kind != "log_record" { continue; }
    let Some(tid) = n.payload
        .pointer("/attributes/tool_use_id")
        .and_then(|v| v.as_str()) else { continue; };
    let Some(call_nid) = tool_call_nid_by_tid.get(tid) else { continue; };
    if !valid_nodes.contains(call_nid.as_str()) || !valid_nodes.contains(n.node_id.as_str()) {
        continue;
    }
    edges.push(make_edge(
        session_id, &n.node_id, call_nid, "facet_of",
        json!({"basis": "tool_use_id"}),
    ));
}
```

- [ ] **Step 5: 테스트 통과 확인**

Run: `cargo test --test graph_facet_edges 2>&1 | tail -20`
Expected: PASS (2 tests).

- [ ] **Step 6: turn_order 제외 확인** — `facet_of`가 turn_order/dedup 패스에 끼지 않는지 확인. turn_order는 특정 kind만 잇고, dedup(`canonical_pairs`/seen)은 (from,to,kind) 키라 영향 없음. 필요시 turn_order 생성에서 `facet_of`는 무관하므로 그대로.

- [ ] **Step 7: 커밋**

```bash
git add src/graph/build.rs tests/graph_facet_edges.rs tests/common/mod.rs
git commit -m "feat(graph): facet_of edge — log_record→tool_call by tool_use_id"
```

---

## Task 2: 백엔드 — `facet_of` 엣지 (응답 ↔ llm_request span, `request_id`)

**Files:**
- Modify: `src/graph/build.rs` (Task 1 패스에 이어서; 노드 루프에 request_id 인덱스 추가)
- Modify: `tests/graph_facet_edges.rs`

- [ ] **Step 1: 실패 테스트 추가** — `tests/graph_facet_edges.rs`

```rust
fn assistant_ev(rid: &str) -> ObservedEvent {
    let mut e = common::base_event(EventKind::AssistantMessage, "evt-asst");
    e.request_id = Some(rid.into());
    e.payload = json!({"text":"hi","model":"claude-opus-4-8"});
    e
}
fn llm_span_ev(rid: &str) -> ObservedEvent {
    let mut e = common::base_event(EventKind::OtelSpan, "evt-span");
    e.trace_id = Some("trace-1".into());
    e.span_id = Some("span-1".into());
    e.payload = json!({
        "raw_span":{
            "name":"claude_code.llm_request",
            "attributes":[
                {"key":"request_id","value":{"stringValue":rid}},
                {"key":"duration_ms","value":{"stringValue":"28900"}}
            ]
        }
    });
    e
}

#[test]
fn facet_of_links_llm_span_to_assistant_by_request_id() {
    let evs = vec![assistant_ev("req_A"), llm_span_ev("req_A")];
    let (nodes, edges) = compute("sess_t", &evs, &[], &[]);
    let asst = nodes.iter().find(|n| n.node_kind == "assistant_message").unwrap();
    let span = nodes.iter().find(|n| n.node_kind == "otel_span").unwrap();
    let f = edges.iter().find(|e| e.edge_kind == "facet_of"
        && e.from_node_id == span.node_id).expect("span→asst facet_of");
    assert_eq!(f.to_node_id, asst.node_id);
    assert_eq!(f.attributes.get("basis").and_then(|v| v.as_str()), Some("request_id"));
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test --test graph_facet_edges facet_of_links_llm 2>&1 | tail -20`
Expected: FAIL.

- [ ] **Step 3: 구현** — 노드 루프(1. Node materialization)에서 assistant request_id 인덱스 수집. `tool_call_node` 옆에 추가:

```rust
// node 루프 시작부 근처 선언
let mut assistant_nid_by_request_id: HashMap<String, String> = HashMap::new();
```
노드 push 뒤(`kind == "assistant_message"`일 때, request_id가 있으면):
```rust
if kind == "assistant_message" {
    if let Some(rid) = &e.request_id {
        assistant_nid_by_request_id.insert(rid.clone(), nodes[node_index_by_id[&node_id_for_e]].node_id.clone());
    }
}
```
> 구현 주의: 위 인덱싱은 node_id를 이미 계산한 스코프에서 한다. 기존 `tool_call_node.insert(tid, nodes.len())` 패턴과 동일 위치(노드 push 직전/직후)에 `assistant_nid_by_request_id.insert(rid, node_id.clone())` 추가가 가장 단순. (executing agent: 실제 변수명 `node_id`가 push 전 스코프에 있으므로 그걸 clone.)

3e 패스에 span 분기 추가:
```rust
// 3e(cont). facet_of (응답) — llm_request span → assistant_message. 신뢰 키 request_id.
for n in &nodes {
    if n.node_kind != "otel_span" { continue; }
    if n.payload.pointer("/raw_span/name").and_then(|v| v.as_str()) != Some("claude_code.llm_request") {
        continue;
    }
    let rid = n.payload.pointer("/raw_span/attributes")
        .and_then(|a| a.as_array())
        .and_then(|arr| arr.iter().find(|kv|
            kv.get("key").and_then(|k| k.as_str()) == Some("request_id")))
        .and_then(|kv| kv.pointer("/value/stringValue").and_then(|v| v.as_str()));
    let Some(rid) = rid else { continue; };
    let Some(asst_nid) = assistant_nid_by_request_id.get(rid) else { continue; };
    if !valid_nodes.contains(asst_nid.as_str()) || !valid_nodes.contains(n.node_id.as_str()) {
        continue;
    }
    edges.push(make_edge(
        session_id, &n.node_id, asst_nid, "facet_of",
        json!({"basis": "request_id"}),
    ));
}
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test --test graph_facet_edges 2>&1 | tail -20`
Expected: PASS (3 tests). 그리고 `cargo test 2>&1 | tail -15`로 기존 graph 테스트 회귀 없음 확인.

- [ ] **Step 5: 커밋**

```bash
git add src/graph/build.rs tests/graph_facet_edges.rs
git commit -m "feat(graph): facet_of edge — llm_request span→assistant by request_id"
```

---

## Task 3: 백엔드 — 실 fixture로 facet 연관 잠금 (Real-data anchoring)

**Files:**
- Modify: `tests/graph_facet_edges.rs` (실 payload 기반 invariant)
- 사용: `tests/fixtures/otel/real`, `tests/fixtures/transcripts/real`

- [ ] **Step 1: fixture 확인** — `tests/fixtures/otel/real`·`transcripts/real`의 실제 payload를 읽고, tool_use_id를 공유하는 transcript tool_call + OTLP log_record 쌍이 있는지 확인. 기존 fixture 로더(`tests/common/`의 OTLP/transcript 파서)를 재사용.

- [ ] **Step 2: 실패 테스트** — 실 fixture를 ingest→compute한 그래프에서 `facet_of` 엣지가 최소 1개 이상이고, 모든 facet_of의 `to`가 tool_call|assistant_message kind인지 단언.

```rust
#[test]
fn real_fixture_produces_facet_of_with_valid_targets() {
    let evs = common::load_real_fixture_events(); // 기존 로더 컨벤션에 맞춰 구현/재사용
    let (nodes, edges) = compute("sess_real", &evs, &[], &[]);
    let by_id: std::collections::HashMap<_,_> =
        nodes.iter().map(|n| (n.node_id.as_str(), n.node_kind.as_str())).collect();
    let facets: Vec<_> = edges.iter().filter(|e| e.edge_kind == "facet_of").collect();
    assert!(!facets.is_empty(), "실 fixture에서 facet_of가 나와야 함");
    for f in facets {
        let to_kind = by_id.get(f.to_node_id.as_str()).copied().unwrap_or("");
        assert!(matches!(to_kind, "tool_call" | "assistant_message"),
            "facet_of의 to는 엔티티 노드여야: {to_kind}");
    }
}
```

- [ ] **Step 3~4: 로더 구현/재사용 → 통과 확인**

Run: `cargo test --test graph_facet_edges real_fixture 2>&1 | tail -20`
Expected: PASS. (fixture에 적절한 쌍이 없으면, otel/real에서 최소 쌍을 동결 추가.)

- [ ] **Step 5: 커밋**

```bash
git add tests/graph_facet_edges.rs tests/fixtures/
git commit -m "test(graph): lock facet_of correlation against real fixtures"
```

---

## Task 4: 프론트 — `buildEntityFacets` 순수 함수

**Files:**
- Create: `webui/src/components/replay/facets/entityFacets.ts`
- Create: `webui/src/components/replay/facets/__tests__/entityFacets.test.ts`

- [ ] **Step 1: 실패 테스트** — `entityFacets.test.ts`

```ts
import { describe, expect, it } from 'vitest';
import { buildEntityFacets } from '../entityFacets';
import type { GraphNodeDto, GraphEdgeDto } from '../../../../api/types';

const node = (id: string, kind: string): GraphNodeDto => ({
  node_id: id, schema_version: '1', session_id: 's', node_kind: kind,
  started_at: '', ended_at: null, merge_keys: {}, source_event_ids: [id + '-ev'],
  source_uris: [], payload: {},
});
const facetEdge = (from: string, to: string, basis: string): GraphEdgeDto => ({
  edge_id: `${from}->${to}`, schema_version: '1', session_id: 's',
  from_node_id: from, to_node_id: to, edge_kind: 'facet_of',
  origin: 'deterministic', attributes: { basis },
});

describe('buildEntityFacets', () => {
  it('maps an entity to its facet node ids via facet_of edges', () => {
    const nodes = [node('call', 'tool_call'), node('log', 'log_record')];
    const edges = [facetEdge('log', 'call', 'tool_use_id')];
    const m = buildEntityFacets(nodes, edges);
    expect(m.get('call')?.facetNodeIds).toEqual(['log']);
    expect(m.get('call')?.byKind['log_record']).toBe(1);
  });
  it('ignores non-facet_of edges', () => {
    const nodes = [node('a', 'tool_call'), node('b', 'tool_result')];
    const edges: GraphEdgeDto[] = [{ edge_id: 'x', schema_version: '1', session_id: 's',
      from_node_id: 'a', to_node_id: 'b', edge_kind: 'tool_call_to_result', origin: 'deterministic', attributes: {} }];
    expect(buildEntityFacets(nodes, edges).size).toBe(0);
  });
});
```

- [ ] **Step 2: 실패 확인** — `cd webui && npx vitest run src/components/replay/facets 2>&1 | tail -15` → FAIL(모듈 없음).

- [ ] **Step 3: 구현** — `entityFacets.ts`

```ts
import type { GraphNodeDto, GraphEdgeDto } from '../../../api/types';

export interface FacetGroup {
  entityNodeId: string;
  facetNodeIds: string[];
  byKind: Record<string, number>;
}

/** facet_of 엣지(from=facet, to=엔티티)를 따라 엔티티별 facet 묶음을 만든다. */
export function buildEntityFacets(
  nodes: GraphNodeDto[],
  edges: GraphEdgeDto[],
): Map<string, FacetGroup> {
  const kindById = new Map(nodes.map((n) => [n.node_id, n.node_kind]));
  const out = new Map<string, FacetGroup>();
  for (const e of edges) {
    if (e.edge_kind !== 'facet_of') continue;
    const entity = e.to_node_id;
    const facet = e.from_node_id;
    let g = out.get(entity);
    if (!g) { g = { entityNodeId: entity, facetNodeIds: [], byKind: {} }; out.set(entity, g); }
    g.facetNodeIds.push(facet);
    const k = kindById.get(facet) ?? 'unknown';
    g.byKind[k] = (g.byKind[k] ?? 0) + 1;
  }
  return out;
}
```

- [ ] **Step 4: 통과 확인** — `npx vitest run src/components/replay/facets 2>&1 | tail -15` → PASS.

- [ ] **Step 5: 커밋**

```bash
git add webui/src/components/replay/facets/
git commit -m "feat(webui): buildEntityFacets — consume facet_of edges into entity groups"
```

---

## Task 5: 프론트 — `toolMetrics` 추출기 (log facet → 도구 지표)

**Files:**
- Create: `webui/src/components/replay/detail/toolMetrics.ts`
- Create: `webui/src/components/replay/detail/toolMetrics.test.ts`

- [ ] **Step 1: 실패 테스트** — `toolMetrics.test.ts`

```ts
import { describe, expect, it } from 'vitest';
import { buildToolMetrics } from './toolMetrics';
import type { GraphNodeDto } from '../../../api/types';

const logNode = (eventName: string, attrs: Record<string, unknown>): GraphNodeDto => ({
  node_id: 'log-' + eventName, schema_version: '1', session_id: 's', node_kind: 'log_record',
  started_at: '', ended_at: null, merge_keys: {}, source_event_ids: [], source_uris: [],
  payload: { event_name: eventName, attributes: attrs },
});

describe('buildToolMetrics', () => {
  it('merges tool_result + tool_decision log attributes', () => {
    const facets = [
      logNode('tool_result', { duration_ms: '57', success: 'true', tool_input_size_bytes: '362', tool_result_size_bytes: '302', 'event.sequence': 763 }),
      logNode('tool_decision', { decision_source: 'config', decision_type: 'accept' }),
    ];
    const m = buildToolMetrics(facets);
    expect(m.durationMs).toBe(57);
    expect(m.success).toBe(true);
    expect(m.inputBytes).toBe(362);
    expect(m.resultBytes).toBe(302);
    expect(m.decisionSource).toBe('config');
    expect(m.decisionType).toBe('accept');
    expect(m.sequence).toBe(763);
  });
  it('returns nulls when facets absent', () => {
    const m = buildToolMetrics([]);
    expect(m.durationMs).toBeNull();
  });
});
```

- [ ] **Step 2: 실패 확인** — `npx vitest run src/components/replay/detail/toolMetrics 2>&1 | tail -15` → FAIL.

- [ ] **Step 3: 구현** — `toolMetrics.ts` (값은 문자열/숫자 혼재 → 안전 파싱).

```ts
import type { GraphNodeDto } from '../../../api/types';

export interface ToolMetrics {
  durationMs: number | null;
  success: boolean | null;
  decisionSource: string | null;
  decisionType: string | null;
  inputBytes: number | null;
  resultBytes: number | null;
  sequence: number | null;
}

function num(v: unknown): number | null {
  if (typeof v === 'number') return v;
  if (typeof v === 'string' && v.trim() !== '' && !Number.isNaN(Number(v))) return Number(v);
  return null;
}
function str(v: unknown): string | null { return typeof v === 'string' ? v : null; }

/** log_record facet 노드들(payload.attributes)에서 도구 실행 지표를 합친다. */
export function buildToolMetrics(facetNodes: GraphNodeDto[]): ToolMetrics {
  const m: ToolMetrics = {
    durationMs: null, success: null, decisionSource: null, decisionType: null,
    inputBytes: null, resultBytes: null, sequence: null,
  };
  for (const n of facetNodes) {
    if (n.node_kind !== 'log_record') continue;
    const p = (n.payload ?? {}) as Record<string, unknown>;
    const a = (p.attributes ?? {}) as Record<string, unknown>;
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

- [ ] **Step 4: 통과 확인** → PASS.

- [ ] **Step 5: 커밋**

```bash
git add webui/src/components/replay/detail/toolMetrics.ts webui/src/components/replay/detail/toolMetrics.test.ts
git commit -m "feat(webui): buildToolMetrics — tool exec metrics from log facets"
```

---

## Task 6: 프론트 — Insight 탭 지표 중심화 (옵션 A, 서브그래프 제거)

**Files:**
- Modify: `webui/src/components/replay/detail/ResponseMetricsPanel.tsx` → kind 분기 가능한 `EntityMetricsPanel` 추가(기존 export 유지하며 신규 컴포넌트 도입).
- Modify: `webui/src/components/replay/detail/InsightTab.tsx`
- Modify: `webui/src/components/replay/detail/__tests__/DetailPanel.test.tsx`, `InsightTab` 관련 테스트(있으면)
- Modify: `webui/src/routes/SessionDetailPage.tsx` (InsightTab에 facets/toolMetrics 전달)

- [ ] **Step 1: 실패 테스트** — `EntityMetricsPanel.test.tsx` 신규: tool 지표/응답 지표/미수집 3케이스.

```tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { EntityMetricsPanel } from '../EntityMetricsPanel';

describe('EntityMetricsPanel', () => {
  it('renders tool metrics with meaning when kind=tool_call', () => {
    render(<EntityMetricsPanel kind="tool_call" toolMetrics={{ durationMs: 57, success: true, decisionSource: 'config', decisionType: 'accept', inputBytes: 362, resultBytes: 302, sequence: 763 }} llmMetrics={null} />);
    expect(screen.getByText(/결정 출처/)).toBeInTheDocument();
    expect(screen.getByText(/accept/)).toBeInTheDocument();
  });
  it('renders response metrics when kind=assistant_message', () => {
    render(<EntityMetricsPanel kind="assistant_message" toolMetrics={null} llmMetrics={{ requestId: 'r', durationMs: 28900, ttftMs: 3100, inputTokens: 2, outputTokens: 2300, cacheReadTokens: 290000, cacheCreationTokens: 2200, stopReason: 'tool_use', attempt: 1, success: true, model: 'claude-opus-4-8' }} />);
    expect(screen.getByText(/출력 토큰/)).toBeInTheDocument();
  });
  it('shows uncollected when no metrics', () => {
    render(<EntityMetricsPanel kind="tool_call" toolMetrics={{ durationMs: null, success: null, decisionSource: null, decisionType: null, inputBytes: null, resultBytes: null, sequence: null }} llmMetrics={null} />);
    expect(screen.getByText(/미수집/)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: 실패 확인** → FAIL.

- [ ] **Step 3: 구현** — `EntityMetricsPanel.tsx`: `ResponseMetricsPanel`의 Row+InfoTip 패턴 재사용. props `{kind, toolMetrics, llmMetrics}`. kind가 tool_call이면 toolMetrics 행(소요/결과/결정출처/입출력 크기/seq, 의미 InfoTip 추가), assistant_message|thinking면 기존 응답 지표 행. 모두 null이면 "지표 미수집" 배지.
  - 도구 InfoTip 텍스트 예: 결정 출처="이 도구가 실행되도록 허용된 근거. config=설정의 자동 허용, user=사용자 승인 등." 입력/결과 크기="도구에 전달된 입력과 반환된 결과의 바이트 크기."
  - `formatDuration`/`formatTokens`는 `llmRequestMetrics`에서 재사용. byte는 간단 포맷(예: `362 B`, `>=1024 → x.x KB`)을 toolMetrics.ts 혹은 패널에 추가.

- [ ] **Step 4: InsightTab 수정** — `FocusedInsightGraph` import/사용 제거. 구성: `<EntityMetricsPanel .../>` + findings(기존 NodeDetail의 findings 블록 또는 별도). NodeDetail의 헤더(아이콘·label·node_id)는 유지하되 per-kind 얕은 섹션은 EntityMetricsPanel로 대체. 입력 파라미터 요약은 tool_call일 때 간단히 유지(전문은 Raw).

- [ ] **Step 5: SessionDetailPage 배선** — `buildEntityFacets(effectiveGraph.nodes, effectiveGraph.edges)` 메모; 선택 노드의 facetGroup→facet 노드들→`buildToolMetrics`. 응답 metrics는 기존 `metricsByReq`/selectedNode의 request_id로. InsightTab에 toolMetrics·llmMetrics 전달.

- [ ] **Step 6: 테스트 통과 + DetailPanel 테스트 갱신** — 기존 thinking 경로(ResponseMetricsPanel) 회귀 없게. `npx vitest run src/components/replay/detail 2>&1 | tail -20` → PASS.

- [ ] **Step 7: 커밋**

```bash
git add webui/src/components/replay/detail/ webui/src/routes/SessionDetailPage.tsx
git commit -m "feat(webui): metrics-led Insight tab (option A) — EntityMetricsPanel, drop subgraph"
```

---

## Task 7: 프론트 — 메시지 뷰 박자만 (classify 정교화)

**Files:**
- Modify: `webui/src/components/replay/stream/streamModel.ts`
- Modify: `webui/src/components/replay/stream/__tests__/buildStreamModel.test.ts`

- [ ] **Step 1: 실패 테스트 추가** — metric_sample·otel_span·facet성 log(tool_result/tool_decision/api_request/hook_*)는 행 미생성; 상태변화 log(compaction/skill_activated/permission_mode_changed/mcp_server_connection)는 activity로 유지; 메시지·도구 호출은 그대로.

```ts
it('drops telemetry/facet events from the stream but keeps state-change logs', () => {
  const ev = (kind: string, payload: any, id: string) => ({
    event_id: id, session_id: 's', observed_at: '2026-05-31T00:00:0' + id + 'Z',
    actor: 'system', kind, payload, is_sidechain: false,
  } as any);
  const items = buildStreamModel([
    ev('metric_sample', { instrument_name: 'claude_code.token.usage' }, '1'),
    ev('otel_span', { raw_span: { name: 'claude_code.tool' } }, '2'),
    ev('log_record', { event_name: 'tool_result', attributes: {} }, '3'),
    ev('log_record', { event_name: 'compaction', attributes: {} }, '4'),
  ]);
  // metric/span/facet-log → 행 없음; compaction만 activity 1개
  const acts = items.filter((i) => i.type === 'activity-run');
  const evIds = acts.flatMap((a: any) => a.events.map((e: any) => e.event.event_id));
  expect(evIds).toContain('4');
  expect(evIds).not.toContain('1');
  expect(evIds).not.toContain('2');
  expect(evIds).not.toContain('3');
});
```

- [ ] **Step 2: 실패 확인** → FAIL (현재 모두 activity).

- [ ] **Step 3: 구현** — `classify()`에 분류 추가. 상수:

```ts
const STREAM_STATE_LOG = new Set([
  'compaction', 'skill_activated', 'permission_mode_changed', 'mcp_server_connection',
]);
```
classify에서:
- `e.kind === 'metric_sample' || e.kind === 'otel_span'` → `{ cat: 'drop' }`.
- `e.kind === 'log_record'`: `event_name`이 `STREAM_STATE_LOG`에 있으면 `{ cat: 'activity' }`, 아니면 `{ cat: 'drop' }`.
- 나머지(tool_call/hook_event/user_message scaffold 등)는 기존 그대로.
> 주의: tool_call은 계속 activity(접힌 도구 카드). hook_event는 현행 유지 여부 확인 — 설계상 hook은 박자로 볼 수 있으나, 노이즈면 STREAM_STATE_LOG 정책과 동일하게 후속 조정. 이번엔 기존 동작 유지.

- [ ] **Step 4: 통과 + 회귀 확인** — `npx vitest run src/components/replay/stream 2>&1 | tail -20` → PASS. 라이브 append/thinking 마커 테스트 회귀 없음.

- [ ] **Step 5: 커밋**

```bash
git add webui/src/components/replay/stream/
git commit -m "feat(webui): message view shows beats only — drop telemetry/facet rows"
```

---

## Task 8: 프론트 — Raw 뷰 source별 분할 (엔티티 + facet)

**Files:**
- Modify: `webui/src/components/replay/detail/RawTab.tsx`
- Modify: `webui/src/components/replay/detail/DetailPanel.tsx`, `webui/src/routes/SessionDetailPage.tsx` (facet raw 데이터 전달)
- Create: `webui/src/components/replay/detail/RawTab.test.tsx`

- [ ] **Step 1: 실패 테스트** — RawTab이 여러 source 블록(엔티티 raw + facet raw)을 source별 헤더와 함께 렌더.

```tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { RawTab } from '../RawTab';

describe('RawTab', () => {
  it('renders source-split blocks for entity + facets', () => {
    render(<RawTab nodeId="call" blocks={[
      { source: 'transcript', label: 'tool_call', record: { tool_name: 'Bash' } },
      { source: 'log_record', label: 'tool_result', record: { event_name: 'tool_result' } },
    ]} />);
    expect(screen.getByText(/transcript/)).toBeInTheDocument();
    expect(screen.getByText(/log_record/)).toBeInTheDocument();
  });
  it('falls back to single record (back-compat)', () => {
    render(<RawTab nodeId="x" record={{ a: 1 }} />);
    expect(screen.getByText(/"a"/)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: 실패 확인** → FAIL.

- [ ] **Step 3: 구현** — `RawTab` props 확장: `blocks?: Array<{source: string; label: string; record: unknown}>`. blocks가 있으면 각 블록을 source 헤더 + `JsonTree`로 렌더(expansion 키는 `${nodeId}:${source}:${i}`). 없으면 기존 `record` 단일 경로.

- [ ] **Step 4: 데이터 배선** — `SessionDetailPage`: 선택 엔티티의 facetGroup → facet 노드들의 `source_event_ids` → 각 raw fetch(`useEventRawQuery` 다건 또는 batch). 간단화: 1차엔 graph 노드 payload를 직접 source 블록으로 사용(이미 nodes에 payload 있음 — 추가 fetch 불필요). 엔티티 payload + facet 노드 payload들을 `node_kind`별 블록으로 구성해 RawTab에 전달. (전체 raw record가 필요하면 후속에서 fetch 확장.)
  - 블록 source 라벨: 엔티티=`transcript`(또는 node_kind), facet 노드는 `node_kind`(log_record/otel_span) + payload.event_name/raw_span.name.

- [ ] **Step 5: 통과 + DetailPanel 배선 테스트** → PASS.

- [ ] **Step 6: 커밋**

```bash
git add webui/src/components/replay/detail/ webui/src/routes/SessionDetailPage.tsx
git commit -m "feat(webui): Raw view shows entity + facets split by source"
```

---

## Task 9: 통합 검증 — 재ingest + 브라우저 smoke

**Files:** 없음(런타임 검증). CLAUDE.md "UI는 브라우저 smoke 후 commit".

- [ ] **Step 1: 빌드 + 전체 테스트**

Run: `cargo test 2>&1 | tail -15` (PASS) · `cd webui && npx vitest run 2>&1 | tail -15` (PASS) · `npx tsc --noEmit 2>&1 | tail` · `cargo build --release 2>&1 | tail -3`.

- [ ] **Step 2: DB 재생성 + 재ingest** (facet 엣지는 rebuild 필요)

```bash
witmcc init-db
witmcc ingest --all
```
Expected: 에러 없이 완료. (운영주의: 기존 dev DB 폐기·재생성.)

- [ ] **Step 3: serve + 브라우저 검증** — `witmcc serve` 후 claude-in-chrome으로 세션 상세 진입:
  - 메시지 뷰: metric/span raw row가 사라지고 도구 카드/메시지/추론마커만.
  - 도구 카드 선택 → Insight 탭: 소요/결과/결정출처/입출력크기/seq + 의미 ⓘ. 서브그래프 없음.
  - 응답·추론 선택 → 토큰·캐시·종료사유 등(기존 패턴 유지).
  - Raw 탭: transcript/log_record(/otel_span) source별 분할.
  - 라이브 append·finding 하이라이트 회귀 없음.

- [ ] **Step 4: implementation-notes 갱신** — `docs/implementation-notes.html`에 facet_of + 3-뷰 변경 섹션 추가(설계 편차·결정 기록). 커밋.

```bash
git add docs/implementation-notes.html
git commit -m "docs(insight): facet correlation + 3-view notes"
```

---

## Task 10: PR

- [ ] **Step 1: 푸시 + PR 생성**

```bash
git push -u origin feat/facet-correlation-insight
gh pr create --title "feat: facet correlation + 지표 중심 Insight 재설계" \
  --body "$(cat <<'EOF'
## 요약
상호보완 facet(transcript·log·span)을 신뢰 단일 키(tool_use_id·request_id)로 연관하는 backend facet_of 엣지 + 프론트 세 뷰(메시지 fold / 지표 Insight / Raw 분할).

## 변경
- 백엔드: graph-builder facet_of 엣지 (migration 없음, 재ingest 필요)
- 프론트: buildEntityFacets·toolMetrics, 지표 중심 Insight(서브그래프 제거), 메시지 뷰 박자만, Raw source 분할
- metric_sample은 범위 밖(향후 시계열 뷰)

## 검증
- cargo test / vitest 전부 green
- 실 fixture로 facet 연관 잠금
- 브라우저 smoke 완료

설계: docs/superpowers/specs/2026-05-31-witmcc-facet-correlation-insight-design.md
EOF
)"
```

> 운영주의 PR 본문/CLAUDE.md 갱신: facet_of 엣지 추가, `witmcc init-db`+재ingest 필요.

---

## Self-Review 결과
- **Spec coverage:** §4 Layer1→Task1·2·3; Layer2 메시지뷰→Task7, Insight→Task5·6, Raw→Task8; 에러처리→Task6(미수집)·Task8(fallback); 테스트→각 Task; metric 비범위→명시. 커버됨.
- **Type consistency:** `buildEntityFacets`→`FacetGroup`, `buildToolMetrics`→`ToolMetrics`, `EntityMetricsPanel` props `{kind,toolMetrics,llmMetrics}` Task5·6 일관. `facet_of`/`basis` 백·프론트 일관.
- **열린 질문:** 도구 span span-tree 연관(Raw 완전성)은 이번 비범위 — 필요 시 후속 Task. 상태변화 log 마커 형태는 Task7에서 기존 activity 유지로 결정.
