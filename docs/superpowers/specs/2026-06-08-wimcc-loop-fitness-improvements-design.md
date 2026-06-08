# wimcc 자기개선 지표 적합성 — F1~F4 개선 (설계)

> **입력:** `docs/superpowers/2026-06-08-wimcc-loop-fitness-analysis.md`(실측 분석, 82세션 재ingest +
> API probe). 그 분석이 도출한 4개 블로커(F1~F4)를 닫는 구현 스펙. 모든 수치는 그 분석의 실측값이며
> `.wimcc-analysis.sqlite`로 재현 가능.
>
> **목표 한 줄:** LLM이 wimcc 밖으로 나가거나 거짓 신호에 속지 않고, wimcc 지표만으로 4 영역의 개선점을
> 신뢰성 있게 도출할 수 있게 한다. wimcc는 판정하지 않는다 — fact는 정직하게 노출, judgment는 evidence를
> 달아 LLM에게.

## 비목표

- 자기개선 루프의 *적용*(patch 생성/auto-apply) 단계. wimcc는 read-only insight = 분석 단계에만 앉는다
  (reward-hacking 회피 구조). 이 스펙은 분석 단계 evidence의 *신뢰성*만 다룬다.
- detector에 judgment(효율/적정성/위반) 점수화 추가. 어떤 신규 신호도 fact이거나 evidence-assembly다.

## 처방 원칙 (rubric)

각 작업은 분석의 처방 분류를 따른다: **fact→정직한 facet/집계** · **judgment→evidence join+노출** ·
**휴리스틱 추정→제거/축소**. 모든 변경은 CLAUDE.md 원칙 준수: TDD red-first, schema_version+provenance,
evidence_refs, real-data anchoring(frozen fixture 또는 docs 인용), source-preserving.

우선순위: **F1 → F2 → F3 → F4** (F1·F2는 "거짓을 참으로 제시"하는 능동적 해악, F3·F4는 부재/고마찰).

---

## F1 — 집계 정직화  [fact→facet · 스키마 0, on-demand 집계/DTO 수정]

**문제(실측):** `/metrics`가 `verification_pass_rate: 0.0, passed: 0, total: 10`을 unknown 카운트 없이
노출 → "테스트 다 실패" 오판. 코퍼스 진실: 1734건 중 measured 195(11%) / unknown 1539(89%).
`/usage`의 `turns: 2683`은 실제 user turn(distinct turn_id) **43**의 62배. cost_basis는 세션 전체 단일
`estimate`. `/usage/baseline`도 turns 오도 정의를 cross-session 전파.

- **F1-1** `/metrics`에 `verification_measured_count`·`verification_unknown_count` 추가하고
  `verification_pass_rate`의 분모를 **measured-only**로 정정(현재 unknown을 분모에 섞어 0.0으로 왜곡).
  `status_provenance` 집계만으로 산출(verification_run에 이미 존재). pass_rate 정의 변경은 `meta`에
  명시.
- **F1-2** `/usage` `turns` → `assistant_events`로 개명, `user_turns`(distinct turn_id)·`user_messages`
  필드 추가. `by_model`도 동일. schema_version bump(API 계약 변경).
- **F1-3** `/usage`의 `cost_basis`를 단일 세션 라벨에서 **source별 분리**로: OTLP request_id로 측정된
  토큰은 `measured`, transcript 추정은 `estimate`. offline-only DB에선 전부 estimate지만 필드가 이를
  *정직하게* 반영해야 한다(거짓 measured 금지).
- **F1-4** `/usage/baseline`의 `turns` 백분위를 F1-2 개명에 맞춰 `user_turns` 기준으로 재정의(또는
  둘 다 노출). cross-session 오도 전파 차단.

> **TDD:** 각 항목은 `.wimcc-analysis.sqlite`의 실측값(195/1539, 2683 vs 43)을 fixture로 잠그는 실패
> 테스트 우선. "unknown이 분모에서 빠졌다", "user_turns가 distinct turn_id와 같다"를 assert.

## F2 — verification 탐지 정밀화  [휴리스틱→제거/축소 · 탐지 로직 수정]

