# Judge·Graph 레이어 삭제 + 저장/상관 정리 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 사용되지 않는 LLM judge 서브시스템과 graph 레이어를 데이터 모델·코드·API에서 완전히 제거하고, deterministic L1 finding 파이프라인으로 단순화하며, 낭비되는 compute/IO·중복 저장을 해소한다.

**Architecture:** findings 파이프라인은 이미 `view.events`만 사용(graph node 미사용)하므로 event-first로 정리한다. ① judge 삭제 → 4개 extractor 모두 deterministic L1/Always 승격. ② graph 삭제 → ingest가 graph rebuild 대신 insight 파이프라인을 직접(debounce) 호출. ③ OTel 상관키를 인덱스 컬럼으로 승격 + span 중복 저장 제거. 각 Phase는 독립적으로 green·shippable.

**Tech Stack:** Rust (axum, sqlx/SQLite, tokio), TypeScript/React (webui, vitest). 테스트: `cargo test`, `cargo build`(삭제 refactor의 1차 검증 = 컴파일러가 모든 참조처 보고), `cd webui && pnpm vitest`.

**근거 문서:** `docs/superpowers/specs/2026-06-06-wimcc-data-model.md`(전체 데이터 모델 + 비효율 분석). 모든 file:line은 2026-06-06 main 기준.

---

## Pre-flight

- 브랜치 `refactor/drop-judge-graph` 는 main에서 이미 생성됨(현재 브랜치).
- **dev DB 재생성 규칙(CLAUDE.md):** migration 추가 시 `wimcc init-db` + 재ingest 필요. Phase A·B·C 각각 migration을 추가하므로, 해당 Phase의 통합 검증 전에 DB 재생성한다.
- 커밋 메시지는 프로젝트 hook이 `Co-Authored-By`/AI footer를 거부하므로 **footer 없이** 작성한다.
- 각 Phase 시작 전 `cargo build && cargo test` 가 green인지 확인(baseline).

---

## Phase A — Judge 서브시스템 삭제 (deterministic L1)

**목표:** judge 관련 데이터 모델·코드·CLI·런타임을 전부 제거. 4개 extractor가 모두 자기 confidence로 직접 active finding이 된다. `findings_pending_judge`·`judge_verdict_cache` 테이블 drop.

### Task A1: behavioral red — 3개 extractor가 judge 없이 active finding이 되어야 한다

**Files:**
- Test: `src/insight/pipeline.rs` (하단 `#[cfg(test)] mod tests`) 또는 `tests/insight_l1_promotion.rs`(신규 통합 테스트)

- [ ] **Step 1: 실패 테스트 작성** — 기존 fixture 세션(예: `tests/fixtures/**/real/`의 transcript)을 ingest 후 `run_extractors`를 돌려, `risky_action`/`context_bloat`/`final_state_mismatch` 후보가 발생하는 입력에서 **status=active, provenance.layer="L1"** finding이 나오는지 단언. 현재는 pending으로 빠져 0건이라 실패.

```rust
// tests/insight_l1_promotion.rs (신규)
// 파괴적 Bash 명령이 든 합성 세션을 ingest → risky_action이 즉시 active finding이어야 한다.
#[tokio::test]
async fn risky_action_promotes_without_judge() {
    let pool = wimcc::db::test_pool().await; // 기존 테스트 헬퍼 패턴 확인 후 사용
    // (헬퍼가 없으면: in-memory sqlite + migrate + transcript fixture ingest)
    let session_id = seed_session_with_destructive_bash(&pool).await;
    let rows = wimcc::insight::pipeline::run_extractors(&pool, &session_id).await.unwrap();
    let risky: Vec<_> = rows.iter().filter(|r| r.category == "risky_action").collect();
    assert!(!risky.is_empty(), "risky_action must promote at L1 without a judge");
    assert_eq!(risky[0].status, "active");
    assert!(risky[0].provenance.contains("\"layer\":\"L1\""));
}
```

- [ ] **Step 2: 실패 확인** — Run: `cargo test --test insight_l1_promotion risky_action_promotes_without_judge`. Expected: FAIL (현재 IfAbove(1.0)→pending이라 active 0건).
  - 참고: 기존 테스트 헬퍼/픽스처 패턴은 `src/insight/extractors/*.rs`의 `#[cfg(test)]` 블록과 `tests/`를 먼저 읽어 재사용한다.

