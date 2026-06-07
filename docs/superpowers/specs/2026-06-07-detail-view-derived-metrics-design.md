# 디테일 뷰 & derived layer 재설계 — design spec

- 날짜: 2026-06-07
- 상태: design (brainstorming 합의 완료, 구현 계획 미작성)
- 선행: `2026-06-06-wimcc-data-model.md`, `2026-06-06-drop-judge-graph-layers.md`
- 관련 시각 자료: `.superpowers/brainstorm/22965-1780793136/content/*.html`

## 1. 배경과 목적

직전 작업에서 데이터 모델의 비효율(judge·graph 레이어)을 제거했다. 이번 작업의
목표는 두 가지다.

1. **데이터 모델 재검증 + 개선 기준 확립** — 사람과 LLM이 "무엇을 개선할지" 판별할 수
   있는 기준을 세운다.
2. **휴먼 리더블한 디테일 뷰(insight view) 개선** — execution replay에서 "무엇이, 왜
   그렇게 됐는가"를 디테일 뷰에서 바로 읽을 수 있게 한다.

### 현황 진단 (개선 출발점)

디테일 뷰(Insight 탭)는 현재 **HOW(얼마나 걸렸나/컸나)** 지표만 보여주고,
**WHAT(실제 명령·결과·diff·프롬프트)** 은 헤더 한 줄이나 Raw JSON 탭으로 밀려 있다.
또 11개 메세지 타입 중 metrics가 있는 건 4개(tool_call·assistant·thinking·hook)뿐이고,
공통 골격이 없어 타입별 패널이 제각각이다. 이는 "execution replay" 정체성과 어긋난다.

## 2. 핵심 원칙 (개선 기준)

1. **원본 vs 가공 구분.** 디테일 뷰의 모든 값은 ① Claude Code 원본(관측)인지
   ② wimcc 가공(해석)인지 사용자가 항상 구분할 수 있어야 한다. evidence-linked가
   성립하려면 "CC가 말한 것 vs wimcc가 추론한 것"이 분리돼야 한다.
2. **derived = 결정적 bad smell 지표.** 가공 레이어는 raw에서 규칙기반·결정적으로
   얻는 LLM 동작 bad smell의 정량 지표여야 한다. 회귀 방어와 LLM self-improvement의
   토대가 될 만큼 객관적이어야 한다.
3. **판단 금지, 사실만.** 지표는 사실·카운트·비율까지만 만든다. severity/confidence
   같은 판단·해석·가정은 넣지 않는다 — 판단은 LLM·사람이 지표를 보고 한다.
4. **매직넘버 금지.** 임계값은 코드에 박지 않는다. raw 값 노출 또는 분포(percentile)로
   결정적으로 도출한다.
5. **단일 사례 일반화 금지.** 표본 1건 fixture로 룰을 굳히지 않는다. 시퀀스/임계값
   룰은 실데이터 누적 후 잠근다.
6. **존재가치 = insight 재료.** 목적(bad smell 판단 재료)을 달성하지 못하는 데이터
   모델은 존재가치가 없다 — 제거 또는 재료로 승격.
7. **replay와 분석은 별개 표면.** 실시간 replay(메세지·디테일 뷰)와 온디맨드 구간
   분석은 목적·데이터·UX가 다르다. replay에 세션 집계·비교 모델을 억지로 주입하지
   않는다 — 그렇게 하면 replay 구조가 망가진다. 두 표면을 분리해 각자의 UX를 갖는다.

## 3. 데이터 모델 층위: 원본 vs 가공

### ① Claude Code 원본 (그대로 관측 — `ObservedEvent.kind`)

wimcc는 정규화만 한다: 공통 필드를 1급으로 승격하고 나머지는 `payload`에 raw 보존
(source-preserving). 의미를 새로 만들지 않는다.

- **Transcript JSONL**: user_message, assistant_message, thinking, tool_call,
  tool_result, hook_event, system_summary, session_state, attachment_meta
- **OTLP export**: otel_span, metric_sample, log_record