**문제(실측):** `classify_segment` Tier-2(`detection_basis="test_keyword"`)는 known_tool allowlist를
못 맞춘 세그먼트가 `test`/`spec` 토큰을 가지면 verification_run으로 추정. multi-line Bash(commit
메시지·heredoc·echo)에서 split된 **산문 줄**이 오탐: `"- SA1 ... Airflux test"`,
`"- CI 회복: scripts/run-tests.mjs 신설"`, `"declare the contract at spec §1.9"`. test_keyword 454건(26%)
중 불릿-산문 38·Hangul 18·>200자 29건이 phantom. `src/insight/verification_allowlist.rs:282` 참조.

- **F2-1** 세그먼트 추출에서 **인용 문자열/heredoc 본문/`-m` 메시지 인자를 별도 명령 세그먼트로
  split하지 않는다.** 산문이 애초에 세그먼트가 되지 않게 — 근본 원인 차단.
- **F2-2** Tier-2 keyword fallback 유지 여부 결정(plan에서 frozen fixture로 측정 후 택일):
  - **(a) 제거** — known_tool(결정론 allowlist)만 verification_run. `make spec`·`./run_integration_test.sh`
    류 비-allowlist 실 러너를 놓치나, 추정 0.
  - **(b) 축소** — lead 토큰이 plausible executable(allowlist 밖이라도 path/binary 형태)일 때만 Tier-2
    허용. 산문 lead(`-`, 빈 토큰, 다어절 자연어)는 deny.
  - 판정 기준: **frozen 오탐 fixture(위 3종)에서 phantom run이 0이 되고, 실 러너(`node scripts/smoke-test.mjs`,
    `make spec`) 회귀가 없을 것.** real-data anchoring으로 잠근다.
- **F2-3** (선택) `verification_run`에 measured 불가 사유를 이미 status_basis(`piped`)로 일부 노출 —
  F1-1과 함께 LLM이 "측정 실패 vs 진짜 실패"를 구분하도록 status_provenance를 집계에 노출(F1과 합류).

> **TDD:** 오염 명령 3종을 `tests/fixtures/.../real/`에 동결하고 "verification_run으로 분류되지 않음"을
> assert하는 실패 테스트 우선. 기존 `classify_segment_tier2_*` 테스트의 실 러너 케이스는 green 유지.

## F3 — 하네스 facet 레이어  [fact→facet · signal 인프라 재사용 + events 필터]

**문제(실측):** skill(`tool_name=Skill`)·subagent(`is_sidechain`)·mcp(`tool_name LIKE 'mcp__%'`)·
hook(`hook_event`) 원자료는 `observed_event`에 전부 있으나 facet/endpoint 0개. `/events`는
`before/after/limit/tool_use_id/request_id`만 받아 tool_name/kind 필터 불가 → 하네스 분석에 세션 전
이벤트(최대 10935건) 클라 필터. 추가: **`tool_name='Task'` 전 코퍼스 0건** — subagent를 도구명으로
식별 불가, `is_sidechain`에만 의존.

- **F3-1** `/v1/sessions/:id/events`에 `kind`·`tool_name` 필터 쿼리 추가(서버측 SQL 필터). 하네스
  원자료를 raw 파싱하더라도 실현 가능하게. 단조 추가라 스키마 0.
- **F3-2** `harness_facet` 집계 endpoint — 세션당 결정론 카운트: skill 호출 수(+호출된 skill 목록),
  subagent run 수(`is_sidechain` 묶음, Task 식별 불가 사실도 명시), mcp 호출 수(+server별), hook 실행 수
  (+blocking 여부 + 누적 duration). **모두 fact 카운트** — "비효율" 판정 없음.
- **F3-3** judgment 보조용 **evidence-assembly**(점수화 아님): "주입됐으나 안 쓴 skill"은
  SessionStart `hook_additional_context` 주입 skill 목록 ↔ 실제 `tool_name=Skill` 호출의 **집합 차이를
  fact로** 노출하고, "그래서 비효율인가"는 LLM이 판정. evidence_refs로 양쪽 event를 묶는다.
- **F3-4** (관측 사실) `tool_name='Task'`가 채워지지 않는 원인 규명 — subagent를 도구명으로 식별 가능하게
  파서 보강할지, `is_sidechain`만으로 충분한지 결정. real-fixture로 Task tool_call payload 형태 확인 후.

> **TDD:** 패널 세션 실측 카운트(예: 1b30ced8 skill 59, sidechain 5235)를 fixture로 잠그는 테스트 우선.

## F4 — Spec 정합성  [대부분 judgment → 정량 지표 대상 아님 · 경량]