### Task A2: PromotionPolicy 제거 — extractor trait·types 단순화

**Files:**
- Modify: `src/insight/types.rs:30-39` (PromotionPolicy enum 제거)
- Modify: `src/insight/extractor.rs:15-16` (`promotion_policy()` 메서드 제거)
- Modify: `src/insight/extractors/{risky_action,context_bloat,final_state_mismatch,tool_failure}.rs` (각 `fn promotion_policy` 제거)

- [ ] **Step 1: `PromotionPolicy` enum 삭제** — `src/insight/types.rs`의 `enum PromotionPolicy {...}` 블록(30-39) 제거. `FindingCandidate`/`Provenance`는 유지.
- [ ] **Step 2: trait에서 `promotion_policy` 제거** — `src/insight/extractor.rs`에서 `fn promotion_policy(&self) -> PromotionPolicy;` 줄과 `use ...PromotionPolicy` 제거.
- [ ] **Step 3: 각 extractor의 `promotion_policy` 구현 제거** — 4개 파일에서 `fn promotion_policy(...) { ... }` 와 관련 `use` 제거. confidence/floor/severity는 그대로 둔다(risky 0.7 / context 0.5 / final 0.6 / tool_failure 1.0).
- [ ] **Step 4: 컴파일 확인** — Run: `cargo build`. Expected: pipeline.rs가 `promotion_policy`/`PromotionPolicy`를 참조해 FAIL(다음 태스크에서 해결). 다른 에러 없으면 정상.

### Task A3: pipeline을 L1-only로 재작성

**Files:**
- Modify: `src/insight/pipeline.rs` (judge 경로 전부 제거)

- [ ] **Step 1: judge 의존 코드 제거** — `run_extractors_with_runtime`, `route_candidate`, `judge_or_queue`, `enqueue_pending`, 그리고 pending drain(Step 1 블록 62-119), judge/runtime/pending import를 제거. `build_l1_row`·`all_extractors_for_pipeline`·`CONFIDENCE_FLOOR`는 유지.
- [ ] **Step 2: `run_extractors`를 L1-only로 구현**

```rust
// src/insight/pipeline.rs — 새 본문 (judge 관련 use 전부 제거)
use sqlx::SqlitePool;
use crate::db::repo_finding::{self, FindingRow};
use crate::error::Result;
use crate::ids::derive_finding_id;
use crate::insight::types::{FindingCandidate, Provenance};
use crate::insight::view::OwnedSessionInsightData;

pub const CONFIDENCE_FLOOR: f32 = 0.5;

/// Deterministic L1 finding pipeline. Every candidate ≥ its floor is promoted
/// directly to an active finding. Idempotent (`INSERT OR REPLACE`).
pub async fn run_extractors(pool: &SqlitePool, session_id: &str) -> Result<Vec<FindingRow>> {
    let data = OwnedSessionInsightData::load(pool, session_id).await?;
    let view = data.as_view(session_id);
    let mut rows = Vec::new();
    for ext in all_extractors_for_pipeline() {
        let category = ext.category();
        let floor = ext.floor();
        let cands = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ext.extract(&view))) {
            Ok(c) => c,
            Err(_) => { tracing::warn!(session_id, category, "extractor panicked; skipping"); continue; }
        };
        for c in cands {
            if c.confidence_l1 < floor.max(CONFIDENCE_FLOOR) { continue; }
            let row = build_l1_row(session_id, &c);
            repo_finding::insert(pool, &row).await?;
            rows.push(row);
        }
    }
    Ok(rows)
}
```
(`build_l1_row`와 `all_extractors_for_pipeline`은 기존 것을 그대로 둔다. `build_l1_row`의 Provenance.layer는 이미 `"L1"`.)

- [ ] **Step 3: green 확인** — Run: `cargo build` 후 `cargo test --test insight_l1_promotion`. Expected: A1 테스트 PASS.
- [ ] **Step 4: 커밋**

```bash
git add src/insight/types.rs src/insight/extractor.rs src/insight/extractors/ src/insight/pipeline.rs tests/insight_l1_promotion.rs
git commit -m "refactor(insight): drop PromotionPolicy; all extractors promote at deterministic L1"
```

