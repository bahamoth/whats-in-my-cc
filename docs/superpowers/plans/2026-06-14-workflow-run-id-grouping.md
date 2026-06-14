# Workflow run_id 결정론적 그룹핑 (turn_id 추론 대체) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:executing-plans / subagent-driven-development. Steps use checkbox (`- [ ]`).

**Goal:** 워크플로우 fan-out 그룹핑을 내가 발명한 `turn_id` 추론 대신 **하네스가 이미 파일로 남기는 결정론적 키 `subagents/workflows/<runId>/`** 기반으로 재구성한다. 병렬·파이프라인 모두 정확, 소급(재ingest/backfill) 가능, 텔레메트리 불필요.

**근거(real-data anchored):** 워크플로우 에이전트 transcript·사이드카는 `<sessionId>/subagents/workflows/<runId>/agent-<id>.{jsonl,meta.json}`에 떨어진다(현재 세션 probe + 옛 653ea169 run 디렉터리 6개 실측). run id는 `raw_event.source_uri`에 보존됨 → backfill 가능. 사이드카 `meta.json={agentType:"workflow-subagent"}`. journal.jsonl이 멤버십 보강. OTel `parent_agent_id`는 워크플로우 에이전트엔 null(미사용 확정).

**Architecture:** 백엔드 ingest가 경로에서 `workflow_run_id`를 뽑아 observed_event의 1급 컬럼으로 승격(+사이드카 ingest 확장) → API DTO 노출 → webui가 `workflow_run_id`로 그룹핑(turn_id 라우팅 폐기). 워크플로우 이름은 run id ↔ main `Workflow` tool_call 결과(run id 포함)로 연결.

---

## File Structure
- `migrations/20260614120000_0024_observed_workflow_run_id.sql` (create)
- `src/model/observed.rs` (modify) — `workflow_run_id: Option<String>`
- `src/ingest/subagent_meta.rs` (modify) — `workflow_run_id_from_path` + `sidecar_path_parts`가 workflows 층 처리(+ run_id 반환)
- `src/ingest/store.rs` (modify) — transcript 이벤트·사이드카 이벤트에 workflow_run_id 주입
- `src/db/repo_observed.rs` (modify) — insert/select 컬럼 + `backfill_workflow_run_id`
- `src/main.rs` (modify) — serve/ingest 시작 시 backfill 호출
- `src/api/dto.rs` + events serialize (modify) — DTO에 workflow_run_id
- `webui/src/api/types.ts` (modify) — `workflow_run_id`
- `webui/src/components/replay/stream/streamModel.ts` (modify) — turn_id 라우팅 → run_id 그룹핑
- tests: `subagent_meta.rs`, `tests/` ingest, `repo_observed`, webui `buildStreamModel.test.ts`

---

## Task 1: migration 0024 + ObservedEvent 필드 + repo insert/select

- [ ] **Step 1: migration** `migrations/20260614120000_0024_observed_workflow_run_id.sql`:
```sql
-- Workflow run grouping: subagents spawned by the Workflow tool are filed under
-- <sessionId>/subagents/workflows/<runId>/agent-<id>.{jsonl,meta.json}. The runId
-- is the deterministic group key (turn_id drifts; OTel parent_agent_id is null for
-- workflow agents). Captured from the file path at ingest; existing rows backfilled
-- from raw_event.source_uri by repo_observed::backfill_workflow_run_id (serve start).
ALTER TABLE observed_event ADD COLUMN workflow_run_id TEXT;
CREATE INDEX IF NOT EXISTS idx_obs_workflow_run
  ON observed_event(session_id, workflow_run_id) WHERE workflow_run_id IS NOT NULL;
```
- [ ] **Step 2: struct** `src/model/observed.rs` — `agent_id` 필드 아래에 `pub workflow_run_id: Option<String>,`. (`#[serde(default)]`/Default 이미 derive면 자동.)
- [ ] **Step 3: repo insert** `src/db/repo_observed.rs` — INSERT 컬럼 목록·VALUES·`.bind(&e.workflow_run_id)` 추가(agent_id 옆). select(row→ObservedEvent)에 `workflow_run_id: r.try_get("workflow_run_id").ok()` 추가.
- [ ] **Step 4: 빌드/기존 테스트** `cargo build` → 컴파일. Run `cargo test repo_observed` (있으면) Expected: green.
- [ ] **Step 5: 커밋** `feat(db): observed_event.workflow_run_id 컬럼(0024) + repo`

## Task 2: 경로 파서 (workflow_run_id + 사이드카 workflows 층)

