# wimcc 자기개선 지표 적합성 — 실측 분석

**질문:** 코딩 에이전트가 워크플로우를 스스로 개선하는 데 wimcc가 내놓는 데이터로 목적을 달성할 수 있나?

**방법:** 전체 transcript 코퍼스를 fresh schema 0022 DB로 재ingest(82세션, PR #40 바이너리, parse_errors 0)
후 `wimcc serve` 기동, 모양별 패널 세션에 9개 Pull API endpoint를 probe. 검증된 rubric — 신호의 성질
(결정론적 **fact** vs 판정 필요 **judgment**) × 가용성(✅가공/🟡raw/❌부재) — 으로 채점.

> **한계(단일 조건):** offline `ingest --all`은 transcript만 처리한다. OTLP(trace/metric/log)는 live
> receiver 경로라 이 DB엔 없다. 따라서 모델·비용의 *measured* 커버리지와 trace-correlated 하네스 신호
> (`hook_execution_complete` 등)는 여기서 0이며, live 수집 시 개선될 수 있다 — 아래 ❌ 중 일부는
> "이 조건에서 부재"이지 "구조적 부재"가 아님을 구분해 표기.

---

## Verdict — 조건부 적합 (분석 단계 substrate로서)

wimcc의 **원자료 + signal/detector 아키텍처는 건전하고 프론티어 모범사례와 정합**한다(deep-research로
확인: Anthropic code/model grader 분리, evidence-linked finding, rule 투명성). 그러나 현재 **LLM이
wimcc 밖으로 나가지 않고 신뢰성 있게 개선점을 도출하기엔 4개의 블로커**가 있다. 모두 rubric의 처방
규칙(fact→facet / judgment→evidence-assembly / 휴리스틱→제거)으로 분류된다.

| 영역 | 충분성 | 핵심 근거(실측) |
|------|--------|----------------|
| Spec 정합성 | **불가** | spec 산출물(CLAUDE.md/migrations/docs)이 관측 입력이 아님 → 대부분 ❌ |
| LLM 하네스 | **부분(고마찰)** | 원자료 전부 존재하나 facet 0개 + `/events`가 tool_name/kind 필터 불가 → 전부 🟡 |
| 코드·테스트 | **부분(가장 성숙, 단 신뢰성 2결함)** | per-run 정직(✅)하나 집계 오도 + 휴리스틱 오탐 |
| 모델·비용 | **부분** | 세션 총합 ✅, turn별·measured/estimate 분리 ❌ |

---

## Cross-cutting 결정적 발견

### F1. 오도 집계가 API 표면에 그대로 노출 — 코퍼스 전반 재현 (judgment 함정)

- **`/metrics`**: `verification_pass_rate: 0.0, passed: 0, total: 10` — **`unknown` 카운트 필드 없음.**
  LLM은 "검증 다 실패"로 오판. 코퍼스 진실: verification_run 1734건 중 **measured는 195건(11%),
  unknown 1539건(89%)**. PR #40 수정이 들어간 바이너리에서도 그렇다.
- **`/usage`**: `turns: 2683` (1b30ced8) — distinct turn_id 진실값은 **43** (62배 부풀림). assistant
  usage 이벤트 수를 turns로 노출. `/usage/baseline`의 turns 백분위(median 203)도 같은 오도 정의를
  cross-session 전파.
- **`cost_basis`**: 세션 전체 단일 `"estimate_public_pricing"` — measured/estimate 분리 없음.

→ 이 수치들은 *결정론적으로 계산은 되지만 의미가 왜곡*된 fact다. 처방: 분모에서 unknown 분리
(`verification_unknown_count`), turns 개명(`assistant_events` vs `user_turns`), cost_basis turn별 분리.
**(fact→facet)**

### F2. verification 탐지의 휴리스틱 오탐 — 산문 오염 (사용자 철학 위반)

`detection_basis='test_keyword'` 454건(26%) 중 다수가 셸 명령이 아니라 **'test'·'CI' 키워드를 가진
산문**: `command="- SA1 Metica activation was previously gated on completion of Airflux test"`,
`"- CI 회복: scripts/run-tests.mjs 신설"`, `"declare the contract at spec §1.9..."`. 정량:
불릿 시작 38건, Hangul 포함 18건, >200자 29건 — 명백한 phantom verification_run. `known_tool` basis
1280건(74%)은 건전(실제 도구 호출). **단 known_tool도 1098/1280=86%가 unknown** — unknown-masking은
정탐에서도 실재.

→ 키워드로 산문을 "테스트 명령"으로 추정 = "휴리스틱으로 판정하지 말라"의 위반.
처방: test_keyword 탐지를 제거하거나 known_tool로 축소(결정론적 도구 호출만). **(휴리스틱→제거)**

### F3. 하네스 신호는 facet 0 + events 비필터 — 데이터는 있으나 사용 불가에 가까움

skill(`tool_name=Skill`)·subagent(`is_sidechain`)·mcp(`tool_name LIKE 'mcp__%'`)·hook(`hook_event`)
원자료는 전부 `observed_event`에 있으나 **전용 facet/endpoint 0개**. 게다가 `/events`는
`before/after/limit/tool_use_id/request_id`만 받고 **tool_name/kind/actor 필터 불가** → 하네스 분석엔
세션 전 이벤트(최대 10935건)를 받아 클라이언트 필터해야 함. 추가 발견: **`tool_name='Task'`가 전
코퍼스 0건**(sidechain은 수천)이라 subagent를 도구명으로 식별 불가, `is_sidechain`에만 의존.

→ 전부 fact인데 가용성 🟡(고마찰). 처방: skill_invocation/subagent_run/mcp_usage/hook_outcome 집계
facet + events kind/tool_name 필터. **(fact→facet)**

### F4. Spec 산출물이 관측 경계 밖

CLAUDE.md/AGENTS.md/migrations/docs가 ingest 입력이 아니다(transcript/OTLP/hook만). drift(CLAUDE.md
"0020" vs 실제 0022 — 본 분석에서도 확인)·지시 준수 판정에 필요한 대조 대상이 wimcc 안에 없어 LLM이
매번 파일시스템으로 나가야 한다.

→ fact("지시 X 존재", "심볼 Y documented")인데 ❌. 처방: spec 산출물 인덱싱(새 source_type).
**(fact→facet, 새 입력 소스)**

---

## 강한 긍정 — 갈아엎지 말 것

- **`/detectors` 매니페스트**: detector마다 `intent/inputs/rule/output/config_keys/rationale`. `tool_failure`
  intent에 *"is_error는 도구 실행 여부만 나타내며 pass/fail 판정에 미사용"*이라고 **결정론 경계를 명시** —
  연구가 확인한 프론티어 rule-투명성과 정합.
- **`/signals`**: 각 signal이 `summary` + **`evidence_refs`(event ID)** + `facts`(구조화 fact) +
  `provenance`(detector@version). 이것이 연구가 검증한 **"fact는 노출, 판정은 evidence 달아 LLM에게"**
  패턴 그 자체. wimcc의 signal/detector 설계는 프론티어 모범사례와 정합.
- **`/verification-runs`**: 집계(/metrics)는 오도하지만 **per-run은 정직** — `status_provenance`
  (measured|unknown), `status_basis`, `failure_summary`, `covered_diff_hunk_ids`. drill-down하면 진실
  복구 가능.

---

## 함의 (스펙으로 가기 전)

1. **아키텍처가 아니라 표면 가공·탐지 정밀도가 문제다.** signal/detector/evidence 기반은 프론티어
   정합 — 재설계 불필요. 막힌 건 (a) 집계 정직성, (b) 탐지 휴리스틱, (c) 하네스 facet 부재,
   (d) spec 미관측.
2. **reward-hacking 회피 구조 확인:** wimcc는 read-only insight(patch·auto-apply 없음)라 자기개선
   루프의 *분석 단계*에만 앉는다. 따라서 적합성 질문은 "분석 단계 evidence가 신뢰 가능한가"로 한정되고,
   F1·F2가 바로 그 신뢰성을 깬다(거짓 신호를 LLM이 사실로 받음).
3. 우선순위(실측 근거 순): **F1 오도 집계 정직화 → F2 휴리스틱 오탐 제거 → F3 하네스 facet →
   F4 spec 관측**. F1·F2는 "거짓을 참으로 제시"하는 능동적 해악이라 최우선. F3·F4는 "데이터 부재/고마찰".
