# 인사이트 표면 재설계 — KPI 타일에서 "5개 질문 효율 진단"으로 (한글 읽기본)

> **이 문서는 읽기용 한글본입니다.** 구현 지시(원천 사양)는 영문본 `2026-05-30-witmcc-insight-surface-redesign-design.md`이며 그쪽이 정본입니다. 내용 불일치 시 영문본이 우선합니다.

**날짜:** 2026-05-30
**상태:** 설계 사양 — 2026-05-30 승인; §11 결정 확정; 구현 계획(writing-plans)으로 진행.
**대체(UI 한정):** 현재 `KpiStrip`(6 타일: outcome · verification · episodes · risk · cost · latency, PR-3). `2026-05-29-witmcc-ux-redesign-v2-design.md`의 대화 스트림·디테일 패널·타임라인은 그대로 유지. 본 사양은 **상단 인사이트 표면**(KPI strip + 하단 색깔 episode 바)과 그것을 채우는 **수집**만 재설계.
**Charter:** `2026-05-27-witmcc-ux-redesign-epic.md`. no-inheritance 목록과 프로젝트 non-goals(개선 patch 없음, annotation/correction 모델 없음, read-only, local-first) 준수.

---

## 1. 문제 정의 (사용자 피드백, 2026-05-30)

현재 KPI strip이 발단이었다. 하나씩 짚어보니 세 부류의 결함이 드러났고, 모두 실데이터(세션 `653ea169`, 바로 이 작업 세션)로 확인했다:

1. **오해를 부르는 과도한 추상.** "Risk: N"은 단일 카운트다. `653ea169`에서는 **1953**으로 표시되는데 — 그중 1941개(99%)는 워크플로우 서브에이전트의 내부 `StructuredOutput` 스키마 재시도 사이클이고, 거기에 양성(benign) `Read`/`grep` 비정상 종료까지 섞여 있다. 실제 사용자가 본 도구 실패는 **~28**건. "사실 대부분 노이즈"라고 해명해야 하는 숫자는 지표가 아니다.
2. **고장났거나 미완인 수집.** 이 세션은 TDD를 많이 했는데도 verification이 **0%**로 나온다. 닫힌 16-패턴 allowlist + `normalise_command`(첫 `&&`에서 잘라 앞의 `cd`만 남김)가 `cd webui && npx vitest run`이나 `npx tsc -b`를 못 보기 때문. `cost`·`latency`는 미연결 placeholder.
3. **잘못된 인사이트 단위.** raw 카운트("Episodes: ~1900")는 바로 아래 phase 바와 중복이고 덜 정직하다. 그리고 사용자는 사실 점수를 원하지 않는다 — 진단 질문의 답을 원한다.

이어서 사용자가 도구의 **진짜 목적** — 답해야 할 다섯 질문 — 을 명시했다:

- **Q1 — 효율.** AI 에이전트를 얼마나 효율적으로 쓰고 있나? 비효율은 어디서 발생하고, 무엇을 고치면 되나?
- **Q2 — 비용/토큰 낭비.** 낭비가 어디서 발생하고 *무엇이 유발*하나? 높은 모델? 과도한 입력? 대량 도구 호출? 서브에이전트 과다?
- **Q3 — 시간.** 해결에 얼마나 걸렸고, 오래 걸렸다면 *왜*? API 레이턴시? 시행착오? 장애로 인한 도구 실패?
- **Q4 — 제대로 풀었나?** "해결"의 기준이 모호하면 정량화: *가드*(test/build/lint/check)가 몇 개 돌았고 몇 개 통과했나?
- **Q5 — 프롬프트/지시 누적.** 시스템 프롬프트·에이전트 지시가 계속 쌓인다. 오래되어 오염된 누적 컨텍스트를 정량적으로 모니터링할 방법이 있나?

본 사양은 표면을 이 다섯 질문 중심으로 재조직한다.

---

## 2. 설계 원칙 (브레인스토밍에서 확정)