### Task A4: judge 모듈·런타임·CLI 제거

**Files:**
- Delete: `src/insight/judge/` (전체 디렉터리: mod.rs, types.rs, errors.rs, runtime.rs, budget.rs, metrics.rs, cache.rs, providers/, prompts/)
- Modify: `src/insight/mod.rs` (`pub mod judge;` 제거)
- Modify: `src/cli.rs` (`JudgeMode` enum + Serve의 `judge`/`judge_budget`/`judge_fixture_path` arg 제거)
- Modify: `src/main.rs` (`JudgeRuntime` import + 170-180 분기 제거)
- Modify: `src/api/mod.rs` (`judge_runtime` 필드 + import + 80 초기화 제거; serve OTLP rebuild 경로가 runtime을 넘기면 그 인자도 제거)

- [ ] **Step 1: judge 디렉터리 삭제** — `git rm -r src/insight/judge`.
- [ ] **Step 2: 모듈 선언 제거** — `src/insight/mod.rs`에서 `pub mod judge;` 줄 제거.
- [ ] **Step 3: CLI 정리** — `src/cli.rs`에서 `enum JudgeMode`(14-23)와 `Command::Serve`의 `judge`(112-114)·`judge_budget`(115-117)·`judge_fixture_path`(118-120) 필드 제거.
- [ ] **Step 4: main.rs 정리** — `src/main.rs:2`의 `insight::judge::runtime::JudgeRuntime` import, `:170-180`의 judge 분기, Serve 호출부에 넘기던 runtime 인자 제거.
- [ ] **Step 5: AppState 정리** — `src/api/mod.rs`의 `judge_runtime` 필드(54)·import(26)·기본 초기화(80) 제거. serve 측 ingest가 `run_extractors_with_runtime`를 호출하던 곳은 `run_extractors`로 교체(이 함수는 A3에서 L1-only가 됨).
- [ ] **Step 6: 컴파일 → 잔여 참조 제거 루프** — Run: `cargo build`. 컴파일러가 남은 judge 참조를 모두 보고한다. 보고되는 파일마다 참조 제거 후 재빌드. Expected 최종: build OK.
- [ ] **Step 7: 테스트 + 커밋** — Run: `cargo test`. Expected: PASS(judge 관련 테스트는 삭제됨). 

```bash
git add -A
git commit -m "refactor(insight): remove LLM judge subsystem (module, runtime, CLI flags, AppState)"
```

### Task A5: judge·pending 테이블 drop (migration 0018) + repo 제거

**Files:**
- Create: `migrations/20260606130000_0018_drop_judge.sql`
- Delete: `src/db/repo_judge_cache.rs`, `src/db/repo_findings_pending.rs`
- Modify: `src/db/mod.rs` (해당 `pub mod` 선언 제거)
- Modify: `src/db/repo_retention.rs` (judge_cache/pending 스윕 참조 제거 — grep로 확인)

- [ ] **Step 1: migration 작성**

```sql
-- migrations/20260606130000_0018_drop_judge.sql
-- Judge subsystem removed: extractors now promote deterministically at L1.
DROP TABLE IF EXISTS judge_verdict_cache;
DROP TABLE IF EXISTS findings_pending_judge;
```

- [ ] **Step 2: repo 삭제 + 모듈 선언 제거** — `git rm src/db/repo_judge_cache.rs src/db/repo_findings_pending.rs`; `src/db/mod.rs`에서 두 `pub mod` 줄 제거.
- [ ] **Step 3: 잔여 참조 제거** — Run: `grep -rn "repo_judge_cache\|repo_findings_pending\|judge_verdict_cache\|findings_pending" src/` → retention 스윕 등 참조를 제거. 특히 `repo_retention.rs`의 `resource_kind` 스윕 목록에서 judge_cache/pending 제거.
- [ ] **Step 4: DB 재생성 + 검증** — Run: `cargo run -- init-db` (dev DB 재생성) 후 `cargo build && cargo test`. Expected: PASS. retention 테스트가 judge_cache를 참조했다면 함께 갱신.
- [ ] **Step 5: 커밋**

```bash
git add migrations/20260606130000_0018_drop_judge.sql src/db/
git commit -m "refactor(db): drop judge_verdict_cache + findings_pending_judge tables (0018)"
```

