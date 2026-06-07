# wimcc as LLM Analysis Substrate — Deterministic Facet Expansion

> REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use `- [ ]`.
> Source: **dogfooding 세션 (2026-06-07)** — 이전 CC 세션을 *wimcc로* 관측해, wimcc가
> LLM에게 개선점 분석 재료를 충분히 주는지 검증. 분석 대상 세션 `c8256e80`(Plan 6 작업,
> opus-4-8 단일 360턴, TDD red-first) + 4개 병렬 분석 에이전트(spec / 하네스 / 코드·테스트 / 모델·비용).

**철학 (사용자 확정):** wimcc는 판정하지 않는다. **결정론적 데이터를 최대한 만들어 LLM이
개선점을 찾게** 한다. 따라서 성공 기준은 "wimcc가 판정 로직을 갖췄나"가 아니라
**"wimcc가 내놓는 결정론적 데이터만으로 LLM이 실제 개선점을 도출할 수 있나"**.

**Verdict — 조건부 YES.** 거의 모든 개선점의 **원자료(substrate)는 이미 wimcc 안에 존재**한다
(`observed_event` 28컬럼 + `diff_hunk`/`usage_facet`/`verification_run`/`signal` 사이드테이블 +
telemetry/metric/log facet + correlation keys: tool_use_id/request_id/trace_id/turn_id). 그러나
(a) LLM이 바로 쓸 **가공 facet/집계가 카테고리마다 비어 있고**(특히 하네스·spec),
(b) **일부 완성된 지표가 오히려 오도**한다. → 풍부한 substrate를 카테고리별 facet으로 가공하고
측정 신뢰성(measured/estimated/unknown)을 분리하는 것이 핵심.

---

## 평가 근거 — 4 카테고리 × data-sufficiency

태그: ✅ wimcc 가공 지표/signal만으로 도출 · 🟡 raw event(payload) 직접 파싱 필요(데이터는
있으나 미가공) · ❌ 원자료 자체 부재.

| 카테고리 | ✅ | 🟡 | ❌ | 결론 |
|---------|----|----|----|------|
| **Spec 정합성** | 0 | 6 | 0 | GREEN 상한 없음 — wimcc는 "무엇을 바꿨나"는 알지만 docs/migrations/plans를 관측 대상에서 제외해, 모든 판정이 "변경 확정 → 외부 문서 직접 대조"의 2단 |
| **LLM 하네스** | 0(부분) | 대부분 | (이번 세션 한정 MCP·subagent 0건이라 N/A) | skill/subagent/mcp/hook/prompt-adherence가 **가공 facet 0개**. 원자료는 풍부(hook 두 소스, skill tool_call, llm_request span) |
| **코드·테스트** | 5 | 3 | 1 | **가장 성숙**. `/verification-runs`가 command_kind·status_provenance·exit_code·failure_summary·covered_diff_hunk_ids를 구조화 노출 |
| **모델·비용** | 4 | 5 | 0 | 세션 총합·baseline은 ✅, 턴별 시계열은 OTLP 존재 구간(~19%)만 복원 |

## 핵심 발견 (교차검증으로만 드러난 것)

1. **가공 지표가 LLM을 오판으로 유도** — 가장 큰 위험.
   - `verification_pass_rate = 0.036`(c8256e80, 28건 중 1)은 "테스트가 계속 깨짐"처럼 보이지만,
     원천(`/verification-runs`)은 **26건이 `status=unknown`(측정 실패)**. 원인: `cargo test 2>&1 | tail`
     파이프가 exit code를 마스킹(파이프 최종 stage가 `tail`, exit 0). 지표만 본 LLM과 원천을 본
     LLM의 결론이 **정반대**.
   - `/usage`의 `turns=360`은 사용자 turn이 아니라 assistant 산출 이벤트 수
     (message 119 + thinking 109 + tool_call 132). 실제 distinct `turn_id`는 **13**. "turn당 $0.39"는
     오해, 실제 사용자 turn당 ~$10.8.
2. **하네스 신호는 "원자료는 다 있는데 가공이 0"** — skill 호출(`tool_name=Skill`), hook 실행
   (transcript `hook_event` + OTLP `hook_execution_complete`), llm_request span이 전부 수집돼 있으나
   하네스 레이어를 정조준한 detector/facet이 0개라 전부 🟡.
3. **spec 정합성은 관측 경계 밖** — 이 세션이 만든 drift(impl-notes가 Plan 6 미반영,
   CLAUDE.md migration "0020" vs 실제 0022) 둘 다 잡혔지만, docs/migrations를 입력으로 받지 않아
   LLM이 매번 파일시스템으로 나가 대조해야 함.

