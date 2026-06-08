# wimcc 자기개선 루프 적합성 — cross-session 평가 (설계)

> 선행: `docs/superpowers/plans/2026-06-07-llm-analysis-substrate.md`(dogfooding 발견 + Task A~D 로드맵).
> 본 문서는 **그 로드맵을 구현하기 전에**, "현 wimcc 구현이 자기개선 루프에 적합한가"를
> **증거 기반으로 판정**하는 평가 방법론을 정의한다. 산출물은 개선 patch가 아니라 **평가 보고서**.

## 1. 동기

substrate 문서는 4 카테고리(Spec 정합성 / LLM 하네스 / 코드·테스트 / 모델·비용) data-sufficiency
판정으로 "조건부 YES" verdict를 냈다. 그러나 그 판정은 **단일 세션(c8256e80) + 4 병렬 분석
에이전트** 기반이고, 문서 스스로 한계를 명시한다: 단일 세션, 하네스의 MCP·subagent는 그 세션
0건이라 N/A, 표본 적은 항목 일반화 금지.

따라서 Task A~D에 착수하기 전에 **평가의 표본 폭을 넓혀**(cross-session) verdict와 Task 우선순위가
단일 세션 편향이 아님을 잠근다. 이 평가가 끝나야 "무엇을, 어떤 형태로 고칠지"가 증거로 정해진다.

## 2. 비목표 (이 평가가 하지 않는 것)

- substrate Task A~D 구현. (평가가 우선순위를 확정한 *뒤* 별도 plan으로.)
- wimcc에 판정 로직 추가. (아래 §4 철학 참조.)
- 분석 대상 세션 자체의 개선. (대상은 **wimcc 구현의 적합성**이지 그 세션이 아니다.)

## 3. 평가 단위 — 세션 패널 × 4 카테고리 grid

각 (세션, 카테고리) 셀에서 독립적으로 data-sufficiency를 채점한다.

### 3.1 세션 패널 (~8개, 결정론적 선택)

`observed_event`를 SQL 프로파일링해(이미 실측) 단일 세션이 못 본 **모양 축**을 커버한다.
선택은 고정 SQL 기준으로 재현 가능해야 한다(임의 선택 금지).

| 축 | heavy 후보 (실측) | zero/few 후보 (실측) |
|----|------------------|---------------------|
| subagent (sidechain) | `0f1e71f6` (9882) | `1244efb0`, `ed82aee9` (0) |
| MCP (`mcp__*`) | `46aa99a7` (192), `1053583d` (146) | `14df593c`, `3a07124f` (0) |
| skill (`tool_name=Skill`) | `1b30ced8` (59), `4258d662` (40) | `aac68973` (0) |
| user-turn (distinct turn_id) | `ed82aee9` (73) | `0d52a5ae` (1) |

> 원본 control 세션 `c8256e80`은 현 DB(Jun 5)에 **없다** → 평가하려면 명시적 재ingest 필요(§6).
> 패널 확정 SQL은 plan 단계에서 고정하고, 선택된 8개의 사유를 보고서에 기록한다.

### 3.2 4 카테고리

substrate와 동일: **Spec 정합성 · LLM 하네스 · 코드·테스트 · 모델·비용**.

## 4. 채점 rubric — **두 축** (핵심: 판정/추정 금지)

> **철학 (사용자 확정, CLAUDE.md·judge 삭제 결정과 동일):** wimcc는 판정하지 않는다.
> 결정론적으로 명백히 확인되는 지표·스멜은 그대로 계산·노출하되, **정량화하기 어려운 판정을
> 판독기 로직으로 가정·가설·추정에 근거해 만들면 안 된다 — 그러면 전부 무너진다.** 판정이
> 필요하면 그 판정은 **LLM이 하도록 데이터를 제공**하는 것으로 끝낸다.

따라서 셀마다 "요구되는 신호"를 가용성만으로 보지 않고 **먼저 성질을 가른다.**

### 축 A — 신호의 성질

- **결정론적 사실 (fact):** 관측에서 객관적으로 확정되는 것. 예: tool 호출 발생 여부, exit code,
  토큰 수, sidechain 묶음, hook blocking 여부, request_id별 model, cache read/write 바이트.
- **판정 필요 (judgment):** 가치/규범 평가가 들어가는 것. 예: "이 turn이 비효율인가",
  "이 CLAUDE.md 지시를 위반했나", "주입된 skill을 안 쓴 게 잘못인가", "이 모델이 과한가".

### 축 B — 가용성

- ✅ wimcc 가공 지표/facet/집계로 바로 노출
- 🟡 raw event(payload)에 데이터는 있으나 미가공(직접 파싱 필요)
- ❌ 원자료 자체 부재