**정정된 사실(실측):** CLAUDE.md는 CC 동작 원리상 매 턴 시스템 프롬프트로 컨텍스트에 들어가지만,
**transcript JSONL엔 기록되지 않는다.** 이 세션을 throwaway DB로 재ingest해 확인: `"project
instructions, checked into"` 매치 6건은 전부 조사 노이즈(assistant_message 1 + tool_call 3 + tool_result
2)였고 **실제 `# claudeMd` 주입 블록은 0건**. wimcc는 transcript를 분석하므로 CLAUDE.md *텍스트*를
(에이전트가 Read하지 않는 한) 직접 보지 못하고, 그에 대한 **행동·효과**만 본다.

→ 원래 F4-1("새 source_type doc ingestion")은 **과설계**(CLAUDE.md는 cwd 파일 1회 읽기로 충분, 별도
파이프라인 불필요). 원래 F4-3(staleness/coverage detector)은 **judgment를 휴리스틱으로 추정하는 철학
위반**이라 폐기.

**원칙:** CLAUDE.md 준수는 **대부분 judgment이며 정량 지표 대상이 아니다.** wimcc는 (a) wimcc 자기
상태로 결정되는 drift만 fact로 내고, (b) 기계적으로 확인 가능한 지시의 *행동 evidence*만 fact+evidence_refs로
노출하며, (c) 그 외 준수 판정은 LLM에게 맡기거나 정량 범위 밖으로 둔다.

- **F4-1 (drift fact · 가벼움, 먼저)** `GET /v1/schema-info` — applied migrations + 최신 번호. 문서/에이전트가
  주장한 번호와 wimcc 실제 상태의 불일치를 **fact**로(예: CLAUDE.md "0020" vs 실제 0022). **wimcc 자기
  상태만으로 산출 — doc ingestion 불필요.**
- **F4-2 (CLAUDE.md 텍스트 · 필요 시 경량)** drift/지시 대조에 CLAUDE.md 텍스트가 필요하면 **cwd의
  `CLAUDE.md`를 ingest 시 1회 읽어** 세션에 첨부. 새 source_type/파이프라인이 아니라 cwd 파일 읽기
  (cwd는 이미 관측됨). source-preserving.
- **F4-3 (기계적 지시 → 행동 evidence-assembly)** 기계적으로 확인 가능한 지시(예: TDD red-first =
  동일 모듈 impl Edit 이전에 실패하는 test 실행이 있었나)는 **행동 시퀀스를 fact + evidence_refs로 묶어**
  노출. "준수했나/충분한가" 판정은 LLM. **점수화 detector 금지.**
- **(범위 밖)** 기계적으로도 확인 불가한 서술적 지시는 **정량 지표 대상이 아니다.** 거짓 정량화 대신
  LLM이 raw evidence 위에서 판정하는 다른 접근을 따른다 — 이 스펙은 그것을 지표화하지 않는다.

---

## 실행 순서·검증

1. F1(스키마 0) → 2. F2(탐지 정밀, frozen fixture) → 3. F3(facet + events 필터) → 4. F4(전부 경량 — F4-1 schema-info 먼저).
- 각 단계 후 `.wimcc-analysis.sqlite` 재ingest 또는 신규 fixture로 회귀 확인.
- UI 변경 동반 시 브라우저 smoke(CLAUDE.md 의무).
- 매 단계 self-check 체크리스트(CLAUDE.md) 통과.

## 성공 기준

- F1: `/metrics`·`/usage`가 measured/unknown·assistant_events/user_turns를 분리 노출, pass_rate 분모에
  unknown 미포함. 실측 fixture(195/1539, 2683 vs 43)로 잠김.
- F2: frozen 오탐 fixture 3종이 verification_run으로 분류되지 않고, 실 러너 회귀 0.
- F3: 하네스 4종 fact 카운트 + events kind/tool_name 필터 노출. "주입됐으나 안 쓴 skill" 집합 차이 fact화.
- F4: schema-info가 applied migration drift를 fact로 노출. CLAUDE.md 텍스트는 cwd 파일 읽기(필요 시).
  준수 판정은 어떤 detector로도 점수화하지 않는다 — 기계적 지시는 행동 evidence-assembly, 나머지는 범위 밖.
- 전반: 어떤 신규 신호도 judgment 점수화가 아니다(fact 또는 evidence-assembly). 모든 일반화에 표본 수.