## 아키텍처 — derived(on-demand) vs realtime(ingest-time)

코드 확인 결과 (정식 "derived" 명명은 없으나 실질 3층위):

| 층위 | 시점 | 저장 | 예 | 비고 |
|------|------|------|----|------|
| 파생 side-table | **ingest-time** | DB | `diff_hunk`·`usage_facet`·`verification_run` | 재ingest 시 갱신 |
| `signal`(detector) | **ingest-time, 매 배치 전체 재계산** | DB | 5 detector | `run_detectors`가 store/otel/hook ingest 끝마다 호출 → `OwnedSessionInsightData::load`(전체) → INSERT OR REPLACE |
| 집계 지표 | **on-demand(조회 시)** | 미저장 | `/metrics`·`/usage`·`/baseline` | 사용자 정의 "derived = 분석 버튼" |

**사용자 의도:** derived 구분을 둔 이유는 실시간 수집·가시화 경로의 연산 최소화. **성능 희생 없이
실시간 가능하면 그것이 최선.** → 지표 형태로 갈린다:
- **단조 카운터/합계/비율**(tool 분포, skill/subagent/mcp/hook 수, verification passed/failed/unknown,
  by_model 토큰, cache_hit) → **증분 누적이면 성능 희생 0으로 실시간** (rollup 테이블 또는 SQL COUNT/GROUP BY).
- **윈도/시퀀스**(re_read, red-green cycle, adherence) → 순수 증분 까다로움(현재 전체 재계산 이유).
- **분위수/cross-session**(baseline) → derived 유지가 정답(주기 캐시).

> 정리할 성능 부채 2건(별도): ① signal이 매 OTLP 배치 **전체 세션 재계산**(O(N)×배치),
> ② `metrics`가 전체 이벤트 메모리 로드 후 Rust 필터(`list_session(…,100_000)`, 코드 주석도
> "candidate for SQL COUNT" 인정).

---

## Task A: 측정 신뢰성 보정  [데이터 모델 변경 ≈ 0 — 기존 on-demand 집계/DTO 수정]

**근거:** 핵심 발견 #1. 정직한 "모름"이 집계에서 "나쁨"으로 왜곡되는 것을 차단.

- [ ] **A1** `/metrics`에 `verification_unknown_count` + `verification_measured_rate`(=measured÷total)
  분리. `pass_rate`가 unknown을 분모에 섞지 않도록. (verification_run에 `status`/`status_provenance`
  이미 존재 → 집계 로직만.)
- [ ] **A2** `/usage`의 `turns` → `assistant_events`로 개명하고 `user_turns`(distinct turn_id)·
  `user_messages` 별도 필드 추가. "turn당 비용"의 13배 왜곡 제거.
- [ ] **A3** per-turn `cost_basis`(measured=OTLP 구간 / estimate=나머지) 분리 — 세션 전체에 추정
  한 장으로 라벨돼 측정 $3.78 vs 추정 $140.74의 37배 괴리가 안 보이는 문제.
- [ ] **A4** (선택) `status_provenance=unknown` run에 `measurement_blocked_reason`
  (`redirect_swallowed_exit`/`piped`/`no_exit_observed`).