### 처방 규칙 (verdict가 권고로 바뀌는 지점)

| 성질 | 가용성 gap(🟡/❌)일 때 올바른 처방 | 안티패턴 (적발 대상) |
|------|-------------------------------|--------------------|
| **fact** | deterministic facet/집계 추가 | (없음) |
| **judgment** | **evidence 묶음**을 join+노출만. 판정은 LLM. | **점수화/추정 detector** — 가설·휴리스틱으로 judgment를 정량화하는 모든 것 |

**즉 평가의 출력엔 "어떤 신호가 부족하다"뿐 아니라 "그 신호가 fact냐 judgment냐, 따라서
deterministic facet인가 evidence-assembly인가"가 셀마다 붙는다.**

## 5. 셀당 채점 절차

각 (세션, 카테고리)에서:

1. **노출 인벤토리** — wimcc가 그 세션·카테고리에 대해 결정론적으로 내놓는 것을 Pull API +
   raw payload에서 목록화.
2. **신호 분해 + 두 축 태깅** — 카테고리가 요구하는 신호를 나열하고 각각 (fact|judgment) ×
   (✅|🟡|❌).
3. **오도 지표 cross-check** (substrate 최대 위험 #1) — 가공 지표가 결정론적 원천과 어긋나는지
   **객관 대조**. 이건 판정이 아니라 *계산값 대 그 계산의 원천* 비교라 결정론적. 단일 세션 함정
   (verification_pass_rate가 unknown을 분모에 섞음, `turns=360`이 assistant 이벤트 수)이 **이 패널에서
   재현되는 패턴인지** 단발 케이스인지 판별.

> 채점은 **데이터 존재/형태**만 판단한다. "이 세션이 비효율인가" 같은 judgment를 채점자가 내리지
> 않는다 — 그건 §4 철학 위반. 채점자는 "그 judgment를 LLM이 내릴 evidence가 조립돼 있나"만 본다.

## 6. 선행 조건 — corpus 재ingest

현 `.wimcc.sqlite`는 **Jun 5자 stale**: graph_node/graph_edge/judge_verdict_cache 테이블 잔존,
migration 0019~0022 미적용. PR #40의 measured-coverage 수정(`parse_exit_code` → `Exit code N`,
`status_provenance`)이 데이터에 반영되려면 **현 스키마(0022)로 재ingest**가 평가 전제(step 0).
control 세션 `c8256e80`을 패널에 넣으려면 그 transcript도 함께 ingest.

## 7. 집계 + verdict

- **재측정 매트릭스** — category × session, 셀마다 (fact|judgment) × (✅|🟡|❌). substrate의 단일
  표를 대체.
- **broadly-holds vs single-case 플래그** — Real-data anchoring 원칙(표본 수 명시) 준수. N/8 세션에서
  재현된 발견만 일반화.
- **오도 지표 목록** — 코퍼스에서 재현되는 것.
- **안티패턴 적발** — substrate Task 중 judgment를 detector로 정량화하려는 것(1차 의심: B5
  `harness_directive_adherence`)을 evidence-assembly 형태로 재설계 권고.
- **증거 기반 Task A~D 재랭킹** — 다음 단계(구현 plan)의 입력.

## 8. 실행 방식 (plan 단계에서 확정 — 옵션으로 남김)

grid는 (세션 × 카테고리)의 독립 셀이라 자연히 fan-out 가능. 두 옵션:

- **(a) 순차 직접** — 메인 세션에서 셀을 순서대로 채점. 토큰 적게, 느림.
- **(b) multi-agent workflow fan-out** — 세션×카테고리를 병렬 분석 에이전트로. substrate 평가가 쓴
  방식. **별도 opt-in 필요**(workflow는 명시 동의 시에만). 빠르고 표본 넓힘에 적합.

권고: 패널이 ~8 × 4 = 32 셀이라 (b)가 적합하나, opt-in 전까지는 (a)로 파일럿(1~2 세션)해 rubric을
검증한 뒤 확장.

## 9. 성공 기준

- 8개 세션 패널 전부에 대해 4 카테고리 매트릭스가 (fact|judgment) × (✅|🟡|❌)로 채워졌다.
- substrate의 단일 세션 발견 3건 각각이 "재현됨(N/8)" 또는 "단발"로 라벨됐다.
- Task A~D 각각이 (fact→facet | judgment→evidence-assembly | 안티패턴→재설계)로 분류됐다.
- 보고서의 모든 일반화 statement에 표본 수가 붙어 있다(단일 사례 일반화 0건).
