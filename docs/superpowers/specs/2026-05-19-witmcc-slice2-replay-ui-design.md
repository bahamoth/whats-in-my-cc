---
title: witmcc slice-2 design — read-only Session Replay UI
status: design (awaiting plan)
date: 2026-05-19
branch_target: slice2-replay-ui
prev_slice: slice-1 (transcript ingest + graph + Pull API)
---

# slice-2 — read-only Session Replay UI

## 0. Context

slice-1은 main에 머지 완료(MERGE bb85204). 결과:

- transcript JSONL → ObservedEvent → deterministic graph 파이프라인 동작.
- Pull API 4 endpoints (`/v1/health`, `/v1/sessions`, `/v1/sessions/{id}`,
  `/v1/sessions/{id}/graph`).
- 22 Rust tests green. host allowlist + loopback bind.

알려진 gap 중 slice-2 범위에 해당하는 것: 시각화 부재. AC-1
("single session replay") 충족을 위한 최소 read-only UI 부재.

slice-2는 위 데이터를 그대로 사용해 **브라우저에서 한 세션을 시각화**하는 데
집중한다. 추가 데이터 소스(OTel/Hook/File·Git), findings engine, redaction,
MCP, pagination은 본 슬라이스에 포함하지 않는다.

근거 문서:

- `docs/00_prd_revised.html` — execution replay가 제품 핵심 경험.
- `docs/01_product_design_spec.html` — IA(Dashboard·Replay·Why·Source·Resource).
- `docs/02_technical_architecture_spec.html` — UI server가 동일 프로세스에서 제공.
- `docs/06_mvp_execution_plan.html` — M4 (UI replay), AC-1.

## 1. Goal & non-goals

### Goal

브라우저에서:

1. 수집된 세션 목록을 본다.
2. 한 세션의 timeline(lane 기반)을 본다.
3. timeline 노드를 클릭하면 해당 event의 **raw transcript record**를 evidence로 본다.

위 흐름이 `witmcc serve` 단일 바이너리만으로 가능해야 한다.

### Non-goals (slice-2 범위 외)

- Why panel / Resource Drawer의 실제 동작 (findings 없음 — lane placeholder만).
- File Lineage view.
- Cost/Latency 차트 (OTel 없음).
- 추가 데이터 소스(OTel · Hook · File·Git) ingest.
- pagination cursor, redaction state branching, MCP server, export bundle.
- Authentication (slice-1의 loopback + host allowlist만 유지).
- 다국어. UI 텍스트는 영어로 통일.

## 2. Architecture

```
Browser (http://127.0.0.1:8787)
  React SPA (Vite build artifact)
        │
        ▼ fetch /v1/...
axum (witmcc serve)
  ├─ /v1/health, /v1/sessions, /v1/sessions/:id,
  │  /v1/sessions/:id/graph                                      (slice-1 그대로)
  ├─ /v1/events/:event_id/raw                                    (NEW)
  └─ catch-all  → embedded SPA (rust-embed)
```

핵심 결정:

- **same-origin**: SPA와 API가 모두 `127.0.0.1:8787`에서 서비스 → CORS 불필요.
  slice-1 host_allowlist 그대로 적용.
- **single binary**: `rust-embed`로 `webui/dist/`를 바이너리에 포함. 운영
  배포에 외부 자산 폴더 필요 없음.
- **catch-all fallback**: `/`, `/sessions`, `/sessions/:id`, 그리고 미정의된
  client-side route는 모두 `index.html`을 반환. `/v1/*`는 우선 매치되어
  catch-all에 닿지 않는다.
- **dev workflow**: `vite dev` (5173) + Vite proxy `/v1` → `127.0.0.1:8787`.
  운영(`witmcc serve`)에서는 embed된 빌드만 사용.

리포지토리에 추가될 톱-레벨:

```
webui/
  package.json
  tsconfig.json
  vite.config.ts
  index.html
  .nvmrc                  ← "20"
  src/
    main.tsx
    App.tsx
    routes/
      SessionListPage.tsx
      SessionDetailPage.tsx
    components/
      Timeline.tsx
      SourcePanel.tsx
      MetaStrip.tsx
      JsonView.tsx
    api/
      client.ts          ← fetch wrappers, types
      types.ts           ← Pull API response types (수동 작성, slice-1 응답과 1:1)
```

Rust 측 추가:

