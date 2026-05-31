# Telemetry Facet Fold + Episode Redesign — Design

- 날짜: 2026-05-31
- 브랜치: `facet-fold-episode-redesign`
- 발견 맥락: episode 분류 부정확성 문제제기(`docs/superpowers/issues/2026-05-31-episode-classification-issues.md`)에서 출발 → 근본 원인이 "텔레메트리를 1급 타임라인 노드로 취급"에 있음을 데이터로 확인하며 범위 확장.
- 선행 문서: 위 issue, `docs/superpowers/plans/2026-05-30-episode-classifier-drift-fix.md`(drift off-by-one 한정 — 본 설계가 포함·대체).
- source of truth 사양: `docs/02_technical_architecture_spec.html`(파이프라인), `docs/03_data_model_spec.html §6`(Episode, telemetry facet), `docs/04_api_mcp_spec.html`.

## 1. 문제 (데이터로 검증됨)

사용자가 본 증상: 세션 `01fe9550`의 한 episode가 `log_record` 8건만 담은 채 `intake`로 표시됨. 분류가 무의미.

조사 결과 세 겹의 문제가 확인됨(전부 dev DB 실측):

### 1.1 텔레메트리가 1급 노드로 1:1 승격 (진짜 뿌리)

- 전역 `graph_node` 30,025개 중 **19,548개(65.1%)가 텔레메트리**(log_record/metric_sample/otel_span/hook_event/session_state/attachment_meta). 대화·행위 백본은 10,477개뿐.
- 노드당 병합 이벤트 수 = **1.0** (log/metric/span 전부) → 하나도 안 묶이고 각자 독립 노드.
- 세션 `01fe9550`: log 230 + span 206 + metric 191 = 627 / 전체 ~924 이벤트 = **68% 텔레메트리**.

### 1.2 지난 `facet_of` 작업은 "표시용 연결"이지 "통합"이 아니었음

- `facet_of` edge(`graph/build.rs:677-726`)는 텔레메트리를 노드로 **그대로 둔 채** 일부에만 연결선만 추가. webui에서 표시할 때 그룹핑.
- 연결 커버리지: metric **0%**, otel_span **21%**, log_record **43%**. 나머지는 고아 노드로 타임라인·episode 스트림에 잔존.
- **노드 수를 하나도 줄이지 않음.** 그래서 episode는 여전히 65~68% 텔레메트리 위에서 계산됨.

### 1.3 episode 분류기 자체의 결함

- **분류는 오염 스트림 위에서 수행**: 경계가 `log:`/`metric:` 이벤트에 박힘. 도입부(세션 `01fe9550`은 첫 user_message가 세션 시작 **3분 17초 후**)의 startup 텔레메트리가 기본값 `Intake`를 상속 → phantom "intake" episode.
- **누적 버그**: `rebuild_session`은 graph는 delete-후-insert하지만 `episode` 테이블은 `INSERT OR REPLACE`만 하고 세션 단위 delete를 안 함(`graph/build.rs:42` vs `:56`). 라이브 rebuild마다 "마지막 열린 episode"가 새 `end_event_id`로 적재 → 한 시작 이벤트가 **124개 episode**의 시작점. 전역 episode 6,934개 / zero·negative-duration 555개.
- **프론트 겹침 해소 오류**: `phaseOf`가 "첫 매칭"을 골라 가장 넓은 stale episode가 배지로.
- **에러 경로 버그**: 분류기 `is_error_result`가 `payload.get("is_error")`(최상위)를 읽으나 실제 데이터는 `payload.tool_result.is_error`에 있음. **2157건의 실제 에러가 분류기엔 0건** → `diagnosis`/`repair` phase가 6,934건 중 **0건 생성**(L1 `tool_failure` 추출기는 올바른 경로를 읽어 정확히 2157 finding 생성 — 같은 데이터, 분류기만 틀린 경로).

### 1.4 효용·소비처 (전수 추적)

- **load-bearing 소비처는 단 하나**: L1 `missing_verification` 추출기가 `intake`/`action`/`verification` 3개 phase로 윈도우 판정 → finding 362건.
- 나머지(WebUI EpisodeStrip/Timeline/phaseOf, HTTP `/episodes`)는 **순수 장식**. Graph edge·인과추론·Judge(L2)·MCP 6 tools/resources는 episode를 **전혀 안 씀**.
- `drift`/`exploration`/`diagnosis`/`repair`는 어떤 finding·edge·judge로도 이어지지 않음.