- **P1 — 보정이 필요한 추상은 금지.** 헤드라인 숫자를 해명해야 한다면("사실 대부분 노이즈") 제거하거나 재구성한다. 제거 대상: Risk 점수, Episodes 카운트, Outcome 3단계, latency p95(이 세션). (`653ea169` 실데이터로 확인.)
- **P2 — 측정-또는-구체만.** 표면에 두는 모든 값은 (a) 관측 데이터에서 직접 계산했거나, (b) 들여다볼 수 있는 evidence-linked 구체 사실. 이종·저신뢰 이벤트를 뭉친 단일 점수 금지.
- **P3 — 모든 값에 provenance 배지.** 얼마나 믿을지를 문구로: **측정**(직접 계산) / **혼합**(도구매칭 높음 + 키워드 추정) / **추정**(휴리스틱) / **미수집·예정**(원천 데이터는 있으나 파싱 미구현). 긴 설명은 `?` 호버/클릭 툴팁.
- **P4 — 한계를 정직하게 노출.** 도출 불가한 것(Q3 API 레이턴시, Q5 의미적 오염)은 숨기거나 위조하지 않고 한계로 명시.
- **P5 — 자동 수정 없음 (non-goal).** Q1의 "무엇을 고치나"는 *증거/위치를 지목*해서 답한다(예: "이 파일을 10번 재읽음", "이 입력이 캐시를 깨뜨림"). patch 생성·수정 처방은 절대 안 함. read-only 인사이트 — 결정은 사용자가.

---

## 3. 다섯 질문 → 답 설계

각 질문 = 헤드라인 답(provenance 배지) + 구체 증거로의 드릴다운. 아래 수치는 `653ea169` 기준이며 **세션 진행 중이라 변동**한다 — 동결 invariant가 아니라 예시 앵커.

### Q1 — 효율: 비효율이 어디서? ✅ 대부분 도출 가능

- **표시:** 중복 도구 호출(예: `SessionDetailPage.tsx` Read ×10, `routes.rs` ×9), 캐시 미스(§Q5), 서브에이전트 비중(§Q2), exploration/drift 헤맴.
- **데이터:** `observed_event` tool_call을 입력 경로/명령으로 그룹(현재 DB에 존재); 캐시는 신규 usage facet(§6.1).
- **드릴(P5):** 신호 클릭 → 구체 이벤트 목록(어느 파일, 어느 턴). 위치 지목까지만, 처방 없음.
- **배지:** 측정(카운트) / 미수집·예정(캐시 기반 부분).
- **한계:** 효율 신호로서의 "drift"는 episode 분류기 버그(§7) 수정 전까지 신뢰 불가.

### Q2 — 비용/토큰 낭비 귀속: 무엇이 고비용을 유발? ✅ 가장 강하고 actionable

이것이 **centerpiece**다. 단일 "cost" 숫자 대신 **원인별 분해**:

| 원인 | 이 세션 | 출처 |
|---|---|---|
| **모델 tier** | opus 769턴 · haiku 332턴 | `assistant_message.payload.model` (DB 존재) |
| **서브에이전트/멀티에이전트** | sidechain 이벤트 **5916 / 7399 (80%)**, StructuredOutput 1967 | `is_sidechain`, tool_name Agent/Workflow/Task |
| **도구 호출량** | 도구별 카운트 | `observed_event` (존재) |
| **입력/캐시** | 청구 ~5.4M vs cache-read ~199.5M(무료) | usage facet (§6.1) |

- **헤드라인:** *청구 ~5.4M 토큰*(input 237K + cache_creation 3.9M + output 1.3M)과 *cache-read 199.5M(무료)*를 **분리** 표시 — 절대 합산 안 함(기존 "197M billed-in"은 범주 오류, cache_read는 청구 대상이 아님).
- **드릴:** "최대 비용 유발 요인" → 예: "이벤트 80%가 서브에이전트; opus가 output 토큰의 N% 차지."
- **배지:** 측정(모델/도구/서브에이전트 카운트) / 미수집·예정(토큰 분리는 facet 도입 후) / 추정(달러 비용 — §6.5).
- **참고:** `653ea169`의 80% 서브에이전트 비중은 비정상적(이 설계를 위한 audit 워크플로우가 부풀림); 그래도 지표 도출 방식 자체는 타당.

### Q3 — 시간: 얼마나, 왜? 🟡 부분적 (격차를 정직하게)

- **총합:** ~9.75시간 wall-clock(이벤트 timestamp `15:12 → 00:57`). `observed_event.observed_at`로 계산하며 **episode duration으로 계산하지 않음**(그 값들은 분류기 버그로 오염 — 앞선 분석이 거기서 "289h"를 잘못 도출).
- **귀속(도출 가능):** 시행착오(repair/retry 패턴), 도구 실패 시간(is_error 이벤트 + 타이밍), 유휴 격차.
- **귀속(도출 불가 — P4):** **호출별 API 레이턴시**는 OTel trace span이 필요한데, `653ea169`는 `latency_ms`/`trace_id`/`span_id`가 있는 이벤트가 **0**건(transcript 수집 세션, OTLP 없음). 한계로 명시하며, OTel trace가 수집될 때만 노출.
- **배지:** 측정(총합, 격차 분해) / 미수집(API 레이턴시).

