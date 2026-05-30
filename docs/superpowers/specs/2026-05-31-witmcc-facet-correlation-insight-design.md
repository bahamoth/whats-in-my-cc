# WitMCC — Facet 연관 + 지표 중심 Insight 재설계

- 날짜: 2026-05-31
- 브랜치: `feat/facet-correlation-insight`
- 상태: 설계 (브레인스토밍 산출물). 구현은 writing-plans로 별도 계획.

## 1. 배경

세션 상세 화면의 대화 스트림이 하나의 논리적 행동(도구 호출 1건)을
`tool_call` + `otel_span` ×N + `log_record` ×N + `metric_sample` ×N 의 **raw row
7~10개로 줄줄이** 펼쳐 보여, "여러 소스가 중복된 것 아니냐"는 의심을 부른다.
우측 Insight 패널(`DetailPanel` → `InsightTab`)은 서브그래프 + 얕은 `NodeDetail`
+ findings를 보여줄 뿐, 수집된 풍부한 실행 지표를 표면화하지 못한다.

## 2. 조사 결론 (실데이터, `.witmcc.sqlite` 세션 `2c5d9a5a…`)

**중복이 아니라 상호보완이다.** 같은 `tool_use_id`·같은 timestamp를 비교한 결과
겹치는 필드는 join key + 상태 플래그(`tool_use_id`/`tool_name`/`success`)뿐이고
나머지는 전부 고유했다.

| source | 고유 기여 |
|--------|----------|
| transcript `tool_call`/`tool_result` | 명령 텍스트 + **실제 출력 내용** + is_error |
| `log_record` | duration_ms · 입출력 byte 크기 · **decision_source/type** · event.sequence · prompt.id |
| `otel_span` `llm_request` | 소요·ttft·토큰·캐시·종료사유·시도·모델 (응답 지표 전체) |
| `metric_sample` | 모델별·타입별 토큰 · 비용 · active_time (세션 집계) |

→ 어느 source도 버릴 수 없다. 문제는 데이터가 아니라 **표현**이다:
`streamModel.classify`가 message/thinking/drop이 아닌 모든 것을 `activity`로 떨궈
시간 인접성만으로 한 줄씩 나열한다.

### 2.1 join 키 검증 (Real-data anchoring)

- **도구 호출 ↔ `log_record`**: `tool_use_id`. `tool_decision` 250/250,
  `tool_result` 248/248 매칭. `log_record`는 `tool_use_id`와 trace_id/span_id를
  **둘 다** 가진다(다리 역할).
- **응답·추론 ↔ `otel_span` `llm_request`**: `request_id` (span 속성에 존재).
  248개. thinking·assistant_message가 같은 `request_id`를 공유. 기존
  `buildLlmRequestMetrics`가 이미 이 join을 한다.
- **`otel_span` tool 계열** (`claude_code.tool`/`tool.execution`/`blocked_on_user`,
  748개): `tool_use_id` 없음. trace_id는 **세션당 1개**라 무용 →
  `log.span_id → 부모 tool span → 자식`의 **span-트리워크**로만 연관 가능.
  duration/success는 log에 이미 중복 → 헤드라인 지표 가치 낮음.
- **`metric_sample`** (1589개): `request_id`·`prompt.id` 둘 다 없음 → per-entity
  귀속 불가. 세션 단위(model/type)로만 의미. 이미 usage facet이 소비.

## 3. 목표 / 비목표

**목표**
1. 합쳐질 수 있는(= 같은 엔티티를 신뢰 키로 가리키는) 상호보완 facet을 그 엔티티에
   **연관**시켜, Insight 뷰가 실행 지표를 한곳에 의미와 함께 보여준다.
2. 대화 박자가 아닌 이벤트(`metric_sample`, tool 계열 span)를 메시지 뷰·타임라인의
   **행에서 제외**해 읽히게 한다(노이즈 분류 ≠ 합치기).
3. Raw 뷰에서 엔티티의 facet을 source별 분할 JSON으로 본다.

**비목표 (의도적 제외)**
- 합쳐지지 않는 데이터의 강제 통합. 키로 안 묶이면 같은 데이터가 아니므로 합치지 않는다.
- `metric_sample`의 WebUI 표면. **이번 범위 밖** — 구현 종료 후 별도 시계열 뷰로 다룬다
  (저장·Pull API·MCP엔 그대로 보존).
