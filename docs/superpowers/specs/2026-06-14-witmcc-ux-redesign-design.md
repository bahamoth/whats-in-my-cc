# WitMCC WebUI 재설계 — 디자인 스펙 (A+B 하이브리드)

**Date:** 2026-06-14
**Status:** Design spec (brainstorming 산출물). 구현 plan 아님 — slice 분해는 writing-plans로 후속.
**Charter:** 본 문서는 `2026-05-27-witmcc-ux-redesign-epic.md` §7이 요구한 "새 디자인 스펙"이다. §3(현행 Signal 모델)을 precondition으로 소비하고 §5를 no-inheritance 목록으로 따른다.
**Builds on:** `2026-06-13-subagent-parallel-batch-grouping-design.md`, `2026-06-14-scaffold-message-grouping-design.md` (그룹핑 모델을 정련), `2026-06-07-detail-view-derived-metrics-design.md` (detail metrics).
**Mockups (living, `webui/public/`, untracked):** `redesign-directions.html`(방향 3안), `redesign-fullscreen.html`(전 화면 A+B), `batch-states.html`(배치 상태 3종). 브라우저에서 시각 검증 완료.

---

## 1. 배경과 성격

이 재설계는 **그린필드가 아니라 현재 구현된 WebUI의 시각·IA 진화**다. 헌장(2026-05-27)은 "MVP exit 이후 백지 재설계"를 상정했지만, 실제로는 dogfooding을 거치며 상당한 UI가 이미 구축됐다(InsightStrip · DetailPanel Insight/Raw · AnalysisPanel · ConversationStream의 subagent/batch/workflow 그룹핑). 따라서 본 스펙은 그 자산 위에서 **A+B 하이브리드 디자인 언어**로 재정비하고, 진단된 통점을 일괄 해소한다.

> **편차 기록:** 헌장 §1은 "패널을 조금씩 production에 박지 말 것"을 경계했으나, 현 UI는 그 경고에도 불구하고 점증적으로 쌓였다. 본 스펙은 그것을 되돌리는 게 아니라 **일관된 언어로 흡수**하는 선택을 한다(데이터 레이어는 §5대로 보존).

### 1.1 사용자와 잠긴 결정 (2026-06-14 brainstorming)

| 결정 | 값 |
|---|---|
| 범위 | **전 화면 통합 재설계** (세션 목록 · KPI · 스트림 · detail · analysis) |
| 디자인 강도 | **디자인 언어 발전** (현 다크 토큰 기반 위에서 새 언어 정의) |
| 방향 | **A+B 하이브리드** = B(트레이스 타임라인 골격: 좌측 시간축 spine + 노드 + 병렬 레인) × A(에디토리얼 톤: 하이라인 제거·표면 레이어·넓은 여백·절제된 색·조판) |
| 배치/서브에이전트 | **요약 상시 표시 + 내부는 인라인 sub-timeline 드릴**(option 1). 단일 에이전트=래퍼 없음, 실행 중=자동 진행 표시, ≥4 동시=밀집 spine+"+N" |
| 단일 이벤트 접힘 | **전면 제거** — 자식이 1개인 그룹/카드/스택은 인라인으로 평탄화 |

### 1.2 핵심 진단 (코드 매핑 + 라이브 스모크, 2026-06-14)

5개 에이전트 코드 매핑 + 정적 세션 `00fae5d9` 라이브 확인으로 도출. 우선순위 높은 통점:

1. **과도한 접힘** — 자식 1개인데도 접힌 껍데기를 그리는 6개 사이트(§5.1).
2. **소리 없는 잘림** — KPI 값, 툴 출력 90px, correlation ID 140px, WhatSection 2000자가 잘림 신호 없이 절단.
3. **약한 위계** — 균일한 하이라인 박스가 반복돼 무엇이 중요한지 안 보임.
4. **식별성** — 세션 목록이 raw UUID 벽. 슬러그·프로젝트·미리보기 없음.
5. **시간/베이스라인 맥락 부재** — KPI·analysis가 point-in-time. observability 도구인데 추이가 없음.
6. **낮은 발견성** — 분석 토글·? 툴팁·Raw 강조점이 눈에 안 띔.
7. **반응형 미비** — 셸/레일/우측 슬롯/세션 테이블에 내로우 대응 없음(상세만 860px에서 분기).