### Q4 — 제대로 풀었나? 가드 실행·통과. ✅ 도출 가능(검증 재작성 전제)

모호한 "됐나"를 **가드 커버리지 + 통과율**로 재구성 — 사용자가 말한 "정량적으로 얼마나 많은 가드가 있었고 통과했나" 그대로.

- **표시:** 종류별(test / build / lint / format) 감지된 가드, pass / fail / unknown, 변경↔가드 연결(코드 변경 뒤에 가드가 따라왔나?).
- **감지(신규, §6.2):** Bash 명령을 `&& || | ; &`로 세그먼트 분리; 래퍼(`npx`, `pnpm dlx`, `bunx`, `poetry run`, …) 제거 후 Tier-1 알려진 도구 매칭 → `detection_basis = known_tool`(높음); Tier-2 키워드(`test`/`spec`) 폴백 → `detection_basis = test_keyword`(추정/guess); status는 `tool_result.is_error`에서, 세그먼트가 파이프되면(exit code 가려짐) 보수적으로 `unknown`.
- **이 세션:** `npx vitest run` ×34, `npm test` ×25 감지될 것(현재 0).
- **배지:** 혼합(도구매칭 측정 + 키워드 추정). pass/fail은 `status_basis`(exit / piped→unknown) 동반.
- **한계(P4):** 비-Bash 가드 — 브라우저 스모크(`mcp__claude-in-chrome__*`), MCP/IDE 러너, 서브에이전트 테스트 — 는 **미감지**; 완전성 주장 안 함.

### Q5 — 누적/오염 컨텍스트 모니터링 🟡 정량 프록시만

- **표시(측정):** **턴당 고정 cached-context 크기**(중앙값 ~288K, 최대 ~558K 토큰 — 1M 창의 29~56%를 매 턴 상시 점유), 그 **성장 추이**, **캐시 미스 이벤트**(~4건: turn 48/90/497/579에서 cache_read 급락·재생성 — 손실 1.19M / 재생성 0.67M). 이것이 "누적"과 "무엇이 컨텍스트를 무효화했나"를 정량화.
- **배지:** 측정(usage facet 기반) / 미수집·예정(facet 도입 전까지).
- **한계(P4, 중요):** usage 객체는 **집계** cached-prefix 크기만 준다. system-prompt vs skills vs agents vs memory로 **분해 불가**, 어느 지시가 "오염/stale"인지 **판정 불가**. 크기·성장·churn은 보여줄 수 있으나 귀속·staleness 점수는 불가. 표면에 명시.

---

## 4. 추가 제안 (다섯 질문을 넘어)

사용자가 더 나은 아이디어를 요청. 모두 권장이며 각각 옵션으로 표시.

- **A. 교차 세션 baseline (권장).** 단일 세션의 "98% cache-hit / 80% 서브에이전트 / 9.75h"는 고립되면 판단 불가. 사용자 본인의 저장된 세션들 rolling median과 비교 → "이 세션 cache-hit이 평소 중앙값보다 낮음", "서브에이전트 비중이 평소의 3배". raw 숫자를 신호로 전환; Q1/Q2에 직결. 데이터 이미 존재(저장된 다수 세션). *사용자 opt-in 질문.*
- **B. 비용 귀속 분해를 Q2 주 화면으로**(§Q2에 반영) — "어디로 갔나" 분해가 어떤 단일 cost 숫자보다 actionable.
- **C. Q3 시간 gap 귀속** — API span이 없으니 이벤트 간 timestamp gap으로 9.75h를 분해(생성 vs 도구실행 vs 유휴 vs 실패). 정직한 근사.
- **D. Q5 컨텍스트 성장 타임라인** — 한 숫자 대신 추이(턴별 cached-prefix 크기 + 캐시미스 마커); 사용자가 누적을 *본다*.
- **E. 증거 지목 ≠ 자동수정(원칙 P5)** — 재확인: Q1의 "무엇을 고치나"는 정확한 이벤트로 링크해 답하며, patch를 내지 않는다(non-goal).

---