> 주의: 메세지 스트림(`ConversationStream`)에 실제 렌더되는 건 ObservedEvent의 일부다.
> `otel_span·metric_sample·attachment_meta·session_state`는 drop된다.

### ② wimcc 가공 (derived — 원본엔 없는 해석)

별도 사이드테이블 + 별도 API. ObservedEvent를 재료로 결정론적 규칙으로 생성.

| 가공물 | 재료 | 비고 |
|--------|------|------|
| `verification_run` | tool_call(Bash)+result · PostToolUse hook · otel_span | "test/build/lint" 분류. **CC엔 이 개념 없음** |
| `diff_hunk` | tool_result의 structuredPatch | 파일 변경 단위. (EventKind에 `DiffHunk` 잔재 — 정리 후보) |
| `usage_facet` | assistant_message.usage | 턴당 토큰 재집계 |
| `finding` | ObservedEvent 세션 view | 본 spec에서 **signal로 전환**(아래) |

## 4. derived layer 재정의

### 4.1 존재가치 기준 (실측)

`SessionInsightView`는 events·diff_hunks·verification_runs만 담는다. 4개 extractor의
소비를 코드로 확인한 결과:

- `verification_run` → `final_state_mismatch`가 소비 ⇒ **존재가치 ✓**
- `diff_hunk` → `risky_action`이 소비 ⇒ **존재가치 ✓**
- `usage_facet` → 어떤 finding extractor도 안 씀(view 미포함). KPI/pricing에만 ⇒
  KPI를 "표시 인사이트"로 인정하므로 유지하되, 본 spec의 집계 지표로 재배치
- `finding` → 인사이트 결과물(재료 아님). severity/confidence는 원칙 3 위반 ⇒ **수정**

### 4.2 bad smell 지표 카탈로그 (raw 근거, 결정적)

모든 지표는 `tests/fixtures/**/real/`에서 확인된 실제 필드에 근거한다.

- **A. LLM 응답 품질** (llm_request span · message.usage): `attempt`(>1=재시도),
  `success`(=false), `stop_reason`(=max_tokens=잘림), `duration_ms`/`ttft_ms`,
  `cache_read ÷ 총입력`(캐시효율)
- **B. 도구 실행** (tool_result · tool_decision log): `is_error`/`success`,
  `tool_result_size_bytes`(거대 결과), `decision`(=deny), `decision_source`(=user=마찰)
- **C. 코드 변경** (structuredPatch · diff_hunk): `lines_added/removed`,
  `userModified`(사용자 교정), file_path 변경 횟수(churn)
- **D. 검증** (verification_run): `status`(=failed), 변경턴 중 검증無 비율(누락),
  `status`(=unknown)
- **E. 흐름·시퀀스** (event 순서 — 신규 영역): re-read(동일 file_path Read N회),
  error-retry loop(동일 tool_use_id 재시도), self-revert(diff added→removed 매칭),
  턴당 tool_call 수
- **F. 인프라** (mcp/hook log): `status`(=failed)+`error_code`,
  `num_non_blocking_error`/`num_cancelled`

> thinking 본문은 raw에 미기록(signature만) — thinking 지표는 llm_request span의
> output_tokens/duration에서만 결정적으로 얻는다.

## 5. 데이터 모델 구조 (선택: 구조 B)

**per-event 지표는 ObservedEvent에서 직접 도출하고, 회귀방어가 필요한 세션/시퀀스
집계만 신규 테이블에 고정한다.**

### 5.1 per-event: ObservedEvent 직접 도출

A·B그룹 같은 이벤트 귀속 지표는 facet 테이블로 복제하지 않는다(source-preserving상
중복). 프론트는 이미 이벤트에서 파싱 중이며(`toolMetrics.ts`,
`llmRequestMetrics.ts`), 디테일 뷰는 이벤트 클릭 기반이라 이벤트에서 직접 렌더가
자연스럽다.

### 5.2 집계: `behavioral_metrics` 신설

세션/시퀀스 집계(E그룹 + 실패율·캐시율·누락율·churn)는 결정적으로 계산해 고정한다.
회귀방어·self-improvement 메트릭의 비교 단위.