---

## 2. 상속·계약 제약 (반드시 준수)

### 2.1 헌장 §3 (현행 = Signal 모델)
- 증거는 **Signal**로 소비: `GET /v1/sessions/:id/signals`, `GET /v1/signals/:id` — `evidence_refs[]` 필수. **severity/confidence 같은 판단 필드는 없다** → detail의 "신호"는 사실만 보여주고 evidence 이벤트로 점프(판단 색 금지).
- episodes/graph/findings/L2 판정 경로는 **폐기** — Why Panel/Resource Drawer/그래프 노드 메타포는 도입하지 않는다.
- 자기개선 표면: `GET /v1/sessions/:id/fingerprint`(코호트 키), `GET /v1/metrics`(세션 횡단 series) — KPI 베이스라인·추이의 데이터원.
- MCP 6종(`search_sessions`·`get_file_lineage`·`get_otel_trace`·`get_session_turns`·`list_detectors`·`get_project_metrics`).

### 2.2 헌장 §5 (no-inheritance / 보존)
- **버려도 됨:** 현 lanes 레이아웃, 2단 SourcePanel, 대시보드 요약 카드, 현 라우팅.
- **보존:** 데이터 레이어 `webui/src/api/*.ts`(fetch + SSE). `webui/src/api/__tests__/*`는 재설계 전반에서 green 유지(regression lock).

### 2.3 프로젝트 원칙 (CLAUDE.md)
- **Read-only**: correction/label/status write 없음. export-bundle 외 외부 write 없음.
- **Fact-only 색 (design spec §6.3)**: analytics에 danger red로 "나쁨" 판단 금지. 색은 식별(agent-hash·lane)·상태(success/warn/danger는 *측정된 사실*일 때만)·provenance에만.
- **OTel-first · Evidence-linked · Schema-versioned · Local-first** 유지.
- **TDD red 우선**: UI 변경도 실패 테스트 먼저. **브라우저 스모크 후 commit**.
- **Real-data anchoring**: 새 필드/동작 주장은 docs 인용 또는 `tests/fixtures/**/real/` invariant로 잠금. 본 스펙의 데이터 의존성은 §6에 명시.

---

## 3. 디자인 언어 (A+B)

기존 `tokens.css`(다크 표면·팔레트·lane 색·motion) **위에** 다음을 더한다. 색 값은 그대로 재사용, 추가는 구조 토큰.

### 3.1 표면과 위계 — elevation으로 (하이라인 격자 폐기)
- 카드는 `border:1px` 격자 대신 **표면 레이어 + soft shadow**로 떠 보이게:
  `--wimcc-elev: 0 1px 0 rgba(255,255,255,.025), 0 10px 30px -22px rgba(0,0,0,.9)`.
- radius 토큰: `--wimcc-r-sm:7px · -r-md:11px · -r-lg:14px`.
- 경계가 꼭 필요한 곳(패널 분할·테이블 행)만 hairline 유지.

### 3.2 타입 스케일 (현 6종 난립 → 정돈)
- display 21px/650 (KPI 값) · title 16px/650 · body 13.5px/1.6 · meta 12px · label 10.5px uppercase tracked · mono 10–11px(ID·시간·수치).
- 본문 line-height 1.6으로 상향(A의 가독성).

### 3.3 간격 리듬
- 4px 그리드 유지. 턴 간 수직 리듬 18–22px(현재 대비 확대). 카드 내부 패딩 13–15px.

### 3.4 색 규율 (fact-only)
- **human=accent blue**, **scaffold=violet**, **agent=hash 색**(세션 내 안정), **lane**: batch=teal · workflow=orange · scaffold=violet.
- provenance pill: `측정`=green-tint · `혼합`=amber-tint · `추정`=gray-tint · `미수집`=outline. (값 옆 상시 표기.)
- duration heat(warn 10s / hot 60s)는 **측정된 사실**이므로 유지하되 범례 제공(§7.4).