## 5. 표면 레이아웃 (방향 A — 컴팩트 스트립 + 클릭 펼침)

브레인스토밍에서 그룹 대시보드(B)·헤드라인+드로어(C)보다 선택. 구조 변경 최소, 점진적 공개.

```
┌──────────────────────────────────────────────────────────────────────┐
│ [컨텍스트 효율 98% ▼] [토큰 청구5.4M·캐시199.5M] [검증 도구N·키워드M] [도구실패(사용자) 28] [비용 ≈$0.09] │
│   측정/미수집           미수집·예정              혼합                측정              추정         │
│ ▼ 펼침: cache-hit, 고정 컨텍스트 288K/558K, 캐시 미스 4회(drill), …                       │
├──────────────────────────────────────────────────────────────────────┤
│ phase 바: action 77% · exploration 11% · drift* 8% · intake 4%   (*drift 보정 후 신뢰) │
└──────────────────────────────────────────────────────────────────────┘
```

- 각 카드: 라벨 · 값 · 1줄 micro-detail · provenance 배지 · `?` 툴팁. 클릭하면 그 자리에서 상세(질문의 드릴다운) 펼침.
- 카드 ↔ 질문 매핑: 컨텍스트 효율 → Q1/Q5; 토큰 + 비용 → Q2; 검증 → Q4; 도구실패 → Q1/Q2; phase 바 → Q1/Q3 맥락. Q3 총시간·Q5 성장은 해당 펼침에 위치.
- **스트립에서 제거**(P1): Risk 점수, Episodes 카운트, Outcome 3단계, latency p95.
- 교차 세션 baseline(제안 A) 채택 시, 각 측정값 아래 "vs 내 중앙값" 델타로 표시.

---

## 6. 데이터 모델 & 백엔드 작업 ("무결성 위해 인접 영역 변경")

사용자가 무결성이 요구하면 수집까지 손대도 된다고 명시적으로 수용. 실제로 필요하다.

### 6.1 신규 — usage telemetry facet (최대 신규 작업)

`assistant_message` 이벤트의 `message.usage`를 턴별 **usage telemetry facet**으로 파싱(usage 객체는 assistant 메시지와 1:1이므로, 새 EventKind가 아니라 `event_id`로 키잉된 `usage_facet` 사이드테이블 — 행이 없는 기존 OTel `metric_sample` EventKind와 구분), 그리고 세션 단위 집계(뷰 또는 rollup 테이블). 필드: `input_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`, `output_tokens`, `model`, `service_tier`(+ 있으면 `ephemeral_1h/5m`). CLAUDE.md의 OTel-first·source-preserving 준수: `schema_version` + provenance 보유, raw `usage` 참조 보존. Q1·Q2·Q5 해금. *현재 `src/`는 cache 필드를 전혀 파싱 안 함; 데이터는 witmcc가 ingest하는 transcript에 이미 있음.* 신규 migration; `witmcc init-db` + 재ingest.

### 6.2 검증 감지 재작성 (Q4)

닫힌 allowlist 매칭을 다음으로 교체: 세그먼트 분리 → Tier-1 알려진 도구(래퍼 제거, 확장 가능한 시드) → Tier-2 키워드(guess) → `verification_run`에 `detection_basis` + `status_basis` 컬럼 추가. 키워드 tier는 작은 비실행 denylist(`cat/echo/grep/git/rm/mkdir/cp/mv/ls/find`) 유지. DEV-S11-03의 "닫힌 목록" 입장을 "Tier-1 시드(real-fixture로 잠금) + Tier-2 폴백"으로 의식적 개정; Tier-1 추가는 여전히 real-fixture invariant 테스트 필요. 감지기는 자체 승격 백로그를 만든다(Tier-2 적중 = Tier-1 후보), 작은 유지보수 뷰로 노출.

### 6.3 tool_failure 재구성 (Q1, Q2)

사용자 가시 도구 실패(~28: Bash/Read/browser/Edit)와 내부 자동 재시도(~1941 `StructuredOutput` 스키마 사이클) 분리 — 후자는 헤드라인에 절대 안 들어가도록 태그. 양성 비정상 종료를 "Risk"로 취급하는 것 중단.

### 6.4 episode 분류기 drift 버그 (플래그, Q1 drift 전제)