## 2. 근본 원인 사슬

```
텔레메트리(log/metric/span)를 1급 노드로 1:1 승격            ← 뿌리
  └ 지난 facet_of는 "연결 edge"만, 노드 제거 안 함
     ├─→ 그래프/스트림의 65~68%가 텔레메트리
     ├─→ episode가 오염 스트림 위에서 계산 → 무의미 경계 + phantom intake
     ├─→ 누적 버그(episode 무한 적재) + 프론트 "첫 매칭" → stale 배지
     └─→ 에러 경로 버그 → diagnosis/repair 영구 0건
```

핵심: episode "분류 정확도"는 **증상**이다. 근본 처방은 ① 텔레메트리를 facet으로 fold해 깨끗한 백본을 만들고, ② 그 위에서 episode를 결정론적으로 재정의(또는 강등), ③ 잔존 버그 제거.

## 3. 설계 원칙

사용자 확정 원칙:

1. **묶을 수 있는 것만 묶는다.** 결정론적 소유키가 있으면 fold, 없으면 **강제로 묶지 않는다**.
2. **raw_event는 single source of truth.** facet은 raw를 대체하지 않는다 — 데이터 통합을 통한 인사이트 + 성능 개선을 위한 **파생(materialized) 사전분석 레이어**.
3. **텔레메트리·메타 이벤트는 노드 스트림에서 사라지고** 타입별로 올바른 경로(fold / 세션facet / drop)로 간다.

프로젝트 기존 원칙(CLAUDE.md): OTel-first(trace_id/span_id는 1급 상관키), Source-preserving, Evidence-linked, Schema versioning, TDD red 우선.

## 4. Fold 분류 규칙 (결정론적 소유키 기준)

각 이벤트를 **소유 이벤트가 결정론적으로 있는가**로 가른다.

### Group A — fold into owner (독립 노드 제거)

| 이벤트 | 소유키 | fold 후 facet 데이터 | 키 출처(검증) |
|---|---|---|---|
| `api_request` 로그 | `request_id` → assistant 턴 | cost_usd · duration_ms · 토큰 · model | payload.attributes.request_id |
| `llm_request` span | `request_id` → assistant 턴 | span duration · 트레이스 트리(parentSpanId) | raw_span.attributes[] (flatten_attrs) |
| `tool_result`/`tool_decision` 로그 | `tool_use_id` → tool_call | 권한 allow/deny · 결과 미러 | payload.attributes.tool_use_id |
| `attachment_meta` (type:file) | transcript parent/uuid → 그 user_message | @첨부 파일 메타 | event_uuid/parent_uuid 체인 |

소유키가 있는 이벤트는 owner 노드 payload의 `facets` 하위에 접고 **standalone 노드·facet_of edge를 만들지 않는다**. (현 `facet_of` edge 메커니즘을 fold로 대체.)

### Group B — 세션 레벨 facet (per-event로 강제하지 않음)

| 이벤트 | 이유 | 처리 |
|---|---|---|
| `metric_sample` (304건/세션, request/tool/span 키 0개) | 10초 타이머 샘플, 소유 이벤트 없음 | 세션 레벨 지표 시계열 facet (active_time·token.usage·session.count 등) |
| `session_state` `permissionMode` | 세션 상태 게이지 | 세션 레벨 facet(또는 상태변경 마커) |

소유키 없는 것을 "가장 가까운 이벤트"에 붙이면 비결정론적 → **원칙 1 위반, 금지.**

### Group C — drop (raw에만 보존, 노드·facet 둘 다 안 만듦)

- `session_state` `leafUuid` (transcript 내부 포인터)
- `attachment_meta` `deferred_tools_delta` (툴 레지스트리 변동)
- `hook_execution_start/complete` 로그, `mcp_server_connection`, `session.count` 등 운영 로그

raw_event(SSOT)에는 그대로 남아 감사·재처리 가능. 그래프/타임라인엔 투영 안 함.

## 5. 아키텍처