- scope: session (필요 시 turn)
- 결정적·idempotent. 같은 입력 → 같은 숫자.
- **열린 질문**: ingest 1회 계산 후 저장(증분 갱신) vs 조회 시 계산 — 별도 결정.
  live `serve`는 OTLP batch마다 전체 재계산하는 기존 비용이 있어 이 선택에 영향.

### 5.3 finding → signal 전환

`finding`의 `severity`/`confidence`(판단)를 제거하고, 해석 없는 `signal`로 전환한다.

```
Signal { kind, scope(event|turn|session), evidence_refs, facts: {…raw값} }
```

`verification_run`·`diff_hunk`는 fact 레코드로 유지. `usage_facet`은 유지하되
캐시효율 등 비율은 집계로 승격.

### 5.4 계산 비용

`run_extractors`는 ingest당 1회, 세션 events(≤100k)를 메모리에 1회 로드해 순회한다
(순수 CPU·결정적). 집계를 같은 패스에 끼우면 한계비용 ≈ 0. 시퀀스 패턴도 file_path별
카운트라 O(N). **폐기 기준**: 실데이터에서 신호가 거의 0이거나 O(N²)+인 지표는 채택
안 함. per-event facet 전면 고정(대안 A)은 ObservedEvent 중복이라 비효율 — 채택 안 함.

## 6. bad smell 판독기

### 6.1 룰베이스 predicate (기존 L1 패턴 확장)

판독기는 순수 함수다. judge/LLM 없음. 기존 extractor가 이미 룰베이스다
(예: `tool_failure`는 `is_error==true` + 윈도우 스캔, confidence 1.0 = 직접 인용).

```
fn detect(window: &[ObservedEvent], facts, cfg) -> Vec<Signal>
```

결정적·idempotent·단일 패스. 출력 Signal엔 판단 없이 raw 사실만.

### 6.2 signal 3분류

- **단일 필드 술어** (해석 0, 즉시 가능): is_error·attempt>1·success=false·
  stop_reason=max_tokens·decision=deny·decision_source=user·userModified·mcp
  failed·hook error·verification status → 필드 직접 인용
- **임계값/연속값** (가정 위험): ttft·duration·result_size·lines·턴당 tool 수·
  cache율 → 매직넘버 금지. raw값 노출 / 세션내 상대분포 / 누적 percentile
- **시퀀스/관계** (정의 필요): re-read·error-retry loop·self-revert·churn →
  "동일/되돌림"을 엄격 정의(완전일치)하면 결정적. 윈도우는 명시 파라미터

### 6.3 현 코드의 "가정 3종" 제거 (`tool_failure.rs` 실측)

1. `severity: high/info` (판단) → 제거, 사실만
2. `BENIGN_EXIT_MARKERS=["no matches found"…]` (휴리스틱 분류) → 제거,
   exit_code·메세지를 raw로 노출
3. `INTERNAL_RETRY_TOOLS=["StructuredOutput"]` + `RETRY_WINDOW=5` (단일사례
   하드코딩 + 매직넘버) → tool_name·재시도 횟수를 사실로 노출, 윈도우는 config

### 6.4 detector 분해 (LLM-legible)

각 detector를 3조각으로 분해해 LLM이 읽고·튜닝하고·개선할 수 있게 한다.

- **① manifest** (선언, LLM이 읽는 진실): `id`·`intent`·`inputs`(raw 의존성 명시)·
  `rule`·`output`·`rationale`(docs/fixture 근거 앵커)
- **② config** (rule pack 파일, LLM이 튜닝): `enabled`·임계값·윈도우·percentile.
  매직넘버를 코드 밖으로. on/off로 단계 도입
- **③ predicate** (실행, 순수 함수): manifest를 구현. 테스트가 manifest의
  inputs/output ↔ predicate 일치를 잠가 "manifest 거짓말" 방지

### 6.5 LLM 개선 루프 (tagging loop 확장)

registry가 manifest 카탈로그를 read-only API/MCP로 노출한다. LLM은 "detector →
signal → evidence → raw event"의 완전한 사슬을 읽고 안전하게 개선한다.

