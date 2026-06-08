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

우선순위(build 과제): **F1 → F2 → F3** (F1·F2는 "거짓을 참으로 제시"하는 능동적 해악, F3은 부재/고마찰).
**F4는 build 과제가 아니라 determination** — Spec 정합성에 결정론적 정량 지표가 존재하지 않음을 확정하고
범위 밖으로 둔다(아래 F4).

---

## F1 — 집계 정직화: rate scalar 제거, 합성 가능한 count만  [스키마 0, on-demand 집계/DTO 수정]

**원칙(사용자 확정):** **rate는 window를 박아넣은 파생값이며 합성되지 않는다(don't compose).** 분석마다
구간(세션 전체 / per-turn / compact 전후 / 시간구간)이 다른데 window-고정 scalar는 그 한 분석에만 맞다.
또 세션별 rate를 평균해도 코퍼스 rate가 안 나온다(크기 다름) — 올바른 합성은 분자·분모를 따로 더하는
것. → substrate는 **(a) raw per-event fact**(이미 `/verification-runs`가 run별 status·provenance·
started_at·trigger_event_id 노출)와 **(b) 합성 가능한 count**만 낸다. **rate는 소비자가 자기 window에서
계산한다. wimcc는 어떤 window-고정 rate scalar도 내지 않는다.**

**문제(실측):** `/metrics`의 `verification_pass_rate: 0.0`(passed 0/total 10)은 unknown 1539/1734(89%)를
분모에 섞어 "테스트 다 실패"로 오도. `/usage`의 `turns: 2683`은 실제 user turn **43**의 62배(naming 거짓).
`cache_hit_ratio`·`tool_failure_rate`도 같은 window-고정 rate.

- **F1-1 (rate scalar 제거)** `/metrics`에서 `verification_pass_rate`·`tool_failure_rate`·`cache_hit_ratio`
  **삭제**. 합성 가능한 count/component만 남김: `tool_call_total`, `tool_failure_count`,
  `context_bloat_count`, 그리고 verification은 **`verification_passed`/`verification_failed`/
  `verification_unknown`/`verification_total`**(status_provenance로 산출 — measured=passed+failed,
  unknown 분리). cache는 ratio 대신 토큰 component를 노출하거나(이미 `/usage`에 있음) /metrics에서 제거.
- **F1-2 (`/usage`도 동일 원칙)** `cache_hit_ratio` 삭제(토큰 component는 이미 노출 — 소비자가 계산).
  `turns` → `assistant_events`로 개명(거짓 naming 교정) + `user_turns`(distinct turn_id) count 추가.
  `billed_tokens`·`estimated_cost_usd`는 **합(sum)이라 합성 가능 → 유지**(단 `cost_basis` provenance 라벨
  유지). `by_model`도 동일.
- **F1-3 (raw windowing enabler, 선택)** `/verification-runs`는 이미 raw canonical. per-turn/compact-window
  슬라이싱을 join 없이 쉽게 하려면 run에 `turn_id` 부가(현재 `trigger_event_id`→observed_event join으로도
  가능). 채택은 B6/F5 필요 시.
- **F1-4 (baseline)** `/usage/baseline`은 cross-session인데 **per-session rate의 분위수**(예: cache_hit
  ratio 분위수)는 "rate 분포"라는 별개의 기술통계라 유지 가능하나, `turns` 분위수는 F1-2 개명(`assistant_events`
  /`user_turns`)에 맞춰 정정. 합성형 통합 지표가 필요하면 per-session **count**를 합산해 계산(rate 평균 금지).

> **blast radius:** rate 삭제·turns 개명은 `SessionMetrics`·`SessionUsageDto`·`ModelUsageDto`·
> `repo_usage_facet` aggregate·baseline·**WebUI 소비자**까지. UI 변경은 브라우저 smoke 의무(CLAUDE.md).
>
> **TDD:** `.wimcc-analysis.sqlite` 실측(195 measured/1539 unknown, 1b30ced8의 2683 assistant vs 43 turn)을
> fixture로. "rate scalar 부재", "verification_unknown=total-measured", "user_turns=distinct turn_id"를 assert.

## F2 — verification 탐지 정밀화  [휴리스틱→제거 · Tier-2 keyword fallback 삭제]

**문제(실측):** `classify_segment` Tier-2(`detection_basis="test_keyword"`)는 known_tool allowlist를
못 맞춘 세그먼트가 `test`/`spec` 토큰을 가지면 verification_run으로 추정. multi-line Bash(commit
메시지·heredoc·echo)에서 split된 **산문 줄**이 오탐: `"- SA1 ... Airflux test"`,
`"- CI 회복: scripts/run-tests.mjs 신설"`, `"declare the contract at spec §1.9"`. test_keyword 454건(26%)
중 불릿-산문 38·Hangul 18·>200자 29건이 phantom. `src/insight/verification_allowlist.rs:282` 참조.

**결정(데이터 기반): Tier-2 keyword fallback을 완전히 제거하고 known_tool(결정론 allowlist)만 남긴다.**
근거 — 프로즈 오탐은 **전부 Tier-2**다(프로즈는 Tier-1 allowlist에 매칭 불가). 실측 detection_basis 분포:
known_tool 1280건(measured 182), test_keyword 454건(measured 13 = passed 1 + failed 12, 나머지 441 unknown).
따라서 Tier-2 제거 시 **measured 신호의 93.3%(182/195) 유지** + 프로즈/heredoc/quote 오탐 클래스 전체 소거.
Tier-2는 본질적으로 "이 텍스트에 test 단어가 있으니 테스트일 것"이라는 휴리스틱 추정 — 사용자 철학 위반의
정확한 사례라 제거가 정합.

- **F2-1** `classify_segment`에서 Tier-2 keyword fallback 블록 제거(`src/insight/verification_allowlist.rs`
  ~295–317). detection_basis는 `known_tool` 하나만 남음. `test_suite_other`/`test_keyword`를 기대하던
  기존 테스트는 None 기대로 갱신.
- **F2-2 (trade-off, 정직)** 비-allowlist 실 러너(`make spec`, `node scripts/smoke-test.mjs`,
  `./run_integration_test.sh`)는 더 이상 잡히지 않는다. **거짓 phantom보다 일부 누락이 낫다**는 판단.
  특정 실 러너가 필요하면 allowlist를 **결정론적으로** 확장(키워드 추정 아님).

> **TDD:** 오염 명령 3종(`"- CI 회복: scripts/run-tests.mjs 신설"`, `"- SA1 … Airflux test"`,
> `"declare the contract at spec §1.9 …"`)을 `classify_segment` → `None`으로 잠그는 실패 테스트 우선.
> known_tool 회귀(`cargo test`, `npx vitest run`)는 green 유지. 기존 `classify_segment_tier2_*` 테스트는
> None 기대로 수정.

## F5 — verification outcome 성공 탐지 (high-unknown 근본원인)  [파싱 결함 수정 · 우선순위 상]

**근본원인(systematic-debugging으로 확정):** verification status가 89% unknown인 것은 "신호 부재"가
아니라 **"성공 신호를 안 읽음"**이다. `resolve_outcome`(`src/insight/outcome.rs`)와 Tier-4 fallback
(`src/ingest/verification_run.rs`)은 **실패 신호만** 본다 — OTLP `success`(offline 부재), hook
`exit_code`(offline 부재), tool_result `Exit code N`(**CC는 비정상 종료에만 prepend**), `looks_like_failure`
content 패턴. **성공 탐지 경로가 0개.** 성공한 cargo/vitest는 exit code 라인을 안 남기므로, 출력에
`test result: ok. 42 passed; 0 failed` 같은 결정론 성공 요약이 있어도 Unknown으로 떨어진다.

**증거(실측):** unknown known_tool 1098건 중 — 성공 마커(`test result: ok` 373 + `passed` w/o fail 265)
= **638(58%)**, `Exit code` 라인 2건, content 빈 것 0건. 대부분이 "안 읽힌 성공"이다.

- **F5-1** `looks_like_failure`와 대칭인 `looks_like_success(content)` 추가(도구 결정론 성공 요약):
  cargo `test result: ok`, pytest `=N passed`/` passed in `, vitest/jest `Test Files`+`passed`. Tier-4에서
  `Unknown && is_verification_kind && !looks_like_failure(c) && looks_like_success(c)` → **Passed,
  provenance=`Estimated`**(measured=exit code와 구분). 이미 실패를 `Estimated`로 판정하므로 대칭·일관.
- **F5-2 (정직)** 출력 요약 기반이라 provenance는 `estimated`(measured 아님). 거짓 추정이 아니라
  도구 자체 성공 요약을 읽는 것이며, 잘못된 unknown으로 지표를 무력화하는 것보다 정직하고 유용하다.

**효과:** measured+estimated 커버리지 11%→~60%+, unknown 89%→~30%대. F1(정직한 count)이 비로소
"양질 지표"가 되는 전제. **순서: F2(phantom 제거) → F5(unknown 실질 감소) → F1(정직 count 노출).**

> **TDD:** `.wimcc-analysis.sqlite`의 실 성공 출력(cargo `test result: ok`, vitest `Test Files … passed`)을
> fixture로 `resolve`+Tier-4 → `Passed/Estimated` 잠금. 혼합(`1 failed, 41 passed`)은 `!looks_like_failure`
> 가드로 Passed 아님 확인. 재ingest 후 unknown 비율 급감을 실측 회귀로.

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

## F4 — Spec 정합성: 정량 지표가 존재하지 않음 → wimcc 지표 범위 밖 (determination, build 과제 아님)

**질문(사용자 확정):** wimcc는 claude.md를 읽을 필요가 없다. 진짜 질문은 — **claude.md/agents.md
(이니셜 프롬프트 = 행동 spec)를 정량평가할 지표가 존재하는가.**

**분석:** "이니셜 프롬프트가 효율/효과적인가"를 재려는 모든 후보 지표를 분해하면, 예외 없이 (a) 지시
텍스트를 읽어야 하거나, (b) 귀속·유사도·"교정인가" 같은 **판정 단계**를 요구한다.

| 후보 | 재려는 것 | 왜 결정론 지표가 못 되나 |
|------|----------|------------------------|
| 무시된 지시 | 지시 위반 | 지시 텍스트 필요 + 준수=대부분 judgment(소수 기계적 지시만 spot-check) |
| 중복 지시 | 안 쓰인 지시 | "필요했나"를 결정론으로 못 잼 |
| 누락 지시 | 막을 수 있던 실수 | "실수→지시 누락" 귀속이 inference |
| 프롬프트 bloat | 크기 대비 효용 | 크기만 fact, 효용은 judgment |
| 재지시 빈도 | 사용자 교정 반복 | "교정"·유사도 판정이 judgment |

> **부수 사실(실측):** 어차피 CLAUDE.md 주입은 transcript JSONL에 기록되지도 않는다(이 세션 throwaway
> 재ingest로 확인 — `# claudeMd` 주입 블록 0건). wimcc는 행동·효과만 본다.

→ **claude.md/agents.md 품질을 직접 재는 결정론적 정량 지표는 존재하지 않는다. 본질적으로 judgment
영역이다.**

**결론 (사용자 framework: "정량 평가 어려우면 정량 대상 아님"):** Spec 정합성은 **wimcc의 정량 지표
범위 밖**이다.

- wimcc는 spec-품질 metric도, spec-conformance detector도 만들지 않는다(휴리스틱 추정 = 철학 위반).
  **F4는 wimcc 코드 build 과제가 아니다.**
- wimcc의 기여는 **행동 evidence substrate** — 어떤 도구를 어떤 순서로, 재시도/재독, commit 전
  verification 여부 등 **F2·F3가 이미 내는 결정론 fact**. spec 전용 신규 작업 없음.
- **판정은 LLM 소비자가 한다.** LLM은 claude.md/agents.md를 *이미 자기 컨텍스트에 보유*하므로 wimcc가
  그 텍스트를 읽거나 저장할 필요가 없다. LLM이 wimcc의 행동 evidence ↔ 자기 컨텍스트의 지시를 대조해
  준수/효율을 판정한다(프론티어 패턴: deterministic evidence → LLM judge).
- **(선택 · spec-metric 아님)** `GET /v1/schema-info`(applied migrations 등 wimcc 자기 상태 fact)는 값싸게
  노출할 수 있으나, 이는 "claude.md 평가 지표"가 아니라 LLM이 임의 주장과 대조할 수 있는 일반 fact일
  뿐이다. 채택 여부는 F1~F3와 독립.

### F4 부록 — claude.md 개선 루프는 결정론적으로 가능한가

"전부 결정론"이 아니라 **"결정론적으로 *게이트*"**가 프론티어 정답(deep-research). 루프를 분해하면:

- **탐지**(무엇이 잘못됐나): 기계적 지시만 결정론, 산문 원칙은 judgment.
- **제안**(무엇을 고칠까): **환원 불가능한 LLM 판정** → 완전 결정론 루프는 일반적으로 불가능.
- **검증**(나아졌나): 조건부 결정론 — ① 결정론적 *행동 outcome 지표*가 존재하고(예: commit 전
  verification_run 여부, TDD red-first 시퀀스, 특정 re-read 소멸) ② held-out/반복 실행으로 비율을 old vs
  new로 비교할 때. 코드 루프는 outcome이 싸고 날카롭지만(test fail→pass), claude.md는 outcome이 확률적
  미래 행동이라 N회 실행 필요 + 지표가 결정론 행동 fact여야 함.

→ **결론:** 완전 결정론 claude.md 루프는 불가(제안=판정). **결정론적으로 게이트된 루프는 기계적
지시 부분집합에서만 가능** — 거기서 wimcc가 위반-탐지 fact + **cross-session 행동 outcome 델타**(예:
"변경 C 이후 commit-전-test 비율 X→Y")를 결정론으로 공급한다. 산문 원칙은 outcome조차 judgment라
LLM 판정 + 사람 리뷰로 남는다.

> **헌장 제약:** wimcc는 이 루프를 닫지 않는다(Non-goal: Claude Code 설정/메모리 변경·patch 생성 금지).
> wimcc는 결정론적 evidence·outcome-델타 *측정*만 공급하고, 제안·적용은 wimcc 밖(LLM 제안 + 사람 게이트).

---

## 실행 순서·검증

1. F1(스키마 0) → 2. F2(탐지 정밀, frozen fixture) → 3. F3(facet + events 필터). **F4는 build 없음**
   (determination; `schema-info`는 선택적 부가 fact).
- 각 단계 후 `.wimcc-analysis.sqlite` 재ingest 또는 신규 fixture로 회귀 확인.
- UI 변경 동반 시 브라우저 smoke(CLAUDE.md 의무).
- 매 단계 self-check 체크리스트(CLAUDE.md) 통과.

## 성공 기준

- F1: `/metrics`·`/usage`에서 window-고정 rate scalar(pass_rate·tool_failure_rate·cache_hit_ratio) 제거,
  합성 가능한 count(verification passed/failed/unknown/total)만 노출. `turns`→`assistant_events`+`user_turns`.
  실측 fixture(195 measured/1539 unknown, 2683 vs 43)로 잠김. WebUI 소비자 갱신 + 브라우저 smoke.
- F2: Tier-2 제거. 프로즈 fixture 3종 → `None`, known_tool 러너(`cargo test`·`npx vitest run`) 회귀 0.
  measured 신호 93.3% 유지(182/195), test_keyword phantom 클래스 소거.
- F3: 하네스 4종 fact 카운트 + events kind/tool_name 필터 노출. "주입됐으나 안 쓴 skill" 집합 차이 fact화.
- F4: Spec 정합성에 결정론적 정량 지표가 **존재하지 않음**을 확정(determination). wimcc는 spec-metric/
  detector를 만들지 않고, 판정은 LLM(claude.md를 자기 컨텍스트에 보유)에게 둔다. `schema-info`는 선택적
  일반 fact일 뿐 spec-metric 아님.
- 전반: 어떤 신규 신호도 judgment 점수화가 아니다(fact 또는 evidence-assembly). 모든 일반화에 표본 수.