- [ ] **Step 1: 실패 테스트** `src/ingest/subagent_meta.rs` `#[cfg(test)]`:
```rust
#[test]
fn workflow_run_id_from_workflow_path() {
    let p = std::path::Path::new("/x/SESS/subagents/workflows/wf_abc/agent-a1.jsonl");
    assert_eq!(workflow_run_id_from_path(p).as_deref(), Some("wf_abc"));
    let plain = std::path::Path::new("/x/SESS/subagents/agent-a1.jsonl");
    assert_eq!(workflow_run_id_from_path(plain), None);
}
#[test]
fn sidecar_parts_handles_workflows_layer() {
    let p = std::path::Path::new("/x/SESS/subagents/workflows/wf_abc/agent-a1.meta.json");
    let sc = sidecar_path_parts(p).unwrap();
    assert_eq!(sc.session_id, "SESS");
    assert_eq!(sc.agent_id, "a1");
    assert_eq!(sc.workflow_run_id.as_deref(), Some("wf_abc"));
}
```
- [ ] **Step 2: red** `cargo test -p wimcc subagent_meta` Expected: FAIL.
- [ ] **Step 3: 구현** — `SidecarRef`에 `pub workflow_run_id: Option<String>` 추가. `sidecar_path_parts`를 두 레이아웃 모두 처리하도록:
```rust
pub fn workflow_run_id_from_path(path: &Path) -> Option<String> {
    // .../subagents/workflows/<runId>/agent-*.{jsonl,meta.json}
    let run = path.parent()?;                       // <runId>
    if run.parent()?.file_name()?.to_str()? != "workflows" { return None; }
    if run.parent()?.parent()?.file_name()?.to_str()? != "subagents" { return None; }
    Some(run.file_name()?.to_str()?.to_string())
}
```
`sidecar_path_parts`: file_name → agent_id(strip agent- / .meta.json). 그 다음 parent가 "subagents"면 기존 경로(session=parent.parent, run=None). parent.parent가 "workflows"이고 그 위가 "subagents"면 workflows 경로(run=parent, session=subagents의 부모). 그 외 None.
- [ ] **Step 4: green** `cargo test -p wimcc subagent_meta` Expected: PASS.
- [ ] **Step 5: 커밋** `feat(ingest): workflow run_id 경로 파서 + 사이드카 workflows 층`

## Task 3: store.rs — 이벤트에 workflow_run_id 주입

- [ ] **Step 1: 구현(transcript)** — store.rs `for mut ev in evs {` 루프 안, redaction 직후에:
```rust
ev.workflow_run_id = subagent_meta::workflow_run_id_from_path(&meta.source_uri);
```
- [ ] **Step 2: 구현(사이드카)** — `ingest_sidecar_file`의 `ObservedEvent { … }`에 `workflow_run_id: sc.workflow_run_id.clone(),` 추가. (sidecar_path_parts가 이제 run_id 채움.)
- [ ] **Step 3: 테스트** — `tests/`에 실 fixture 경로로 ingest 후 workflow_run_id가 채워지는지(또는 단위: map 후 store 경로). 최소: 현재 세션 subagents/workflows/<run>/ 재ingest → `SELECT COUNT(*) WHERE workflow_run_id IS NOT NULL` > 0. Run: 재빌드 후 `wimcc ingest --path <그 run dir>` → sqlite 확인.
- [ ] **Step 4: 커밋** `feat(ingest): workflow 에이전트 이벤트에 workflow_run_id 주입`

## Task 4: backfill + 시작 시 호출

- [ ] **Step 1: 구현** `repo_observed.rs` `backfill_workflow_run_id`(backfill_agent_id 미러):
```sql
UPDATE observed_event SET workflow_run_id = (
  SELECT substr(s, 1, instr(s, '/') - 1) FROM (
    SELECT substr(r.source_uri, instr(r.source_uri, '/subagents/workflows/') + length('/subagents/workflows/')) s
    FROM raw_event r WHERE r.raw_event_id = observed_event.raw_event_id))
WHERE workflow_run_id IS NULL
  AND raw_event_id IN (SELECT raw_event_id FROM raw_event WHERE source_uri LIKE '%/subagents/workflows/%');
```
- [ ] **Step 2: 호출** `src/main.rs` ingest_cmd·serve 시작부의 `backfill_agent_id` 옆에 `backfill_workflow_run_id` 추가(비치명적 로그).
- [ ] **Step 3: 테스트/확인** — 재빌드 후 serve 시작 → `SELECT COUNT(DISTINCT workflow_run_id) FROM observed_event` > 0 (옛 세션 run들 복구). 옛 653ea169 워크플로우 에이전트가 run_id 받는지 sqlite 확인.
- [ ] **Step 4: 커밋** `feat(db): workflow_run_id backfill (source_uri 기반, 소급)`