### 3.5 컴포넌트 프리미티브 (재설계의 빌딩 블록)
`TimelineSpine` · `EventNode`(user/asst/tool/think/batch 변형) · `EventCard`(bubble) · `ToolLine`(단일·인라인) · `ToolPill`(멀티·요약) · `FanPanel`(batch/workflow 요약) · `AgentLane`(간트+ID) · `SubTimeline`(드릴된 에이전트 내부) · `EndCard` · `KpiCard`(값+provenance+sparkline) · `MetricRow`(label+value+provenance+ⓘ) · `IdChip`(복사 가능) · `ProvenanceBadge`.

---

## 4. 정보 구조 / 내비게이션

- **라우팅(헌장 §5 자유):** `/sessions`(목록) · `/sessions/:id`(replay). 딥링크 `?selected=<ulid>` 유지. 재설계는 replay-first지만 목록은 유지(다중 세션 비교가 자기개선 루프의 측정면).
- **좌측 레일:** 폭 유지(56–60px). active 인디케이터(좌측 accent 바) 추가, 추후 항목(분석·설정) 자리 확보.
- **TopBar 브레드크럼:** `Sessions / <slug> <project-pill> ● live`. **raw UUID 대신 슬러그**(transcript payload `slug` 필드 — Raw에서 실측). UUID는 hover/복사로.

---

## 5. 횡단 원칙 — 단일 항목 접힘 제거

> 자식이 정확히 1개인 컨테이너는 **접힌 껍데기를 그리지 않는다.** 인라인으로 평탄화하거나, 컨테이너가 유지돼야 하면 자식 수를 헤더에 표기해 "글리치"로 안 읽히게 한다.

### 5.1 대상 사이트 (코드 위치)
| 사이트 | 현재 | 변경 |
|---|---|---|
| `streamModel.ts:1131` WorkflowGroup | N=1도 래퍼+간트(1바)+내부 SubagentGroup 이중 chevron | **N=1 시 래퍼 제거**, 자식을 바로 렌더(batch `:1144`와 대칭). 워크플로 이름/상태/종합은 그 자식에 한 줄 주석으로. |
| `ActivityStack`(1 event) | `Read · 1 events ›` chevron | **임계값 도입**: run 길이 1(선택적으로 1–2)은 인라인 `ToolLine`(chevron 없음). per-item 상태/에러도 인라인. |
| WorkflowGroup→단일 SubagentGroup | 이중 껍데기 | 위 대칭화로 해소 |
| `InsightStrip` 카드(reading 1개) | 클릭 토글 | 단일 reading은 카드에 인라인, 클릭=점프 |
| `AnalysisPanel` 디텍터(signal 1개) | 클릭 토글 | 단일 signal은 인라인, 클릭=evidence 점프 |
| `SubagentGroup`(얕은 leaf) | 항상 chevron | 결론 프리뷰는 유지하되 자식 수 라벨 추가 |

### 5.2 잠그는 테스트 (TDD)
`tests/` 또는 webui vitest에 "N=1 워크플로우는 WorkflowGroup 노드를 만들지 않는다", "1-event run은 toggle을 렌더하지 않는다" 등의 red 테스트를 먼저 둔다. `streamModel` 분기는 순수 함수라 단위 테스트 용이.

---

## 6. 화면별 설계

### 6.1 세션 목록 (`SessionListPage`)
- 행: **슬러그(굵게) + 프로젝트 pill + live + 모델** / 첫 사용자 메시지 **미리보기**(1줄 말줄임) / **상대시간**("3분 전") / event 수(mono) / source mix(txn·otel·hook).
- 상단: 제목 + 카운트 + **검색**(프로젝트·슬러그). 정렬 헤더 유지.
- 반응형: 내로우에서 카드 스택으로.
- **데이터 의존성:** `/v1/sessions` 응답에 `slug` · `first_user_message_preview` · `model`(또는 dominant model) · `project` 추가 필요(현재 미포함 — 백엔드 작업, real-fixture로 잠금). relative time은 프론트 계산.

### 6.2 세션 상세 레이아웃 (`SessionDetailPage`)
- 그리드: `[KPI strip] / [meta row + 분석 토글] / [stream | detail(400px, 가변)]`. analysis는 토글 시 KPI와 본문 사이 full-width.
- 브레이크포인트 800px(현 860 정정)에서 1열 스택. 우측 슬롯 가변폭(현 380 고정 → min/max + 향후 드래그).