### Task A6: MCP search_findings·문서 정합성

**Files:**
- Modify: `src/api/mcp/tools/search_findings.rs` (status 필터에서 pending 관련 분기 있으면 제거)
- Modify: `docs/implementation-notes.html` (judge 제거 기록 추가)

- [ ] **Step 1: search_findings 확인/정리** — `grep -n "pending\|status" src/api/mcp/tools/search_findings.rs`. pending status 언급이 있으면 active만 반환하도록 정리(이미 active 기본이면 변경 없음).
- [ ] **Step 2: implementation-notes 갱신** — judge 서브시스템 제거 결정·근거(기본 비활성으로 3/4 extractor 휴면, 비결정/비로컬 LLM이 evidence-linked·deterministic 철학과 충돌)와 deterministic L1 전환을 `#judge-removal` 섹션으로 추가.
- [ ] **Step 3: 커밋** — `git add -A && git commit -m "docs: record judge-subsystem removal + deterministic L1"`.

---

## Phase B — Graph 레이어 삭제

**목표:** graph_node/edge 테이블·`src/graph/`·`edge_inference/`·`repo_graph` 제거. ingest는 graph rebuild 대신 insight 파이프라인을 직접 호출. `/v1/sessions/:id/graph`·MCP `get_session_graph`/`explain_node`·`finding_evidence` subgraph·프론트 graph 전부 제거. (Phase A 완료가 선행 — pipeline이 이미 L1-only.)

### Task B1: insight view에서 graph 의존 제거

**Files:**
- Modify: `src/insight/view.rs` (nodes/edges 필드 + repo_graph 로드 제거)

- [ ] **Step 1: view 구조체 정리** — `SessionInsightView`와 `OwnedSessionInsightData`에서 `nodes`/`edges` 필드 제거. `load()`에서 `repo_graph::load_session` 호출과 `use ...graph::{GraphEdge, GraphNode}`, `repo_graph` import 제거. `as_view()`에서 nodes/edges 제거.

```rust
// src/insight/view.rs — 정리 후 구조체
pub struct SessionInsightView<'a> {
    pub session_id: &'a str,
    pub events: &'a [ObservedEvent],
    pub diff_hunks: &'a [DiffHunkRow],
    pub verification_runs: &'a [VerificationRunRow],
}
pub struct OwnedSessionInsightData {
    pub events: Vec<ObservedEvent>,
    pub diff_hunks: Vec<DiffHunkRow>,
    pub verification_runs: Vec<VerificationRunRow>,
}
// load(): repo_graph 호출 제거, 나머지 3개만 로드. as_view(): 3개만 전달.
```

- [ ] **Step 2: 컴파일 확인** — Run: `cargo build`. Expected: extractor는 nodes/edges 미사용이라 OK. edge_inference/graph build가 view를 안 쓰면 OK. 에러나면 다음 태스크 대상 파일.
- [ ] **Step 3: 테스트 + 커밋** — `cargo test` → `git add src/insight/view.rs && git commit -m "refactor(insight): drop vestigial graph nodes/edges from session view"`.

### Task B2: behavioral red — ingest가 graph 없이 finding을 만든다

**Files:**
- Test: `tests/ingest_findings_no_graph.rs` (신규)

- [ ] **Step 1: 실패 테스트 작성** — transcript fixture를 ingest 후, (a) finding이 생성되고 (b) graph 테이블이 없어도 동작함을 단언. 현재는 ingest가 `rebuild_session`(graph) 경유라 graph 테이블 의존.

```rust
// tests/ingest_findings_no_graph.rs
#[tokio::test]
async fn ingest_produces_findings_via_event_first_path() {
    let pool = /* in-memory sqlite + migrate */;
    ingest_transcript_fixture(&pool, "tests/fixtures/.../real/<session>.jsonl").await;
    let sid = /* the ingested session_id */;
    let findings = wimcc::db::repo_finding::list_session(&pool, &sid).await.unwrap();
    assert!(!findings.is_empty(), "ingest must produce findings without the graph layer");
}
```

- [ ] **Step 2: 실패 확인** — Run: `cargo test --test ingest_findings_no_graph`. Expected: 이 시점엔 PASS일 수 있음(graph 아직 존재). 이 테스트는 B3 이후 graph 제거에도 **계속 PASS**해야 하는 회귀 가드. 통과하면 그대로 두고 B3 진행.