- 헤드라인 지표를 span-트리(불확실 연관)에서 끌어오는 것. span 연관의 용도는
  **Raw 그루핑 한정**.
- hidden reasoning 복원, annotation/correction write (프로젝트 비목표 유지).

## 4. 아키텍처 — 두 개의 독립 관심사

### Layer 1 — 백엔드: facet 연관 (데이터)
graph-builder(`rebuild_session`)가 신뢰 키로 **facet 엣지**를 생성한다.

- 새 `edge_kind = "facet_of"`. `from = facet 노드`, `to = 엔티티 노드`.
  `graph_edge` 스키마 그대로 사용 → **migration 불필요**. 단 그래프 로직 변경이므로
  `witmcc init-db` + 재ingest 필요(표준 운영주의).
- `attributes`에 연관 근거를 기록(evidence-linked·provenance):
  - `{ "basis": "tool_use_id" }` — log_record → tool_call (deterministic, confidence 1.0)
  - `{ "basis": "request_id" }` — llm_request span → assistant_message (deterministic)
  - `{ "basis": "span_tree" }` — tool 계열 span → tool_call (structural, **Raw 전용**,
    낮은 confidence). headline 지표 계산에 사용 금지(소비자가 basis로 구분).
- source 노드는 보존하고 지표를 payload로 복제하지 않는다(staleness 방지·source-preserving).
- Pull API `GET /v1/sessions/{id}/graph`와 MCP graph resource가 이 엣지를 자동 노출.

### Layer 2 — 프론트: 세 뷰 (화면)
facet 엣지를 소비하는 순수 view-model을 두고 세 표면이 공유한다.

- **순수 함수** `buildEntityFacets(nodes, edges)` → `Map<entityNodeId, FacetGroup>`.
  jsdom 테스트(기존 `insightCards.ts`/`llmRequestMetrics.ts` 패턴).
- **메시지 뷰**(`streamModel`/`ConversationStream`): facet·telemetry 이벤트를 행으로
  그리지 않는다. 분류 정교화 — §6.1.
- **Insight 뷰**(`InsightTab`): 지표 중심. 서브그래프 제거. §6.2.
- **Raw 뷰**(`RawTab`): 엔티티 + facet을 source별 분할. §6.3.

## 5. 백엔드 상세

1. graph-builder에 facet 연관 패스 추가:
   - 인덱스: `tool_use_id → {tool_call node, log_record nodes}`,
     `request_id → {assistant_message node, llm_request span node}`.
   - `tool_use_id`/`request_id` 일치 시 `facet_of` 엣지 생성(basis 기록).
   - (선택, Raw 완전성) `log_record.span_id`에서 부모 `claude_code.tool` span을 찾고
     그 자식 span들에 `facet_of`(basis=span_tree) 생성. 실패 시 조용히 생략.
2. confidence/inference_rule_id 컬럼은 이미 존재(slice-13). deterministic 엣지는
   confidence 1.0, span_tree는 낮은 값.
3. 재ingest 후 엣지 수 sanity 로그.

## 6. 프론트 상세

### 6.1 메시지 뷰 — 박자만
`classify`를 확장해 이벤트를 세 부류로 나눈다.
- **박자(beat)**: user/assistant 메시지, 추론 마커, 도구 호출(접힘),
  그리고 transcript에 없는 진짜 상태변화 `log_record`
  (`compaction`·`skill_activated`·`permission_mode_changed`·`mcp_server_connection`).
- **facet**: 엔티티에 `facet_of`로 붙은 log_record / llm_request span. 행 미표시
  (해당 엔티티 카드·Insight·Raw에서 표현).
- **telemetry(세션집계)**: `metric_sample`, tool 계열 span. 행 미표시.
주의: 분류는 **데이터를 버리지 않는다** — Raw·집계·도구카드로 보존(§3 목표 2).