### 6.3 KPI 스트립 (`InsightStrip`)
- 5카드: label · **값 21px(잘림 없음)** · provenance pill · 보조 detail · **sparkline(추이) + 베이스라인 델타**.
- sparkline 색은 의미 따라 tint(검증=green·실패=amber·맥락/비용=blue), fact-only.
- **데이터 의존성:** intra-session 추이는 `/v1/sessions/:id/turns`(턴별 집계) 또는 metrics series, 베이스라인은 `/v1/sessions/:id/fingerprint` 코호트. 비용 측정 swap은 보류(메모리: KPI 측정-비용 on hold) → **비용 카드는 `추정` 유지**, 추이/베이스라인은 비-비용 카드부터.

### 6.4 대화 스트림 (`ConversationStream`)
- 좌측 **시간축 spine**(옅은 1px 그라데이션) + 시간 라벨. 이벤트마다 **노드**(user=accent·asst=muted·tool=green·think=violet·batch=orange).
- 카드는 elevation bubble(A 톤), 턴 간 여백 확대. human=우측·accent, asst=좌측.
- **단일 툴=ToolLine 인라인**(`↳ Read package.json ✓ 28 lines  18ms`), **멀티=ToolPill 요약**(`⌗ Bash×3·Read  6 events ›  2.6s`).
- thinking=인라인 한 줄(tok·duration, 내용 없음 — 메모리: thinking 내용 미기록).
- scaffold(커맨드·스킬): violet, N≥2만 그룹(현 규칙 유지), 단일은 인라인.
- 선택 상태=accent outline + 배경 틴트. 개별 노드 클릭→우측 detail.
- 시작 마커는 hairline+gutter 아이콘으로 축소(현 40px 회수). autoscroll 토글·pending 배지는 footer 유지.

### 6.5 병렬 배치 / 서브에이전트 / 워크플로우 (핵심)
잠긴 모델(`batch-states.html`):
- **요약 상시(접기 없음):** spine 주황 노드 → `FanPanel`(tag·agent수·상태·소요) + `AgentLane`×N(색 스와치·이름·ID·간트 막대·duration) + **종합 한 줄**.
- **드릴(option 1, 인라인):** 레인 클릭 → 그 에이전트 내부가 `SubTimeline`(에이전트 색 spine, 프롬프트→툴→응답→종합 노드)으로 in-place 펼침. 평면 아코디언 금지. "전체 펼치기" 토글.
- **단일 에이전트:** FanPanel 래퍼 없이 SubTimeline이 메인 spine에 직접(§5 대칭화). batch·workflow 동일 규칙.
- **실행 중(settled=false):** 자동 진행(완료 레인 ✓, 진행 레인 shimmer 막대 + `● 실행 중`, 헤더 `⏳ k/N`). 완료 시 요약으로 정착. 명시적 사용자 토글이 우선.
- **≥4 동시:** 레인 대신 밀집 spine + `+N` 배지(현 gutter density 계승), 종합은 유지. (별도 목업 후속.)
- **End card:** subagent/workflow 종료 카드 유지(결정론적, task-notification 동기화). 시작↔종료 양방향 점프 추가(현재 backward-only).

### 6.6 Detail 패널 (`DetailPanel`)
- 탭 `Insight | Raw`(더 크게, Raw 강조점은 방문 후에도 유지).
- 헤더: kind 아이콘 + 모델 + `원본/가공` 배지 + **IdChip(복사 가능, 잘림 시 전체 hover)**.
- **소제목으로 묶기:** `WHAT — 한 일` / `LLM 동작`(TTFT·stop_reason·attempts) / `토큰`(입력·캐시읽기·출력) / `검증` / `비용`. 각 `MetricRow`에 **provenance pill**과 ⓘ(plain-language tip).
- **신호(Signal):** `/v1/sessions/:id/signals` 소비, evidence_refs로 점프. 판단 색 없음. 단일 signal 인라인.
- 잘림 해소: WhatSection 2000자 절단은 "Raw 탭에서 전문" 앵커 + fade로 명시.

### 6.7 Analysis 패널 (`AnalysisPanel`)
- 토글 버튼에 아이콘(막대) + 안정 위치(발견성).
- 세션 지표 테이블(도구실패율·검증통과율·context bloat) + **detector 분포 막대**(fact-only blue) + 단일 signal 인라인 드릴(클릭=evidence 점프).
- 추이: detector별 시점 분포(언제 실패가 몰렸나) — metrics series 소비. max-height 하드코딩(400px) 제거, 가변.