### Task B3: ingest를 graph rebuild → 직접 insight 호출로 전환

**Files:**
- Create: `src/insight/refresh.rs` (또는 `pipeline.rs`에 `refresh_session_findings` 추가)
- Modify: `src/ingest/store.rs:267`, `src/ingest/otel.rs:389`, `src/ingest/otel_logs.rs:214`, `src/ingest/otel_metrics.rs:356`, `src/ingest/hook.rs:216` (각 `rebuild_session` 호출 → insight 직접 호출)

- [ ] **Step 1: insight 진입점 노출** — `run_extractors`가 이미 공개 진입점이므로 그대로 사용하거나, 명시적으로 `pub async fn refresh_session_findings(pool, session_id)`를 `pipeline.rs`에 추가(내부에서 `run_extractors` 호출). 의도를 드러내는 이름 권장.
- [ ] **Step 2: 5개 ingest 호출부 교체** — 각 파일의 `crate::graph::build::rebuild_session(pool, session_id).await?;` 를 `crate::insight::pipeline::run_extractors(pool, session_id).await?;`(또는 refresh 래퍼)로 교체. 반환 사용처(`(usize,usize,usize)`)가 있으면 시그니처 조정.
- [ ] **Step 3: 컴파일 + 회귀 테스트** — Run: `cargo build && cargo test --test ingest_findings_no_graph`. Expected: PASS.
- [ ] **Step 4: 커밋** — `git add -A && git commit -m "refactor(ingest): run insight pipeline directly; drop per-ingest graph rebuild (Tier 2-1)"`.

### Task B4: graph 코드·API·MCP 삭제

**Files:**
- Delete: `src/graph/` (build.rs, mod.rs), `src/insight/edge_inference/` (전체), `src/db/repo_graph.rs`, `src/model/graph.rs`
- Delete: `src/api/mcp/tools/get_session_graph.rs`, `src/api/mcp/tools/explain_node.rs`
- Modify: `src/insight/mod.rs`·`src/model/mod.rs`·`src/db/mod.rs`·`src/lib.rs`(또는 main) — graph/edge_inference 모듈 선언 제거
- Modify: `src/api/mod.rs:109` (`/v1/sessions/:id/graph` route 제거)
- Modify: `src/api/routes.rs` (`session_graph` 핸들러 제거; `finding_evidence` 783-803에서 subgraph 제거 → evidence_refs + raw source refs만 반환)
- Modify: `src/api/mcp/tools/mod.rs` (get_session_graph/explain_node 등록·schema 제거), `src/api/mcp/resources/mod.rs` (graph resource 제거), `src/api/mcp/methods.rs`(필요 시)
- Modify: `src/api/dto.rs` (`GraphPayload` 등 graph DTO 제거)

- [ ] **Step 1: 코드 디렉터리/파일 삭제** — `git rm -r src/graph src/insight/edge_inference; git rm src/db/repo_graph.rs src/model/graph.rs src/api/mcp/tools/get_session_graph.rs src/api/mcp/tools/explain_node.rs`.
- [ ] **Step 2: 모듈 선언 제거** — `src/insight/mod.rs`(edge_inference), `src/model/mod.rs`(graph), `src/db/mod.rs`(repo_graph), graph mod 선언처(`src/lib.rs` 또는 main)에서 제거.
- [ ] **Step 3: route + 핸들러 제거** — `src/api/mod.rs:109` graph route 줄 제거. `src/api/routes.rs`의 `session_graph` 핸들러 제거.
- [ ] **Step 4: finding_evidence subgraph 제거** — `finding_evidence`(routes.rs:783-803)를 subgraph 없이 finding + `evidence_refs` + raw source refs만 반환하도록 수정. 응답 DTO에서 nodes/edges 필드 제거.
- [ ] **Step 5: MCP 정리** — `tools/mod.rs`에서 get_session_graph/explain_node `pub mod`·schema fn·등록 제거. `resources/mod.rs`에서 `.../graph` resource 제거. `dto.rs`에서 `GraphPayload`/graph DTO 제거.
- [ ] **Step 6: 컴파일 루프** — Run: `cargo build`. 컴파일러가 남은 graph 참조를 전부 보고 → 제거 반복.
- [ ] **Step 7: 테스트 + 커밋** — Run: `cargo test`(graph 관련 테스트는 삭제됨). 