1. **조회**: MCP로 manifest + config + 최근 signal + 의심표본 조회
2. **제안**: config 조정 또는 새 manifest+predicate 초안
3. **잠금**: fixture 실패 테스트 먼저 → 통과 (TDD)
4. **검증**: 재ingest → signal 분포 변화 확인 → 루프

> 선례: `eventTags` untagged-bash 루프, redaction `rule_pack`이 같은 철학으로 동작 중.

### 6.6 detector 가치 측정 (신호 분포 메타지표)

**어떤 지표가 insightful한지는 선험적으로 모른다 — 데이터로만 안다**(표본 1건으로는
절대 모름). 그래서 각 detector에 신호 분포 메타지표를 붙여 가치를 정량 측정한다.

- **발화율**: 이벤트 중 signal 비율. 항상 0%/100% = 정보 없음
- **분산/엔트로피**: 값이 갈리는 정도. 늘 같은 값 = 변별력 0
- **actionability**: 보고 행동을 바꿀 수 있나. 손쓸 수 없으면 noise

루프: ① 싸게 만든다(증분/디바운스 · config on) → ② 신호 분포 관측(발화율·분산 누적)
→ ③ 판정(무신호=폐기 · 유의미=유지) → ④ LLM 루프(임계값 튜닝 · 새 detector).
이것이 원칙 6("존재가치 없으면 제거")의 실행 메커니즘이자 detector를 config로 on/off
하게 만든 이유다. **비싸게 만들고 검증하면 매몰비용 — 싸게 만드는 게 가치 실험의 전제.**

### 6.7 self-improvement 근거로서의 적격성

질문: "LLM이 이 데이터를 자기 개선 근거로 쓸 수 있나?" → **조건부 예.**

- **충족**: 결정적(원칙 1·3) + 귀속가능(evidence_refs) ⇒ 자기 개선의 "재료"는 된다.
- **사후 검증**: 변별력(좋은/나쁜을 가르나)은 신호분포로 측정, 무신호 폐기(§6.6).
- **정직한 한계**: 지표에 "얼마부터 나쁜가"의 규범 기준이 없다. **단일 세션 절대값은
  관찰이지 개선 근거가 아니다.** 개선 근거가 되려면 비교 대상(이전 세션·추세·A/B)이
  필수 — 비교 없는 절대값을 근거로 삼으면 그 자체가 가정(원칙 3 위반).

자기 개선 4형태별 적격성:

| 형태 | 적격 | 전제 |
|------|------|------|
| 회귀 방어 (A/B) | ◎ 강함 | 변경 전후 지표 비교. 동일 작업 |
| cross-session 메타개선 (프롬프트·스킬·CLAUDE.md) | ○ 현실적 | 다세션 baseline (보류 트랙) |
| detector 자체 개선 | ○ 설계됨 | §6.5 LLM 루프 |
| in-context 런타임 자기조정 | △ 부분 | MCP 조회는 가능(read-only), 자동 행동반영은 범위 밖 |

→ 자기 개선은 사후 비교라서 §10.1의 **온디맨드 분석**이 이 목적에 정확히 부합한다.
"이 데이터로 개선됐나?"라는 메타 질문도 같은 지표의 전후 비교로 측정 가능(자기 검증).

## 7. 노출 (입도별 분리)

| 지표 입도 | 노출 위치 | 이유 |
|-----------|-----------|------|
| per-event (응답·도구 지표) | 디테일 뷰 (기존) | 이벤트 클릭 기반 귀속 |
| 이벤트 귀속 signal (re-read·되돌림) | 디테일 뷰 + 마커 | 관련 이벤트에 마커 + 근거 |
| 세션/시퀀스 집계·비교 (실패율·churn) | **온디맨드 구간 분석** 버튼·MCP | 이벤트에 안 붙음 · 사후 비교 |
| 전 계층 | read-only API·MCP·export | LLM·회귀방어 토대 (항상 제공) |

"화면 미노출" = API/MCP로는 내보내되 사람 UI만 생략 (신호 약한 지표).
**메세지 뷰 타임라인은 영향 없음**(ObservedEvent만 사용). 영향권은 전구
마커(`findingEventIds`)·디테일뷰·KPI strip 3곳.