```
raw_event  ← SSOT, 불변 (전부 보존; redaction/integrity 유지)
   │  결정론적 fold (request_id / tool_use_id / transcript-uuid)
   ├─→ facet 레이어 (materialized 파생 — usage_facet 패턴 확장)
   │     · turn facet : cost_usd/duration/tokens/model/trace-tree   (Group A: api_request·llm_request span)
   │     · tool facet : permission decision / result mirror         (Group A: tool_decision·tool_result 로그)
   │     · attachment : file 첨부 메타 → user_message              (Group A)
   │     · session metrics series : 지표 시계열                     (Group B: metric_sample)
   │     · session state : permissionMode                          (Group B)
   └─→ graph / timeline / episode 는 "fold된 백본"만 소비
         = 대화·행위 노드 ~10,477개 (텔레메트리 노드 ~19,548개 소멸)
```

- 구현 위치: `graph::build::compute()`의 단일 패스 안에서 분기 — "노드로 materialize / owner facet에 fold / 세션 facet에 적재 / drop". 별도 배치 없음(기존도 인라인). 현재 노드 materialization 1패스 + facet_of 2패스를 **단일 패스로 통합**해 상수도 절감.
- facet 저장: owner 노드 payload의 `facets` 하위 객체(가장 경량) + 세션 레벨은 별도 경량 테이블 또는 세션 summary. (구현 계획에서 owner-payload vs 별도 facet 테이블 최종 확정 — 기본안: Group A는 노드 payload fold, Group B는 세션 테이블.)

## 6. Episode 처분 (Tier 분할)

깨끗한 백본 위에서 episode를 재정의한다.

### Tier 1 — 결정론적 구조 마커 (유지, 재명명)

`intake`(user_message) · `action`(변경툴 Edit/Write/MultiEdit/Bash) · `verification`(VerificationRun). 이는 **사실 관측**이지 인지단계 해석이 아님 → "phase"가 아니라 **구조적 세그먼트**로 재명명. 결정론적이라 신뢰 가능.

- 추적 1차 렌즈: 백본의 연속 동종 이벤트를 묶는 **결정론적 태그** + (가능하면) **렌더타임 그룹핑**. 영속 분류기·누적 없음.
- `missing_verification` 추출기: episode 테이블 의존을 끊고 **raw 신호(user_message / mutation tool / verification_run)에서 윈도우를 직접 파생**. 동작 동일, 입력만 결정론적 사실로.

### Tier 2 — 해석 휴리스틱 (삭제)

`drift`(read 8연속) · `exploration`(읽기=탐색) · `diagnosis`(에러 후 읽기) · `repair`(실패 후 변경). 결정론 테스트 통과 못 함(같은 행위 시퀀스가 생산적일 수도 헤맴일 수도). 실데이터에서 diagnosis/repair는 0건. **노드·finding·edge·judge 어디로도 이어지지 않음** → 전면 삭제.

### 결과

- phantom "intake"(startup 텔레메트리 상속) 소멸 — 백본엔 그 구간 노드가 없음.
- 누적 버그·프론트 겹침·에러 경로 버그 모두 제거(영속 분류기 폐기 또는 결정론 태그로 대체).
- WebUI EpisodeStrip/Timeline/phaseOf는 렌더타임 태그 그룹핑으로 전환.

## 7. 비용 (코드 + 볼륨 근거)

- **상관 비용 추가 0**: 빌더는 이미 O(N) 단일 패스 + request_id/tool_use_id HashMap을 가짐. fold는 같은 맵·같은 패스 재사용. O(N)은 "N개 이벤트를 각각 1회 처리"라 불가피한 바닥이지 fold가 더하는 오버헤드가 아님(해시맵은 O(N²) 회피용).
- **쓰기 비용 감소**: 노드 30,025 → ~10,477 (**-65%**), facet_of·텔레메트리 엣지 제거, episode 6,934 → 수백(누적 fix).
- **다운스트림 감소**: episode 분류 입력 65%↓, 그래프 직렬화/SSE payload 65%↓.
- **신규 비용 작음**: metric 집계 O(M) group-by, fold 분기 몇 개, 1회 migration.
- 순효과: **비용 음수(cheaper)**. 라이브 rebuild(OTLP 배치마다)도 더 가벼워짐.