```bash
git add -A
git commit -m "refactor: remove graph layer (tables-code, /graph API, MCP graph tools, evidence subgraph)"
```

### Task B5: graph_node/edge 테이블 drop (migration 0019)

**Files:**
- Create: `migrations/20260606140000_0019_drop_graph.sql`

- [ ] **Step 1: migration 작성**

```sql
-- migrations/20260606140000_0019_drop_graph.sql
-- Graph layer removed: views are event-first; findings use evidence_refs.
DROP TABLE IF EXISTS graph_edge;
DROP TABLE IF EXISTS graph_node;
```

- [ ] **Step 2: DB 재생성 + 검증** — Run: `cargo run -- init-db` 후 `cargo build && cargo test`. Expected: PASS. retention `resource_kind`에 graph_node가 있으면 제거.
- [ ] **Step 3: 커밋** — `git add migrations/20260606140000_0019_drop_graph.sql && git commit -m "refactor(db): drop graph_node + graph_edge tables (0019)"`.

### Task B6: 프론트엔드 graph 제거

**Files:**
- Delete: `webui/src/components/replay/insight/FocusedInsightGraph.tsx` + `.module.css` + `neighborhood.ts` + `__tests__/neighborhood.test.ts`
- Modify: `webui/src/api/client.ts:43`(getGraph), `webui/src/api/types.ts:81`(GraphPayload/GraphNodeDto/GraphEdgeDto), `webui/src/lib/queries.ts`(useSessionGraphQuery, useFindingEvidenceQuery가 subgraph 의존 시 응답 타입 조정), 관련 테스트(`__tests__/queries.test.tsx`, `client.test.ts`)에서 graph 케이스 제거

- [ ] **Step 1: dead 컴포넌트 삭제** — `git rm webui/src/components/replay/insight/FocusedInsightGraph.tsx webui/src/components/replay/insight/FocusedInsightGraph.module.css webui/src/components/replay/insight/neighborhood.ts webui/src/components/replay/insight/__tests__/neighborhood.test.ts`.
- [ ] **Step 2: API/타입/쿼리 제거** — `client.ts`의 `getGraph`, `types.ts`의 `GraphPayload`·`GraphNodeDto`·`GraphEdgeDto`, `queries.ts`의 `useSessionGraphQuery` 제거. `useFindingEvidenceQuery`/`FindingEvidenceResponse`가 subgraph(nodes/edges)를 타입에 포함하면 그 필드 제거.
- [ ] **Step 3: 테스트 정리** — `queries.test.tsx`·`client.test.ts`에서 graph 관련 케이스 제거.
- [ ] **Step 4: 검증** — Run: `cd webui && pnpm vitest run && pnpm tsc --noEmit`. Expected: PASS, 타입 에러 없음.
- [ ] **Step 5: 커밋** — `git add -A && git commit -m "refactor(webui): remove dead graph query/types/components"`.

---

## Phase C — 상관 인덱스/컬럼 승격(2-2) + span 중복 저장 제거(3-1)

**목표:** OTel 이벤트의 `attributes.{tool_use_id,request_id}`를 인덱스 컬럼으로 승격하고 `request_id` 인덱스를 추가해, 상관 쿼리의 payload JSON 스캔을 제거(Insight 재설계 직접 수혜). span 본문의 3중 저장을 줄인다.

### Task C1: behavioral red — OTel log 이벤트가 컬럼에 tool_use_id/request_id를 채운다

**Files:**
- Test: `src/ingest/otel_logs.rs` `#[cfg(test)]` 또는 `tests/otel_log_correlation_columns.rs`

- [ ] **Step 1: 실패 테스트 작성** — `attributes`에 `tool_use_id`/`request_id`가 든 OTLP log payload를 ingest 후, 생성된 `observed_event`의 **컬럼** `tool_use_id`/`request_id`가 채워졌는지 단언. 현재는 attributes에만 있고 컬럼은 NULL이라 실패.