---

## 7. 횡단 개선

### 7.1 소리 없는 잘림 가시화
KPI 값·툴 출력(90px)·correlation ID(140px)·WhatSection(2000자) 전부 **말줄임+툴팁 / expand / "전문" 앵커**로 명시하고 ID는 복사 가능.

### 7.2 발견성
분석 토글 아이콘화 · ? 툴팁 핀 동작 힌트 + 스크롤 시 위치 재계산 · Raw 강조점 의미 유지.

### 7.3 반응형 · 라이트모드
브레이크포인트 800px 정규화. 레일/우측 슬롯 내로우 대응. 세션 테이블 카드 스택. 라이트모드는 토큰 override로 일괄(헌장 §6: 재설계의 일부).

### 7.4 a11y · 범례
이벤트 kind 아이콘 aria-label · duration heat/lane 색 dismissible 범례 · 스트림 키보드(j/k 노드 이동, z 그룹 토글, e 다음 에러, / 검색).

---

## 8. 우선순위 로드맵 (slice 분해 — writing-plans로 상세화)

헌장 §7: 재설계는 단일 PR 아님. impact/effort 기준 단계:

**Phase 1 — 접힘·잘림 (high impact / low effort, 데이터 의존 없음)**
- S1: 단일 항목 접힘 제거(§5, streamModel 대칭화 + ActivityStack 임계값) + 잠그는 테스트.
- S2: 소리 없는 잘림 가시화(§7.1) + IdChip 복사.

**Phase 2 — 디자인 언어 (전 화면 톤)**
- S3: 토큰 확장(elevation·radius·type scale) + 프리미티브(EventCard/ToolLine/ToolPill).
- S4: 스트림 A+B 적용(시간축 spine + 노드 + 카드 elevation).

**Phase 3 — 그룹핑 상호작용**
- S5: FanPanel 요약 상시 + 인라인 SubTimeline 드릴 + 단일 에이전트 대칭 + 실행중/≥4 상태.

**Phase 4 — 주변 화면**
- S6: 세션 목록(슬러그·미리보기·상대시간·검색) — *데이터 의존: `/v1/sessions` 필드 추가*.
- S7: Detail 패널 소제목·provenance·Signal.
- S8: KPI sparkline/베이스라인 — *데이터 의존: turns/fingerprint*.
- S9: Analysis 재설계.

**Phase 5 — 마감**
- S10: 반응형·라이트모드·a11y·범례·키보드.

각 slice는 TDD red 우선 + 브라우저 스모크 후 commit.

---

## 9. 열린 질문 (사용자 판단 필요)
1. **브랜치 전략:** 현재 `feat/workflow-grouping`(PR #60) 위에 stack할지, MVP/별도 epic 브랜치로 뺄지. (메모리: integration line 쪼개지 말 것 → 그룹핑 위 stack이 자연스러움.)
2. **세션 목록 데이터 추가**(slug·preview·model)를 백엔드 slice로 먼저 칠지, 프론트는 가용 필드로 점진 적용할지.
3. **비용 KPI**: 측정-비용 swap 보류 유지(추정 표기) 확인.
4. **≥4 동시 에이전트** 밀집 표현은 별도 목업 후 확정.

## 10. Non-goals
- 그래프/Why Panel/Resource Drawer/episode lane(폐기 모델) 부활 금지.
- 판단 색(analytics danger red)·외부 write·CC 설정 변경 금지.
- 단일 대형 PR 금지(slice 분해).

## 11. References
- 진단: 5-에이전트 코드 매핑 + 라이브 스모크(`00fae5d9`), 2026-06-14.
- 목업: `webui/public/redesign-directions.html` · `redesign-fullscreen.html` · `batch-states.html`.
- 코드 사이트: `streamModel.ts:1131/1144/339`, `ActivityStack.tsx`, `InsightStrip.tsx`, `AnalysisPanel.tsx`, `DetailPanel.tsx`, `SessionListPage.tsx`, `AppShell.tsx`, `tokens.css`.
- 관련 스펙: epic charter §3/§5/§6/§7, subagent-parallel-batch-grouping, scaffold-message-grouping, detail-view-derived-metrics.