`classifier.rs:216-230`이 `exploration_streak ≥ 8`일 때 이벤트를 이중 분류(drift 방출과 동시에 같은 이벤트가 다시 exploration으로) → 중첩 episode, 0초·음수 gap 행, 빈 `evidence_node_ids`, 오염된 세션 duration. drift/episode-duration 신뢰 전에 수정 필요. (§6.2의 산물인 `missing_verification` 오탐도 설명됨.)

### 6.5 cost (Q2)

OTel `claude_code.cost.usage` metric 선호(파서는 `src/ingest/otel_metrics.rs`에 있으나 이 세션들에 metric 이벤트가 도착한 적 없음). 그 전까지는 usage 토큰 × 공개 요금표로 **추정** 도출, 배지 추정 표기, 실제 청구액으로 제시 금지.

---

## 7. 발견된 버그 (무결성 위해 수정)

1. 검증 감지가 `npx` / 복합 `cd &&` / `tsc`를 못 봄 → TDD 많이 해도 0 runs (`verification_allowlist.rs:43-79`, `verification_run.rs:288-306`).
2. Risk/findings가 내부 `StructuredOutput` 재시도 + 양성 `is_error`로 부풀려짐(high finding 1953 중 1941).
3. episode 분류기 drift 이중 분류 (§6.4).
4. `missing_verification` 오탐(1215) — #1의 산물.

---

## 8. 정직한 한계 (표면에 명시, P4)

- **Q3 API 레이턴시** — transcript 수집 세션은 불가(OTel span 없음; `latency_ms` 전부 NULL).
- **Q5 의미적 staleness & 구성 분해** — usage는 집계 cached 크기만; system-prompt/skills/agents/memory 귀속 불가, "오염" 판정 불가.
- **Q4 비-Bash 가드** — 브라우저 스모크 / MCP / 서브에이전트 테스트 미감지; 완전성 주장 없음.
- **모든 수치는 세션별·실시간 변동**; "이게 좋은 건가?"의 해법은 교차 세션 baseline(제안 A).

---

## 9. 테스트 접근 (TDD red-first, CLAUDE.md 준수)

- **백엔드, real-fixture 앵커:** usage-facet 추출을 동결된 `tests/fixtures/transcripts/real/` usage 객체로 검증; 검증 감지를 실제 `cd && npx vitest`·파이프·dry-run(`--no-run`) 명령 fixture로 검증(red 먼저). 신규 migration은 `init-db` + 재ingest로 검증.
- **프론트엔드:** 신규 DTO/props 계약 테스트(jsdom은 layout/CSS 테스트 불가); provenance 배지 렌더링·드릴 펼침은 컴포넌트 테스트; commit 전 **브라우저 스모크**(witmcc serve + claude-in-chrome), 프로젝트 UI 규칙대로.
- **어떤 지표도 provenance 배지가 연결되고 데이터 경로가 실제(또는 명시적 미수집·예정)가 아니면 헤드라인에 올리지 않는다.**

---

## 10. Non-goals (재확인)

개선 patch / 자동 수정 없음(Q1은 증거 지목만). 외부 correction/label/status write 없음(annotation 모델 없음). Claude Code 설정/hook/skill/memory 변경 없음. read-only, local-first `127.0.0.1`.

---

## 11. 확정된 결정 (2026-05-30 승인)

1. **교차 세션 baseline(제안 A): 채택** — 핵심 지표 슬라이스가 안착한 *이후* enhancement 슬라이스로 구현(각 측정값 아래 "vs 내 중앙값" 델타).
2. **Q5 범위: 정량 프록시만** — cached-prefix 크기 / 성장 / churn + 캐시미스 드릴. 의미적 staleness와 구성(system-prompt vs skills vs agents vs memory) 분해는 명시적으로 **범위 밖**(데이터에 없음).
3. **cost: 잠정 공개 요금표 추정** — usage 토큰 × 공개 요금표, 배지 추정, 실제 청구액으로 제시 금지; OTel `claude_code.cost.usage` metric 도착 시 교체.
4. **Q4 가드: build / lint / format 포함, 종류 구분** — test와 합산하지 않고 종류별 표시.
5. **순서:** (1) usage telemetry facet §6.1 [Q1/Q2/Q5 해금] → (2) 검증 감지 재작성 §6.2 [Q4] → (3) tool_failure 재구성 §6.3 → (4) episode 분류기 drift 수정 §6.4 → (5) cost §6.5 → (6) 교차 세션 baseline(제안 A). 프론트 표면(방향 A + 배지 + 드릴)은 각 슬라이스가 해금하는 데이터와 함께 점진적으로 안착.