```rust
// tests/otel_log_correlation_columns.rs
#[tokio::test]
async fn otel_log_promotes_correlation_attrs_to_columns() {
    let pool = /* in-memory + migrate */;
    let body = /* OTLP logs JSON with attributes.tool_use_id="toolu_X", attributes.request_id="req_Y" */;
    wimcc::ingest::otel_logs::store_request(&pool, /*...*/).await.unwrap();
    let ev = /* fetch the log_record observed_event */;
    assert_eq!(ev.tool_use_id.as_deref(), Some("toolu_X"));
    assert_eq!(ev.request_id.as_deref(), Some("req_Y"));
}
```

- [ ] **Step 2: 실패 확인** — Run: `cargo test --test otel_log_correlation_columns`. Expected: FAIL (컬럼 NULL).

### Task C2: ingest에서 상관키 컬럼 승격 + request_id 인덱스

**Files:**
- Modify: `src/ingest/otel_logs.rs` (ObservedEvent 생성 시 `attributes`에서 tool_use_id/request_id 추출 → 컬럼 set), `src/ingest/otel_metrics.rs` (동일, request_id가 attr에 있으면), `src/ingest/otel.rs` (span attributes에 있으면)
- Create: `migrations/20260606150000_0020_correlation_index.sql`

- [ ] **Step 1: otel_logs ingest 수정** — log_record의 ObservedEvent 생성부에서 flatten된 attributes의 `tool_use_id`/`request_id`(키 변형: `tool_use_id`, `tool.use.id` 등 실제 픽스처로 확인)를 읽어 `tool_use_id`/`request_id` 컬럼에 set. 값이 없으면 None 유지.

```rust
// otel_logs.rs ObservedEvent 빌드 직전 (flatten된 attrs 맵 a 기준)
let tool_use_id = a.get("tool_use_id").and_then(|v| v.as_str()).map(str::to_string);
let request_id  = a.get("request_id").and_then(|v| v.as_str()).map(str::to_string);
// ObservedEvent { ..., tool_use_id, request_id, ... }
```
(키 이름은 `tests/fixtures/**/real/`의 실제 OTLP log payload로 검증 후 확정 — real-data anchoring.)

- [ ] **Step 2: otel_metrics·otel.rs도 동일 적용** — attributes에 해당 키가 있을 때만 컬럼 승격.
- [ ] **Step 3: 인덱스 migration**

```sql
-- migrations/20260606150000_0020_correlation_index.sql
CREATE INDEX IF NOT EXISTS idx_obs_request_id
  ON observed_event(request_id) WHERE request_id IS NOT NULL;
```

- [ ] **Step 4: DB 재생성 + green** — Run: `cargo run -- init-db` 후 `cargo test --test otel_log_correlation_columns`. Expected: PASS.
- [ ] **Step 5: 커밋** — `git add -A && git commit -m "feat(ingest): promote OTel attributes.{tool_use_id,request_id} to indexed columns (0020)"`.

### Task C3: 상관 쿼리를 컬럼 기반으로 재작성

**Files:**
- Modify: `src/db/repo_observed.rs:374-394` (correlated-telemetry 쿼리)
- Test: 동 파일 `#[cfg(test)]` 또는 통합 테스트

- [ ] **Step 1: red — 컬럼 기반 상관 쿼리 테스트** — tool_use_id로 묶인 transcript tool_call + OTel log(이제 컬럼에 tool_use_id 있음)를 ingest 후, correlated-telemetry 조회가 둘을 반환하는지 단언. (기존 json_extract 경로가 컬럼 승격 후에도 동작하지만, 컬럼 기반으로 바꾼다.)
- [ ] **Step 2: 쿼리 재작성** — `json_extract(payload,'$.attributes.tool_use_id')` 등을 인덱스 컬럼 `tool_use_id`/`request_id` 비교로 교체. raw_span attributes의 `json_each` 스캔은 컬럼 승격(C2의 otel.rs)으로 대체.

```sql
-- 개념: payload JSON 스캔 → 인덱스 컬럼
WHERE (?1 IS NOT NULL AND tool_use_id = ?1)
   OR (?2 IS NOT NULL AND request_id  = ?2)
```

- [ ] **Step 3: green + 커밋** — Run: `cargo test`(상관 테스트 PASS). `git add -A && git commit -m "perf(db): correlate telemetry via indexed columns, not payload JSON scan (Tier 2-2)"`.

### Task C4: span 3중 저장 제거(3-1) — 영향 확인 후 적용