- [x] **(PR #40 선행, merged)** `parse_exit_code`가 CC 실제 형식 `Exit code N`(콜론 없음)을 인식 —
  measured outcome 신호가 Unknown으로 새던 것 복구(215세션/82건 실측). Task A의 *measured 커버리지*
  전제 조건. commit `e422278`.

## Task B: 하네스 facet 레이어  [detector=signal 재사용 ⇒ 스키마 0 / 영속 facet 택할 때만 작은 migration]

**근거:** 핵심 발견 #2. 사용자 1순위 관심(프롬프트/CLAUDE.md/hook/agent/mcp 개선). 원자료는 전부
존재하므로 **집계·facet화만** 추가.

- [ ] **B1** `skill_invocation` facet — `tool_call(tool_name=Skill)` + paired result + SessionStart
  `hook_additional_context` 주입 스킬 대조 → "주입됐지만 안 쓴 스킬" 지표.
- [ ] **B2** `subagent_run` facet — `tool_name=Task` / `is_sidechain=true` 묶음. 0건이면 "직렬 처리"가
  명시적으로 보이게.
- [ ] **B3** `mcp_usage` 집계 — `log_record(event_name=mcp_server_connection)` + `tool_name LIKE 'mcp__%'`.
- [ ] **B4** `hook_outcome` 집계 — transcript `hook_event`(N건) + OTLP `hook_execution_complete`를
  hook_name별 join, **blocking>0**(실제 차단)·누적 duration_ms 노출.
- [ ] **B5** `harness_directive_adherence` detector — CLAUDE.md의 검증 가능한 지시를 결정론 규칙으로
  (예: TDD red-first = test 파일 Edit → 같은 모듈 impl Edit 이전에 실패하는 cargo test가 있었나).
  evidence_refs로 묶어 evidence-linked 철학 준수. (signal 인프라 재사용 — re_read 추가가 같은 패턴.)
- [ ] **B6** `re_read` signal에 `post_compact` 플래그(compact_boundary 이후 재독 = context-loss 인과).
- [x] **(PR #40 선행, merged)** `eventTags`: Edit/Write를 `write.{code,docs,config,data}`로 분류 +
  `.output`(CC task/log)·`.py` 확장자 등록. WebUI replay의 도구 분류 가시성 보강(하네스 가시성의
  프런트 절반). commit `8b1a01e`.

## Task C: 턴별 시계열 노출  [usage_facet 행 그대로 노출 ⇒ 스키마 0]

**근거:** 모델 적정성·비용 효율 분석의 기반. 현재 OTLP 있는 ~19% 구간만 복원 가능.

- [ ] **C1** `GET /v1/sessions/:id/usage/turns` — `usage_facet`(assistant 턴당 1행)을 그대로 노출:
  `{turn_id, request_id, model, 4종 토큰, est_cost, duration_ms, ttft_ms, stop_reason, effort, source}`.
  커버리지 19%→100%.
- [ ] **C2** `turn-complexity` facet — turn당 `{tool_call_count, distinct_tools, output_tokens,
  thinking_present, duration_ms, effort}` → "단순 turn인데 opus/xhigh effort" 같은 적정성 판정 재료.
- [ ] **C3** (선택) `compact_savings` — compact_boundary 인접 요청의 cache write/read 비용 델타.

## Task D: spec 정합성 관측 확장  [새 입력 소스 + 새 레이어 — 가장 무거움]

**근거:** 핵심 발견 #3. wimcc 관측 경계를 "세션"에서 "프로젝트 사양 산출물"까지 확장. `raw_event.source_type`이
현재 transcript/otel/hook뿐 → 데이터 모델 확장 불가피.

- [ ] **D1** docs/migrations/plans 인덱싱(새 source_type or 사이드 인덱스).
- [ ] **D2** `symbol-spec-coverage` — 세션 diff-hunk의 신규 심볼(enum/struct field/DB 컬럼/manifest key)을
  docs에서 역추적해 documented/undocumented boolean.
- [ ] **D3** living-doc staleness detector — detector rule이 바뀌었는데 impl-notes가 옛 서술 유지하는
  경우를 토큰 대조로 플래그.
- [ ] **D4** `GET /v1/schema-info`(applied migrations, 최신 번호) — CLAUDE.md "0020" vs 실제 0022 류 drift.

---

## Progress (2026-06-07 기준)

**해결됨 (PR #40, merged into main — rebase linear):**
- ✅ `e422278` fix(insight): `parse_exit_code` → CC `Exit code N` 인식 (Task A measured-커버리지 전제)
- ✅ `39f5c95` style(clippy): 기존 80개 경고 0 (substrate 위생, 본 plan과 독립)
- ✅ `8b1a01e` feat(webui): Edit/Write 태그 + `.output`/`.py` (Task B 프런트 가시성 부분)

**미착수 (이 문서가 설계·계획 단계):**
- 🔲 Task A: A1~A4 (측정 신뢰성 — 가장 시급, 스키마 0)
- 🔲 Task B: B1~B6 (하네스 facet — 가장 큰 구조적 gap)
- 🔲 Task C: C1~C3 (턴별 시계열)
- 🔲 Task D: D1~D4 (spec 정합성 — 가장 무거움, 새 입력 소스)

**권장 착수 순서:** A(오판 함정 제거, 가벼움) → B(1순위 관심) → C → D. A·B·C는 기존 derived/signal
레이어 확장(스키마 ≈ 0, 영속 facet 택할 때만 `usage_facet` 패턴 복제 migration). D만 새 레이어.

## Self-Review
- 단일 세션(c8256e80) + 9~215세션 보조 표본 기반. data-sufficiency 표는 4개 분석 에이전트의 실측
  curl 결과 종합 — 표본 수가 적은 항목은 일반화 금지(특히 하네스의 MCP·subagent는 이 세션 0건이라 N/A).
- 핵심 발견 #1의 수치(26 unknown, turns 360 vs 13)는 c8256e80 실측. 다른 세션 교차검증은 Task A 구현 시 동반.
- 본 plan은 **wimcc 자체 개선**(코드 facet/endpoint 추가)이며, dogfooding 분석 대상 세션의 개선이
  아니다 — 둘을 혼동하지 말 것.