```
src/api/static_assets.rs  ← rust-embed asset struct + Axum handler
src/api/raw.rs            ← GET /v1/events/:event_uuid/raw 핸들러
```

build.rs는 추가하지 않는다. `cargo build`는 `webui/dist/`가 존재한다고 가정한다.
README와 justfile에 `webui-build`를 cargo build 전에 실행하라고 명시한다.

## 3. API contract — new endpoint

### `GET /v1/events/{event_id}/raw`

`event_id`는 `observed_event.event_id` (ULID, 모든 ObservedEvent에 존재하는 PK).
`observed_event.raw_event_id` → `raw_event(raw_event_id)`로 조인해 raw JSONL
record를 그대로 반환. `event_uuid`는 transcript record 자체의 uuid이며 일부
record(예: `file-history-snapshot`)에는 없거나 빈 값이므로 lookup 키로는
부적합 — `event_id`를 쓴다. 클라이언트는 `graph_node.source_event_ids[0]`을
바로 이 키로 사용한다.

200 OK

```json
{
  "schema_version": "1.0",
  "event_id": "01HZ...",
  "session_id": "...",
  "source": {
    "kind": "transcript",
    "file_path": "~/.claude/projects/.../<session>.jsonl",
    "line_no": 142,
    "ingested_at": "2026-05-19T13:21:48Z"
  },
  "record": { /* raw JSONL parsed object (verbatim) */ },
  "record_type":
      "user_message" | "assistant_message" | "tool_call" | "tool_result"
    | "hook_event" | "attachment_meta" | "session_state"
    | "file_history_snapshot" | "thinking" | "system_summary" | "unknown",
  "redaction_state": "none"
}
```

`record_type`은 `observed_event.kind` 값에서 1:1로 매핑한다(`EventKind`
enum과 동일).

`source.kind`/`file_path`/`line_no`/`ingested_at`은 각각 `raw_event.source_type`
/ `source_uri` / `source_line_no` / `captured_at`에서 가져온다.

`record`는 `raw_event.payload` (BLOB) → UTF-8 디코드 → `serde_json::Value`로
파싱한 결과. unknown 필드 손실 없음.

오류:

- `404 { "error": "event_not_found" }` — event_id가 observed_event에 없음.
- `404 { "error": "raw_record_not_found" }` — observed_event는 있으나
  raw_event 조인 결과 없음(현재 스키마상 FK로 거의 발생 X).
- `410 { "error": "raw_pruned" }` — 자리만 둠. slice-2 데이터에서는 발생하지
  않음. M7 retention 도입 시 분기.

`redaction_state`는 slice-2 동안 항상 `"none"`. 후속 슬라이스에서
`"redacted" | "partial"`을 추가해도 클라이언트는 렌더링 색칠만 바꾸면 된다.

### 기존 endpoint

| Endpoint | 변경 |
|---|---|
| `/v1/health` | 변경 없음 |
| `/v1/sessions` | 변경 없음 |
| `/v1/sessions/{id}` | 변경 없음 |
| `/v1/sessions/{id}/graph` | 변경 없음 |

UI는 위 4개 + 신규 1개 = 5개 호출만 사용한다.

## 4. Routing & data flow

### Client routing

- `/` → redirect `/sessions`
- `/sessions` → SessionListPage
- `/sessions/:sessionId` → SessionDetailPage
- catch-all → "Not found" 컴포넌트(클라이언트)

### Fetch 시퀀스

- SessionListPage 마운트: `GET /v1/sessions` → 카드/표 리스트.
- SessionDetailPage 마운트(병렬):
  - `GET /v1/sessions/:id`
  - `GET /v1/sessions/:id/graph`
  - 두 응답 도착 후 timeline 렌더.
- timeline 노드 클릭(lazy): `GET /v1/events/:event_id/raw` → SourcePanel.
  `event_id`는 클릭된 graph node의 `source_event_ids[0]`을 사용한다.

### Lane 매핑

product spec lane 6개 모두 표시. slice-2 데이터로 채울 수 있는 lane만 노드를
배치한다. 빈 lane은 dimmed + placeholder.

| Lane | slice-2 매핑 |
|---|---|
| Intent | `user_message` node |
| Context | `assistant_message` node |
| Action | `tool_call` node (merged tool_result는 동일 노드의 attribute로 표시) |
| State | `file_history_snapshot` node |
| OTel | (empty) "no OTel observed in this session" |
| Quality | (empty) "no findings yet" |