**Files:**
- Modify: `src/ingest/otel.rs:367` (`observed.payload = {"raw_span": span.raw}`)
- 확인: 프론트 Raw 탭이 `observed.payload.raw_span`을 읽는지 — `webui/src/components/replay/detail/rawBlocks.ts` + RawTab

- [ ] **Step 1: 소비처 확인** — Run: `grep -rn "raw_span" src/ webui/src/`. observed.payload.raw_span을 읽는 곳(Raw 탭, telemetry facet 분리 등)을 파악. telemetry facet은 이미 payload에 merge되어 span 메타데이터를 들고 있음(`repo_observed.rs:68`).
- [ ] **Step 2: 결정 분기** — (a) Raw 탭이 raw_event 원본을 별도로 가져올 수 있으면(`/events/:id/raw`는 raw_event.payload 반환) observed.payload에서 `raw_span` 통본 제거하고 telemetry facet만 유지. (b) Raw 탭이 observed.payload에만 의존하면, 통본 제거는 Raw 뷰 손실 → 이 경우 raw_span을 유지하되 raw_event verbatim과의 중복은 "audit 보존"으로 수용하고 3-1은 보류(문서화). 어느 쪽이든 근거를 implementation-notes에 기록.
- [ ] **Step 3: (a)인 경우) red 테스트** — span ingest 후 observed.payload에 `raw_span` 키가 없고 telemetry facet으로 span 메타(name/status/attrs)가 조회 가능함을 단언.
- [ ] **Step 4: 적용 + green + 커밋** — otel.rs:367을 `serde_json::json!({})` 또는 telemetry-only로 변경(결정에 따라). Run: `cargo test` + 브라우저 smoke(Raw 탭에서 span 카드 확인). `git add -A && git commit -m "refactor(ingest): stop double-storing raw_span in observed payload (Tier 3-1)"`.

> 3-2(metric/log bulk를 full observed_event 행으로 저장)는 표현·저장 구조 변경 규모가 크고 뷰가 이미 필터링하므로 **이번 범위에서 제외**(별도 트랙 후보). implementation-notes에 보류로 기록.

---

## 통합 검증 + 마무리

- [ ] **전체 green** — Run: `cargo build && cargo test && cd webui && pnpm vitest run && pnpm tsc --noEmit`.
- [ ] **DB 재생성 후 실데이터 재ingest** — `cargo run -- init-db` + 실제 transcript/OTel 재ingest. finding이 4종 모두 active로 뜨는지, 상관 telemetry가 detail에서 보이는지 확인.
- [ ] **브라우저 smoke** — `wimcc serve` + 정적 세션 navigation(`[[witmcc-smoke-use-static-session]]`)으로 detail/findings 동작 확인(graph 뷰 부재로 인한 깨짐 없음).
- [ ] **문서 정합** — CLAUDE.md(graph 전용 서술·judge 언급), 사양서 00~04 callout, implementation-notes(`#judge-removal`, `#graph-removal`, Tier 2-2/3-1) 갱신.

---

## Self-Review

- **Spec coverage:** judge 삭제(A1–A6) ✓, graph 삭제(B1–B6) ✓, Tier 2-1(B3 ingest 직접 호출) ✓, Tier 2-2(C1–C3) ✓, Tier 3-1(C4) ✓, Tier 3-2 명시적 제외 ✓.
- **Placeholder scan:** 신규 코드는 실제 코드 제시. 단 일부 테스트 픽스처/헬퍼는 "기존 패턴 확인 후 사용"으로 둠 — 실행 시 `tests/`·`#[cfg(test)]` 기존 헬퍼를 먼저 읽어 구체화할 것(삭제·migration 중심 plan의 불가피한 지점). OTLP attribute 키 이름은 real fixture로 확정(real-data anchoring).
- **Type consistency:** `run_extractors`(L1-only) 시그니처 `(&SqlitePool, &str) -> Result<Vec<FindingRow>>` 유지, ingest 호출부·view 구조체·SessionInsightView 필드명 일관.
- **순서 의존:** A(pipeline L1화) → B(graph 삭제 시 rebuild→run_extractors 교체가 깔끔) → C(독립). migration 번호 0018(judge)·0019(graph)·0020(index) 단조 증가.