## 8. 디테일 뷰 UI

### 8.1 공통 골격 5층 (모든 타입 공유)

- **H. 헤더 + correlation**: 아이콘·타입·시각·actor + 원본/가공 배지(파랑/보라) +
  점프 칩(tool_use_id↔result, request↔span, turn)
- **① WHAT — 한 일**: 실제로 무엇을 했는지 휴먼리더블하게 (지금 최대 갭).
  Raw로 안 가도 핵심을 본다
- **② HOW — 지표**: per-event 결정적 지표 (이벤트 직접 도출). `결정적` 표식
- **③ SIGNALS — bad smell**: 이벤트 귀속 사실만. severity 없음 (finding 대체)
- **④ RAW**: 접힌 원본 JSON (source-preserving, unknown 필드 보존)

### 8.2 타입별 데이터 매트릭스 (★ = 목적부합 추천)

원본 (파랑):

| 타입 | WHAT | HOW | SIGNALS |
|------|------|-----|---------|
| user_message | ★ 프롬프트 본문·slash-command·첨부 | — | 턴 시작점 |
| assistant_message | 응답 본문 | ★ 토큰·캐시·비용·속도·ttft·모델 | ★ 잘림·재시도·실패 |
| thinking | 본문 미기록 → "추론 N토큰" | ★ output_tokens·duration | 잘림·재시도(공유) |
| tool_call (+result) | ★ command/input 전문 + **결과 출력**·is_error | 소요·입출력 크기·승인 | ★ 실패·거대결과·deny·re-read·error-retry |
| hook_event | ★ hook명·명령·stdout/stderr | 소요·exit_code | ★ hook 실패·num_error |
| system_summary | ★ compact/away 본문 | 압축 규모(가능시) | ★ compaction=컨텍스트 한계 |
| log_record | ★ friendly명 + 전체 attributes | duration(해당시) | ★ mcp 실패·subagent 실패 |

가공 (보라):

| 타입 | WHAT | HOW | SIGNALS |
|------|------|-----|---------|
| verification_run | ★ command·kind·status·exit·failure_summary | 소요·covered_diff_hunk_ids | ★ 실패·unknown (누락율은 §8.3) |
| diff_hunk | ★ patch_preview·file_path·change_type | lines_added/removed | ★ userModified (churn은 §8.3) |

패턴: 원본은 WHAT 중심(+응답계열 HOW 풍부), 가공은 WHAT+SIGNALS 동시(애초에 bad
smell 재료).

> **정정 (원칙 7):** 세션 집계형(검증 누락·file churn의 전체 수치)은 디테일 뷰 SIGNALS
> 에서 **제거**하고 §8.3 구간 분석 뷰로 보낸다. 디테일 뷰 SIGNALS에 남는 건 **per-event
> 귀속 사실 + per-event 시퀀스 마커**("이 이벤트가 re-read의 N번째")뿐이다. 세션 전체
> 수치를 디테일에 넣지 않는다 — replay 구조를 지키기 위해.

### 8.3 온디맨드 구간 분석 뷰 (별도 표면)

replay와 분리된 독립 뷰. 집계·비교 모델을 메세지/디테일에 주입하지 않기 위함(원칙 7).

- **목적**: bad smell 정량 분석 · 회귀 방어 · self-improvement 근거(비교)
- **데이터**: behavioral_metrics(집계·시퀀스) + 비교/분포 + detector 가치측정(§6.6)
- **UX**: 구간 선택(시간 범위 · 턴 범위 · 세션 전체 · 다세션 비교) → 분석 실행 →
  분포·비교·추세 표시. replay의 스트리밍 타임라인과 완전히 다른 화면·인터랙션
- **시간성**: 온디맨드 (버튼/MCP, 계산+캐시 — §10.1)
- **산출 예**: 도구 실패율 · 캐시 히트율 · re-read 수 · 검증 누락율(+baseline 대비
  ▲▼) · detector 발화율·분산
- read-only. export/MCP로도 동일 제공.

## 9. 비목표