lane 매핑 테이블은 클라이언트 상수(`api/laneMapping.ts`)로 둔다.

### Edge 시각화

- `message_reply`, `tool_call_to_result(merged=false)`,
  `tool_call_to_result(merged=true)` 3종.
- 색상/스타일로 구분. merged self-loop는 노드 위 짧은 곡선.
- `inferred=true` 플래그가 있으면 dashed. (slice-2 데이터에서는 거의 발생하지
  않음 — DEV-04에서 도입한 merged self-loop는 결정론적.)

## 5. UI components

### Layout

```
SessionListPage
   header (title + refresh button)
   list (table or card list, sorted last_event_at desc)

SessionDetailPage
   header (back link + sessionId)
   MetaStrip (counts: events, turns, first_at, last_at, duration)
   ┌─ Timeline (left, ~60% width) ─┬─ SourcePanel (right, ~40% width) ─┐
   │  TimeAxis                      │  Header (record_type · source)   │
   │  Lane × 6 + NodeMarker         │  JsonView                        │
   │  EdgeLayer (SVG)               │                                  │
   └────────────────────────────────┴──────────────────────────────────┘
```

### 핵심 컴포넌트

- **Timeline**: events + graph nodes/edges를 입력으로 받아 SVG로 그린다.
  내부에 horizontal scroll + wheel zoom. 키보드 ←→로 selected node 이동.
  selected node를 우측 패널과 연동.
- **SourcePanel**: 선택된 event_uuid로 lazy fetch. record_type별로 헤더
  설명을 다르게 표시. body는 JsonView로 raw record 렌더.
- **JsonView**: `react-json-view-lite` 사용. 큰 payload(>1000 라인 추정)는
  초기 collapse 후 expand 토글.
- **MetaStrip**: `events.length`, `unique turn_id 수`, `events[0].observed_at`,
  `events[-1].observed_at`로 계산.

### 디자인 시스템 / 라이브러리

- React 18, React-DOM 18, react-router-dom 6.
- TypeScript 5 strict.
- Vite 5.
- Radix UI Dialog/ScrollArea (unstyled, 접근성).
- vanilla CSS modules. Tailwind 도입 보류(YAGNI). 다크 모드는 slice-2 범위 외.
- 차트 라이브러리 도입 없음 (timeline은 직접 SVG, 별도 차트 없음).
- 상태 관리: React state + URL params. 글로벌 store 없음.

## 6. Error / empty / loading

| 상황 | UI 동작 |
|---|---|
| `/v1/sessions` 빈 배열 | "No sessions yet. Run `witmcc ingest --all`" 안내 + CLI 힌트 |
| `/v1/sessions` 5xx | full-page error + "Retry" |
| `/v1/sessions/:id` 404 | "Session not found" + 목록으로 돌아가기 |
| `/v1/sessions/:id/graph` empty nodes | 6 lane은 그리되 "no observable nodes in this session" overlay |
| raw fetch 404 | SourcePanel에 "raw record not available for this event" |
| raw fetch 410 | "raw record pruned by retention" |
| `event_uuid`가 graph nodes에 없음 | timeline에 표시되지 않음 (현재 graph 모델 한계) |
| 매우 큰 raw payload | JsonView 초기 1000 라인 truncate + "view full" 토글 |
| host_allowlist 거부 | 거의 발생 X. 발생 시 fetch 에러 메시지 그대로 |
| 새로고침 깊은 링크 | axum catch-all → `index.html` → SPA router 처리 |
| 백엔드 미기동 | fetch 실패 → "API unreachable" 안내 |

## 7. Build & dev workflow

`justfile` 추가:

```just
webui-install:
  cd webui && npm install

webui-dev: webui-install
  cd webui && npm run dev   # 5173, proxy /v1 → 8787

webui-build: webui-install
  cd webui && npm run build  # → webui/dist

serve-dev:
  cargo run -- serve --auto-migrate

build-release: webui-build
  cargo build --release
```

- `cargo build`는 `webui/dist/` 부재 시 컴파일 에러(rust-embed가
  존재하지 않는 디렉터리를 가리키므로). README와 justfile로 절차를 명시한다.
- build.rs로 자동화하지 않음 — Node 도구 체인을 Rust 빌드의 사전조건으로
  강제하면 cargo만 쓰는 사람에게 부담. 명시적 명령이 더 정직하다.