### 6.2 Insight 뷰 — 지표 중심 (옵션 A 확정)
구성: `헤더(아이콘·라벨·node_id)` → `지표 그리드(의미 ⓘ)` → `Findings`.
`FocusedInsightGraph` **제거**(그래프 탐색은 하단 Timeline이 담당).
kind별 지표(출처 명시):

- **tool_call** (출처: log via tool_use_id):
  소요시간(duration_ms) · 결과(success/is_error) · 결정 출처(decision_source·type) ·
  입력/결과 크기(byte) · 순서(event.sequence). 입력 파라미터는 간략, 전문은 Raw.
- **assistant_message / thinking** (출처: llm_request span via request_id):
  소요시간 · ttft · 출력/입력 토큰 · 캐시 읽기/생성 · 종료사유 · 시도 · 성공 · 모델.
  잘림(max_tokens)/재시도/실패 시 ⚠. 기존 `ResponseMetricsPanel`을 일반화.

토큰·캐시·결정출처 등 오해 쉬운 항목엔 `InfoTip` 설명(추론 가시화와 동일 패턴).
연관 facet이 없으면(예: log 미수집) transcript 정보 + "지표 미수집" 배지로 정직 degrade.

### 6.3 Raw 뷰 — source별 분할
선택 엔티티 + 그 facet 노드들을 source별 블록으로 분할 표시
(transcript / log_record / otel_span). 도구 호출이면 span_tree facet 포함.
각 블록은 `JsonTree`. facet 없으면 엔티티 raw만.

## 7. 데이터 흐름
1. graph query(full, 비windowed)가 nodes+edges(+facet_of) 반환.
2. `buildEntityFacets`가 엔티티→facet 맵 구성.
3. 노드 선택 → Insight: 엔티티 kind에 맞는 facet 지표 렌더. Raw: facet 분할.
4. 메시지 뷰: `streamModel`이 facet/telemetry를 drop, 박자만 카드로.
- 윈도우 무관: facet 지표는 full graph의 facet 노드 payload에서 오므로 스크롤과 독립.

## 8. 에러 처리 / degradation
- facet 엣지 없음 → Insight는 transcript-only + "지표 미수집"(uncollected 배지).
- facet 노드 payload 필드 결손 → 해당 행 `—`.
- span-트리 연관 실패 → 도구 Raw에 span 생략(transcript+log만). 비치명.
- graph 로딩 실패 → 기존 빈/에러 상태 유지.

## 9. 테스트 계획 (TDD red-first, CLAUDE.md 준수)
- **백엔드**: `tests/fixtures/**/real/`의 동결 payload로 facet 연관 패스 검증 —
  주어진 tool_use_id/request_id 세트에 대해 `facet_of` 엣지의 from/to/basis 단언.
  span_tree 연관은 실데이터 트리(예: tool f7bc6e95 ← execution 92441e52)로 단언.
- **프론트(jsdom)**:
  - `buildEntityFacets`: nodes+edges → 엔티티→facet 맵(키별·basis별).
  - `streamModel`: facet/telemetry 이벤트 drop, 상태변화 log_record 유지, 박자 보존.
  - Insight 패널: tool kind=log 지표 / response kind=span 지표 / facet 없음=미수집.
  - Raw 탭: 엔티티+facet source별 분할.
- **회귀**: 기존 추론 마커→ResponseMetricsPanel 경로, 라이브 append, finding 하이라이트.
- **브라우저 smoke**: `witmcc serve` + claude-in-chrome로 시각 검증 후 commit
  (CLAUDE.md "UI는 브라우저 smoke 후 commit").

## 10. 구현 순서 (개략 — 상세는 plan에서)
1. 백엔드 facet_of 연관(키 기반) + 테스트 + 재ingest.
2. `buildEntityFacets` + streamModel 분류 정교화(메시지 뷰 노이즈 제거).
3. Insight 뷰 지표화(서브그래프 제거, kind별 지표, InfoTip).
4. Raw 뷰 source 분할(+ span_tree facet).
5. 각 단계 브라우저 smoke.

## 11. 열린 질문
- span_tree facet 연관을 1차에 포함할지, Raw 완전성을 위한 후속으로 미룰지(작업량 trade-off).
- 상태변화 `log_record`(compaction 등)의 메시지 뷰 표현 형태(전용 마커 vs 기존 activity).