- finding/resource에 외부 correction·label·status를 쓰는 API (no annotation model)
- 개선 patch 자동 생성
- hidden reasoning 복원 (thinking 본문은 미기록)
- 메세지 뷰 타임라인 렌더 구조 변경 (영향권 외)
- LLM judge 재도입 (deterministic 유지)

## 10. 결정 사항 (구 열린 질문)

1. **behavioral_metrics 계산 = per-event 실시간 + 집계 온디맨드.** per-event 지표(A·B
   그룹)는 이벤트 1건만 보면 되므로 도착 즉시 계산 — 이미 실시간(프론트 파싱).
   세션/시퀀스 집계·비교는 **온디맨드**: "구간별 분석" 버튼 또는 MCP 호출 시 계산하고
   캐시(결정적이라 캐시 안전). 자기 개선 근거는 본질적으로 사후 비교라 실시간이
   불필요하므로, 라이브 증분/디바운스조차 MVP엔 넣지 않는다 — 라이브 컴퓨트 부담 제거.
   (매 batch 전체 재계산은 누적 O(N²)라 처음부터 배제. 비율 등은 증분 가능하지만,
   온디맨드면 증분 자체가 불필요. 가치 확인 후 조회가 빈번해지면 그때 캐시·증분 도입.)
2. **finding → signal = 신규 테이블 + finding 폐기(drop migration).**
3. **EventKind `DiffHunk` 잔재 = 이번에 제거** (observed.rs enum + repo_observed.rs
   역매핑 + sse.rs 매핑).
4. **detector config = TOML + 코드 fallback.** 프로젝트 표준 포맷(TOML). predicate(룰)는
   코드 내 버전드(redaction rule_pack 선례), 파라미터만 외부 `detectors.toml`로 LLM이
   튜닝. 위치는 wimcc config 디렉토리(`~/.config/wimcc/`, macOS
   `~/Library/Application Support/wimcc/`). schema 키(`detectors.v1`). fallback:
   파일/섹션/키 누락 시 해당 단위만 코드 default — config 없이도 동작.
5. **시퀀스 룰(E그룹) 1차 대상 = re-read + error-retry loop.** 가장 명확·결정적이고
   오탐이 적다(re-read=동일 file_path Read 반복, error-retry=tool_failure 윈도우 확장).
   self-revert(diff 라인 정규화 필요)·턴당 tool 수(임계값 분포 필요)는 실데이터로
   정의·분포를 잠근 뒤 후속 도입.
6. **분석 DB = SQLite 유지.** 분석 컴퓨트는 DB가 아니라 Rust in-memory(extractor가
   `&[ObservedEvent]` 순회, SQL 집계 미사용)이고, DB는 `session_id` 인덱스 범위
   조회(OLTP)만 한다. behavioral_metrics를 사전계산·저장하면 비교도 작은 테이블
   대상이라 SQLite로 충분. local-first 단일 사용자라 OLAP 규모(수억 행 벡터화)가 아니며
   단일 DB 운영이 단순하다. DuckDB는 분석을 SQL 기반 대량 ad-hoc 집계로 전환할 때만
   고려하며, 그때도 `sqlite_scanner`로 SQLite 파일을 직접 분석할 수 있어 지금 선택이
   미래 도입을 막지 않는다(저장은 SQLite, 분석만 DuckDB 부착).

## 11. 단계적 구현 순서 (개략, 계획에서 확정)

1. finding → signal 전환 (severity/confidence 제거, 가정 3종 제거)
2. detector 분해 (manifest + config + predicate) + 기존 4개 이식
3. 디테일 뷰 공통 골격 5층 (WHAT 끌어올리기 + 원본/가공 배지)
4. 타입별 WHAT/HOW/SIGNALS 채우기
5. `behavioral_metrics` + **별도 구간 분석 뷰**(replay와 분리, 온디맨드 버튼/MCP, §8.3)
   + 신호분포 가치측정(§6.6)
6. detector manifest의 read-only API/MCP 노출 + LLM 개선 루프
7. 시퀀스 룰(E그룹) 실데이터 기반 단계 도입