## Task 5: API DTO 노출

- [ ] **Step 1: 실패 테스트** — events DTO contract 테스트에 `workflow_run_id` 기대 추가(webui `types.contract` 또는 rust dto 테스트).
- [ ] **Step 2: 구현** — `src/api/dto.rs`의 이벤트 DTO + serialize에 `workflow_run_id`. webui `types.ts` `ObservedEventDto`에 `workflow_run_id: string | null`.
- [ ] **Step 3: green + 커밋** `feat(api): events DTO에 workflow_run_id 노출`

## Task 6: webui — run_id 그룹핑 (turn_id 라우팅 폐기)

- [ ] **Step 1: 실패 테스트** `buildStreamModel.test.ts` — 기존 turn_id 워크플로우 테스트를 run_id 기반으로 교체:
```ts
const wfAgent = (ag, ev, run, text) => base({ event_id: ev, kind:'assistant_message', is_sidechain:true, agent_id:ag, workflow_run_id: run, payload:{text} });
it('같은 workflow_run_id 사이드체인 에이전트 → WorkflowGroup', () => {
  const evs = [ asstMain('m1','wf'), wfCall('m1','wfc','tu','t1','review'),
    base({event_id:'au',kind:'user_message',is_sidechain:true,agent_id:'A',workflow_run_id:'wf_x',payload:{content:'p'}}),
    wfAgent('A','a1','wf_x','A끝'),
    base({event_id:'bu',kind:'user_message',is_sidechain:true,agent_id:'B',workflow_run_id:'wf_x',payload:{content:'p'}}),
    wfAgent('B','b1','wf_x','B끝'), asstMain('m2','종합 X') ];
  const items = buildStreamModel(evs);
  const wf = items.find(i=>i.type==='workflow-group');
  expect(wf.agentGroups.map(g=>g.agentId).sort()).toEqual(['A','B']);
});
it('파이프라인(다른 turn이라도) 같은 run_id면 한 그룹', () => { /* A turn t1, B turn t2, 같은 wf_x → 한 WorkflowGroup */ });
```
- [ ] **Step 2: red** `cd webui && npx vitest run ...buildStreamModel...` FAIL.
- [ ] **Step 3: 구현** — streamModel: `scTurnByKey`/`wfCallsByTurn` 라우팅을 **`scRunByKey`(agent_id→workflow_run_id, 첫 이벤트에서 캡처)** 로 교체. flush 라우팅: mid 있으면 batch, 아니면 `workflow_run_id` 있으면 `wf-<run_id>` 버킷, 아니면 solo. WorkflowGroup.name/taskEventId: 로드된 이벤트 중 tool_name='Workflow'이고 그 payload/매칭 tool_result에 run_id 문자열 포함하는 tool_call을 찾아 parseWorkflowMeta(script).name + event_id. 없으면 name=null.
- [ ] **Step 4: green + 회귀** `npx vitest run` 전체 + `npx tsc -b`.
- [ ] **Step 5: 커밋** `feat(webui): WorkflowGroup을 workflow_run_id로 그룹핑(turn_id 추론 폐기)`

## Task 7: 브라우저 smoke (실 워크플로우, CLAUDE.md 의무)

- [ ] **Step 1:** 재빌드 release + serve(backfill 적용) + vite. 옛 653ea169 또는 현재 probe 워크플로우 세션을 브라우저로.
- [ ] **Step 2:** 확인 — 파이프라인 워크플로우(예 653ea169 wf_*)가 **단계가 흩어지지 않고 한 WorkflowGroup**으로(이전 분리 버그 해소), Agent-배치는 여전히 teal BatchGroup, 미그룹 떼 감소. claude-in-chrome 스크린샷.

## Self-Review
- **Spec coverage:** run_id 추출(T2)·주입(T3)·소급(T4)·노출(T5)·그룹핑(T6)·검증(T7)·스키마(T1). turn_id 추론 폐기=T6.
- **남은 별개 한계(이 plan 범위 밖, 명시):** duration 윈도 절단(전체 run 소요는 journal.jsonl/전체 이벤트 필요) — 후속. 워크플로우 이름 매칭은 run_id↔tool_call 결과 문자열 의존(없으면 name=null로 degrade).
- **degrade:** workflow_run_id 없는(텔레메트리/구조 부재) 세션은 기존대로 — 워크플로우 에이전트가 run_id 없으면 solo로(추론 안 함, 정직).