- node 버전 명시: `webui/.nvmrc` = `20`.

## 8. Testing

### Rust

- `tests/api.rs`에 raw endpoint 케이스 추가:
  - 200 OK 정상
  - 404 (없는 event_id)
  - record_type별 1건씩(user/assistant/tool_call/tool_result/file_history/system/unknown)
- `tests/static_serve.rs` (신규):
  - `GET /` → 200 html, `<div id="root">` 포함
  - `GET /sessions/abc` → 200 html (SPA fallback)
  - `GET /v1/health` → 여전히 JSON (catch-all이 침범하지 않음 회귀)
  - 임베드된 asset 경로(예: `/assets/index.js`)에 대해 200 + 적절한
    content-type
- `host_allowlist` middleware는 변경 없음.

### Frontend

- **vitest + @testing-library/react**:
  - `SessionList`: 빈/정상/에러 렌더
  - `Timeline`: events + graph로 lane/노드/edge 개수, click 핸들러
  - `SourcePanel`: raw 응답으로 헤더·json 렌더
  - `api/client`: fetch 모킹으로 URL 조립과 에러 핸들링
- playwright 도입 없음. E2E는 후속 슬라이스.

### Manual smoke (의무)

CLAUDE.md "UI 변경 시 브라우저에서 직접 검증"에 따라:

- `just build-release && ./target/release/witmcc serve --auto-migrate`
- `127.0.0.1:8787` 접속 → 목록 → 상세 → 노드 클릭 → raw preview 표시까지
  golden path 확인.
- 빈 DB 상태 / 큰 tool 출력 / unknown record / file_history_snapshot
  edge case 확인.

## 9. Acceptance criteria

- `cargo build --release`로 만든 단일 바이너리가 `webui/dist/`를 임베드한 채
  `witmcc serve`만 실행해도 브라우저에서 동작한다.
- 세션 리스트에서 임의 세션을 골라 상세에 들어가면 6 lane이 모두 표시되고,
  데이터가 있는 lane(Intent/Context/Action/State)에 노드가 배치되며 edge가
  그려진다.
- 임의 노드를 클릭하면 1초 이내 raw transcript record가 우측 패널에
  표시된다.
- `/v1/*` 호출은 slice-1 응답 계약과 동일하며, 추가된
  `/v1/events/:event_id/raw`는 본 문서 §3의 스펙을 따른다.
- Rust + Frontend 테스트가 모두 green.
- 새로고침으로 deep link(`/sessions/:id`)에 진입해도 정상 렌더.

## 10. Out-of-scope follow-ups (later slices)

- OTel / Hook / File·Git ingest → OTel lane 활성화.
- findings engine → Quality lane / Why panel / Resource Drawer 활성화.
- redaction state branching, retention pruning(410 분기 실데이터).
- pagination cursor for `/v1/sessions`.
- MCP Streamable HTTP server.
- File Lineage view, export bundle UI.
- 차트(token/cost/latency) — OTel 도입 이후.
- Playwright E2E.
- build.rs 자동 webui-build 통합.

## 11. Risks & mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| `webui/dist/` 없이 `cargo build` 실행 | 빌드 실패 | README + justfile + 에러 메시지 명확화 |
| timeline 노드가 너무 많아 SVG 성능 저하 | 큰 세션 렌더 지연 | slice-2 데이터 규모에서는 무시 가능. 후속 슬라이스에서 가상화. |
| `event_uuid`가 graph 노드에 없는 raw 이벤트 | 클릭 못함 | lane은 graph node 기반이므로 자연스럽게 숨겨짐. 인지 필요 시 "n events not visualized" hint. |
| node 도구 의존성 도입 | 개발 환경 요구 증가 | `.nvmrc`로 버전 고정, README에 명시 |
| Vite/React/TS 메이저 업그레이드 부담 | 유지보수 비용 | 의존성 lockfile 커밋, 업그레이드는 별도 슬라이스 |

## 12. Open decisions (later, not blocking)

- 다음 슬라이스로 갈 때 OTel를 먼저 채울지 findings v1을 먼저 채울지.
- `react-json-view-lite` 대신 직접 작성한 JsonView로 가는 게 더 가벼울지.
- Tailwind 도입 시점(slice-3에서 차트 들어올 때가 자연스러움).