## 8. 범위 / 단계 (각 단계 독립 테스트 가능, TDD red 우선)

> 구체 task 분해는 writing-plans로. 아래는 단계 골격.

- **Phase 0 — 빠른 버그 차단(안전망)**: (a) `rebuild_session`이 episode를 세션 단위 delete-후-insert(graph처럼). (b) 에러 경로 `payload.tool_result.is_error`로 수정 + real-fixture invariant. *주의: Phase 2에서 episode 분류기를 폐기/대체하면 이 수정의 일부는 흡수됨 — Phase 0은 중간 상태 회귀 방지용 최소 수정으로 한정하거나, 계획 단계에서 Phase 2와 병합 여부 결정.*
- **Phase 1 — Telemetry fold**: Group A fold(노드 payload facet), Group B 세션 facet, Group C drop. `facet_of` edge 메커니즘을 fold로 대체. 단일 패스 통합. migration. 백본 노드만 남는지 invariant.
- **Phase 2 — Episode 재정의**: Tier2 분류·테이블·UI 삭제. Tier1을 결정론 구조 세그먼트로(렌더타임 또는 경량 태그). `missing_verification`을 raw 신호 파생으로 재작성(finding 동등성 회귀).
- **Phase 3 — WebUI**: EpisodeStrip/Timeline/phaseOf 전환, Raw view facet 표시 정리, 텔레메트리 노드 비노출. 브라우저 smoke.
- **Phase 4 — 문서**: `implementation-notes.html` 갱신, spec `§6` Episode 정의 정합(Tier1만), 운영 주의(migration + init-db + 재ingest).

## 9. 테스트 전략 (TDD)

모든 코드 변경은 **실패 테스트 먼저**(CLAUDE.md). real-data anchoring: 주장은 docs 인용 또는 `tests/fixtures/**/real/` 실 payload invariant로 잠금.

- fold 정확성: 실 transcript+OTLP fixture에서 (a) api_request 로그 → 해당 assistant 턴 facet에 cost/duration이 결정론적으로 fold됐는지, (b) tool_use_id 로그 → tool_call facet, (c) request_id 없는 metric은 per-event로 안 붙고 세션 facet으로 갔는지, (d) Group C가 노드·facet 둘 다 안 만들고 raw엔 남는지.
- 백본 invariant: fold 후 graph_node에 텔레메트리 node_kind가 0인지.
- episode: Tier1 세그먼트 결정론(동일 입력 동일 출력), Tier2 phase가 산출물에 부재, `missing_verification` finding이 raw-파생으로 기존과 동등(362건 회귀 비교).
- 비용/볼륨: fold 후 세션당 노드 수가 백본 수준으로 감소하는지(회귀 가드).
- 누적·에러버그 회귀 net.

## 10. Non-goals

- raw_event 삭제·변형 (SSOT 불변).
- 비결정론적 fold(소유키 없는 이벤트를 근접 이벤트에 강제 결합).
- Tier2류 해석 라벨의 "개선된 휴리스틱" 재도입 — 본 설계는 삭제. (별도 evidence-linked 기능, 예: 결정론적 에러 재발 루프 감지는 *다른* 트랙.)
- Claude Code 설정/hook 변경, 개선 patch 생성 (CLAUDE.md non-goals).

## 11. 열린 질문 / 리스크

- **facet 저장 형태**: Group A를 owner 노드 payload `facets`에 인라인 fold할지, 별도 facet 테이블로 정규화할지 — 기본안은 노드 payload(경량), 계획 단계에서 API/MCP 노출 계약(`04_api_mcp_spec`)과 대조해 확정.
- **Tier1 영속 여부**: 완전 렌더타임 그룹핑(영속 0) vs 경량 결정론 태그 영속. `missing_verification`이 raw 파생으로 가면 영속 불필요 가능 — 계획에서 확정.
- **Phase 0와 Phase 2 병합**: episode 분류기를 폐기하면 누적·에러버그 수정이 무의미해질 수 있음 — 중간 회귀 안전망으로 Phase 0를 둘지, 바로 Phase 2로 갈지 계획에서 결정.
- **spec §6 정합**: `03_data_model_spec.html`의 7-phase 정의를 Tier1 3종으로 축소 → source-of-truth 문서 수정 필요(사용자 승인 영역).
