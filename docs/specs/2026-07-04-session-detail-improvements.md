# 세션 상세 개선 — 필터링 · 비용 가격표 · 대시보드 인사이트 이식 (2026-07-04)

세션 상세(리플레이) 화면의 세 가지 독립 개선을 하나의 스펙으로 확정한다.
구현은 영역별 독립 PR로 분할한다(§5). 표기 원칙은 대시보드 스펙
`docs/specs/2026-07-04-dashboard-redesign.md` §0을 그대로 계승한다 — 판정
문장 금지(숫자·delta·관측 사실만), 모델명 전체 표시명, 미측정 `—`, 표본 수
병기(n<3 "표본 부족"), 보라(#b07dff)는 코호트 경계 문법 전용. 새 `.tip`
키는 툴팁 카피 규칙(CLAUDE.md)과 `tipStyle.test.ts` 게이트를 통과해야 한다.

## §1. 메시지 카드 필터링

### 1.1 요구 (2026-07-04 사용자 확정)

- 조건 축 4종: **카드 종류(kind) · 역할/출처(role/origin) · 실행 결과
  (에러/시그널/검증) · 내용 검색(도구명/모델/텍스트)**.
- 축끼리 **AND**, 한 축 안의 다중 값(CSV)은 **OR**.
- 동작은 **숨김(제외)** — 비매칭 카드를 스트림에서 제거.
- 수행 위치는 **서버측** — 클라이언트 윈도우 버퍼(최대 5000)에 갇히지 않고
  세션 전체에서 정확해야 한다.

### 1.2 API 계약 — `GET /v1/sessions/:id/events` 확장

`EventsQuery`(src/api/routes.rs)에 다음 파라미터를 추가한다. 모두 기존
`before`/`after`/`limit` 커서 페이징과 결합 가능해야 한다.

| 파라미터 | 형식 | 의미 |
|---|---|---|
| `kind` | CSV (기존) | EventKind snake_case |
| `role` | CSV | 메시지 payload role: `user`,`assistant`,`system` |
| `origin` | CSV | `human`,`command`,`command-output`,`skill`,`system`,`notification`,`teammate` |
| `error` | `true` | `tool_result`의 `is_error=true`인 이벤트. 다른 에러 표현을 추가로 포함하려면 real fixture 앵커가 선행되어야 한다 |
| `signal` | `true` | 시그널 evidence로 연결된 이벤트만 (signals 조인) |
| `verification` | CSV | verification run outcome: `passed`,`failed`,`unknown` |
| `tool` | CSV | 도구명 (`Bash`, `Edit`, `mcp__…` …) |
| `model` | CSV | 모델 id 정확 일치 |
| `q` | 문자열 | 메시지 본문 텍스트 + 도구 입력/결과 문자열 필드 부분 일치, 대소문자 무시 |

실행 전략: SQL로 싸게 거를 수 있는 축(kind, model 등 컬럼/단순
json_extract)은 WHERE로, payload 파생이 필요한 축(origin·error·q)은 **커서
순서로 행을 스캔하며 Rust 술어로 평가하고 limit 충족 시 중단**한다. 세션
단위 행 수(현 코퍼스 최대 6k행)에서 스캔 비용은 무시 가능(§G-3 실측 관례
준용 — 아프면 재실측). `signal=true`는 시그널 evidence event_id 서브쿼리
IN으로 처리한다.

응답 `SessionEventsResponse`에 **`matched_count`**(필터 매칭 총수, 필터
파라미터가 하나라도 있을 때만 계산·포함)를 추가한다.

`around` 딥링크와 필터의 결합은 기존 `kind`×`around` 제한과 동일하게
**미지원**(400) — 점프 UX는 1.4의 "점프 시 필터 해제" 규칙으로 해소한다.

### 1.3 origin 분류의 서버 이식 (편차 명기)

origin 분류 로직은 현재 프론트 `webui/src/components/replay/stream/streamModel.ts`
(classify)에만 존재한다. 서버 필터를 위해 **payload 술어 기반 Rust 함수**
(`origin_of(payload) -> Origin`)로 이식한다.

- 판정 근거는 저장된 payload 필드만 사용한다(commandName 존재, sidechain,
  teammate 식별자, isMeta 등). 프론트 전용 파생 상태에 의존하지 않는다.
- **real-data anchoring**: 각 origin 값별로 `tests/fixtures/**/real/`의 실
  transcript payload를 동결해 invariant assertion으로 잠근다.
- **의도적 이중화**: TS `classify`와 Rust `origin_of`가 같은 분류를 두 곳에
  갖게 된다. 단일화(분류의 백엔드 이관)는 이번 범위 밖 — 구현 시
  implementation-notes에 편차로 기록하고, 두 구현의 분류 일치를 같은 real
  fixture로 양쪽에서 앵커해 드리프트를 테스트로 잡는다.

### 1.4 프론트 — FilterBar · flat 모드 · 라이브 결합

- **FilterBar**: 스트림 슬롯 상단에 축별 칩 드롭다운(kind/origin/결과/
  도구·모델) + 텍스트 입력(디바운스 ≥300ms). 활성 조건은 제거 가능한 칩으로
  나열하고 `matched_count`("N건 매칭")를 병기한다.
- **URL 동기화**: 필터 상태를 URL 쿼리 파라미터로 왕복(딥링크 공유·새로고침
  보존).
- **윈도우 결합**: 필터 변경 시 `useSessionWindow` 버퍼를 리셋하고 필터
  파라미터로 tail 재로드. `loadOlder`/`loadNewer` 모두 동일 파라미터 유지.
- **SSE 백필**: 알림 수신 시 `?after=` 백필 쿼리에 활성 필터를 그대로
  붙인다(SSE envelope은 payload 없는 알림이므로 채널 변경 없음). autoscroll
  OFF 중 대기 카운트는 필터 활성 시 비매칭을 셀 수 있으므로 숫자 대신
  **"새 이벤트 ↓"** 문구로 대체한다.
- **flat 모드**: 필터가 하나라도 활성이면 그룹핑(서브에이전트/워크플로우/
  배치/스캐폴드 묶음)을 만들지 않고 매칭 카드를 시간순 평면 렌더한다. 카드에
  출처 배지(예: "⑂ 서브에이전트 내부")를 붙여 맥락을 표시한다. 부분 매칭
  그룹의 골격 유지는 접힘 상태·빈 그룹 처리 복잡도 때문에 기각(2026-07-04).
- **점프 규칙**: 상세/분석 패널 등 스트림 외부발 이벤트 점프(시그널 evidence,
  검증 리듬 점 등)의 대상이 활성 필터에 매칭되지 않으면 **필터를 해제하고
  이동**하며 해제를 UI로 알린다. `j`/`k`/`e` 키보드 내비는 필터된 목록
  기준으로 동작한다.

### 1.5 테스트 (TDD red 우선)

- Rust: 파라미터별 필터 정확성(fixture 세션), 축 조합 AND, CSV OR, 커서
  페이징 결합(페이지 경계에서 누락/중복 없음), `matched_count`, `around`
  결합 400, origin real-fixture 앵커.
- TS(vitest): FilterBar 상태↔URL↔쿼리 매핑, flat 모드 렌더(그룹 미생성 +
  출처 배지), 대기 카운트 문구 전환, 점프 시 필터 해제.
- 브라우저 smoke 후 커밋(WebUI 변경 공통 게이트).

## §2. 비용 — 툴팁 가격표 · 갱신 자동화

### 2.1 가격표 데이터의 파일 분리

`src/insight/pricing.rs`의 `PRICING` 상수·`PRICING_VERSION`을
**`pricing.json`**(저장소 체크인 데이터 파일)으로 옮기고, Rust는
`include_str!` + serde로 컴파일 타임 임베드·기동 시 1회 파싱한다.

- JSON 스키마: `{ version: "pricing_estimate@YYYY-MM-DD", source_url,
  models: { "<model-id>": { input_per_mtok, cache_creation_per_mtok,
  cache_read_per_mtok, output_per_mtok } } }`.
- 갱신 스크립트가 Rust 소스를 재작성할 필요가 없어져 자동화가 견고해진다.
- `tests/api_usage.rs`의 버전 리터럴 하드코딩은 "형식
  `pricing_estimate@\d{4}-\d{2}-\d{2}` regex + 소스 상수와 동일" 검증으로
  교체한다 — 가격 갱신 때마다 테스트 수기 수정을 없애되 계약(엔드포인트가
  소스 버전을 그대로 노출)은 계속 잠근다. 기존 단가 잠금 단위 테스트
  (deterministic_cost_for_known_model 등)는 JSON 값 기준으로 유지한다.

### 2.2 `/usage` 응답에 단가 노출

`SessionUsageDto`의 per-model 항목에 **적용 단가 4종**(`rates`)을 추가한다.
미가격 모델은 `rates: null`. `pricing_version`은 기존 필드 유지. 가격표
SSOT는 백엔드 — 프론트에 단가를 하드코딩하지 않는다.

### 2.3 비용 툴팁 — 동적 조립

비용 카드(`insight.cost.tip`) 툴팁을 정적 i18n 문자열에서 **동적 조립**으로
전환한다: 기존 추정 근거 문구 + 아래 두 블록을 덧붙인다.

1. **세션에서 관측된 모델만**의 단가 줄 — 모델당 1줄, renderTipMarkup 문법:
   `` `claude-fable-5` `` in **$10** · out **$50** · cache-read $1 ·
   cache-write $12.5 /1M 형식.
2. 기준일 줄 — `pricing_version`에서 날짜를 추출해 "공개 가격표
   **YYYY-MM-DD** 기준" (i18n 템플릿 키, en/ko 패리티).

정적 부분·템플릿 키는 tipStyle 게이트 대상이며 카피 규칙(문장 단위 줄바꿈,
강조 ≥1, 긍정문)을 지킨다.

### 2.4 갱신 자동화 — GitHub Actions 주간 cron

워크플로우 `.github/workflows/pricing-refresh.yml`:

1. 주 1회 공식 가격 문서(`pricing.json`의 `source_url`,
   platform.claude.com 가격 페이지)를 fetch → 파싱 → 현재 `pricing.json`과
   diff.
2. **변동 시**: `pricing.json`(단가 + `version` 날짜)을 갱신하는 **PR을 자동
   생성** — 본문에 출처 URL·모델별 diff 표 포함. 병합은 사용자 검수 후
   (no-self-merge·real-data anchoring 유지).
3. **파싱 실패 시**(페이지 구조 변경 등): 조용히 넘어가지 않고 워크플로우
   실패 + 이슈 자동 생성.
4. **무변동 시**: 아무것도 커밋하지 않는다. `version` 날짜는 "마지막 갱신일"
   의미를 유지한다(주간 무변동 bump 커밋 소음 방지).
5. 스크립트(`scripts/update-pricing.ts`)는 로컬 수동 실행도 지원한다
   (`--check` 모드: diff만 출력).
6. **런타임 외부 fetch 금지**: wimcc 바이너리는 실행 중 외부 가격 API를
   호출하지 않는다(local-first). 갱신은 개발/CI 시점의 저장소 편집으로만.

파서 테스트는 실 가격 페이지 캡처를 fixture로 동결해 앵커한다(구조 변경 시
red가 나게).

### 2.5 테스트

- Rust: pricing.json 로드·스키마 검증, 버전 형식 regex, `/usage` rates
  노출(가격 有/無 모델), 기존 단가 잠금 테스트 유지.
- TS: 툴팁 조립(관측 모델만 포함, 기준일 줄, 미가격 모델 표기), tipStyle,
  en/ko parity.
- 스크립트: 동결 페이지 fixture → 파싱 결과 잠금, 파싱 실패 시 비정상 종료.

## §3. 대시보드 인사이트의 세션 이식

### 3a. 요약 카드(InsightStrip) 강화

- **DeltaChip 공용화**: `webui/src/components/dash/`의 DeltaChip(▲/▼/▬ +
  `betterUp` 방향색)을 공용 위치로 승격하고 InsightStrip 카드의 baseline
  delta 표기에 적용한다.
- **프로젝트 중앙값 대비 위치**: `/usage/baseline` 엔드포인트를 확장해
  5카드 지표별(캐시 적중률 · 과금 토큰 · 검증 통과율 · 도구 실패 수 · 추정
  비용) **프로젝트 스코프 중앙값 + 표본 수 n**을 내려준다. 카드에
  "프로젝트 중앙값의 x.x×" 위치와 n을 병기하고, n<3이면 "표본 부족"으로
  강조를 해제한다(대시보드 §0 표본 정직성 준용).
- **블렌디드 단가 부제**: 비용 카드 부제에 $/1M billed(세션 추정 비용 ÷
  과금 토큰, 프론트 파생 — 분모 0이면 `—`)를 추가한다.

### 3b. 검증 리듬 스트립 (GuardRhythm 세션판)

대시보드 `GuardRhythm`의 렌더를 세션 단위로 재사용해 **AnalysisPanel**에
추가한다: 세션 진행률(이벤트 순서 기준 %) 축 위에 검증 run의 outcome 점
(통과/실패/판정불가 — 대시보드와 동일 색)을 찍고, 점 클릭 시 해당 이벤트로
점프(`onSelectEvent`, §1.4 점프 규칙 적용). 데이터는 기존
`/v1/sessions/:id/verification-runs`를 재사용하며 신규 엔드포인트 없음.

### 3c. 변경 커버리지 세션판 (ChangeCoverage)

`GET /v1/verification/summary`에 **`session_id`** 파라미터를 추가해 단일
세션 스코프 집계를 지원하고, 이 세션의 커버/미커버 hunk 바(대시보드
`ChangeCoverage` 렌더 재사용, 미커버 앰버 강조)를 AnalysisPanel에 추가한다.
hunk 0건이면 섹션 자체에 `—`(미측정 ≠ 0)를 표기한다.

### 3d. 상세 패널 맥락화 (요청 메트릭 p50 배지)

- `SessionMetrics`에 **요청 메트릭 p50** 필드를 추가한다:
  `llm_request_p50 = { ttft_ms, duration_ms, output_tokens, cost_usd }`
  (api_request_log 상관 이벤트 전수 기준, 결정론 계산, 기존 메트릭 인메모리
  캐시 경로 편승, 미측정 항목은 null).
- DetailPanel의 LLM 요청 메트릭 행(TTFT·소요시간·출력 토큰·실측 비용)에
  "세션 중앙값의 x.x×" 배지를 표시한다. 표본(세션 내 측정 요청 수) n<3이면
  배지 대신 "표본 부족". 로드된 윈도우 근사가 아니라 백엔드 전수 계산이므로
  스크롤 상태와 무관하게 정확하다.

### 3e. 테스트

- Rust: baseline 중앙값+n(fixture 다세션), verification summary
  `session_id` 스코프, SessionMetrics p50(홀/짝 표본·미측정 null).
- TS: DeltaChip 이식 후 대시보드 기존 테스트 무회귀, 카드 중앙값 대비·표본
  부족 게이트, 블렌디드 단가 파생(분모 0 → `—`), GuardRhythm/ChangeCoverage
  세션판 렌더·점프, p50 배지. 새 `.tip` 키 tipStyle·parity.
- 브라우저 smoke 후 커밋.

## §4. 배제 (이번 범위 아님)

- 필터 조건 저장/프리셋, OR 조합 빌더 같은 고급 쿼리 UI.
- origin 분류의 백엔드 단일화(§1.3 이중화 해소) — 편차로만 기록.
- wimcc 런타임의 외부 가격 fetch(§2.4-6에서 명시적 금지).
- 상단 KPI 카드의 전면 개편(measured-cost KPI 교체 등 보류 항목) — 3a는
  기존 5카드의 문법 보강이지 카드 구성 변경이 아니다.

## §5. 구현 분할 (독립 PR 4개)

| PR | 내용 | 주요 지면 |
|---|---|---|
| PR-1 | §1 필터링 (API 확장 + FilterBar + flat 모드) | routes.rs, streamModel/ConversationStream, useSessionWindow |
| PR-2 | §2 비용 (pricing.json + /usage rates + 툴팁 + cron) | pricing.rs→json, dto.rs, insightCards.ts, workflows |
| PR-3 | §3a+3d (DeltaChip·중앙값 대비 + p50 배지) | baseline API, metrics.rs, InsightStrip, DetailPanel |
| PR-4 | §3b+3c (검증 리듬 + 변경 커버리지 세션판) | verification summary API, AnalysisPanel |

각 PR은 TDD red 우선, 개선 루프(untagged-bash 등) 실행, WebUI 변경 시
브라우저 smoke 후 커밋 — CLAUDE.md 게이트 공통 적용.

## §6. 결정 기록 (2026-07-04 사용자 확정)

- 한 스펙 + 영역별 독립 PR (단일 PR 통합·별도 스펙 3개 기각).
- 필터 4축 모두 채택, AND 조합, **숨김(제외)** 방식(흐림/목록 패널 기각),
  **서버측** 수행(클라이언트/하이브리드 기각 — SSE 백필과의 결합 문제없음
  확인 후 확정).
- 가격표 갱신은 **자동화 필수** — GH Actions 주간 cron(저장소 로컬 CC 훅
  기각: 주기 불규칙 + Non-goals 회색 지대).
- §3 이식 4건(요약 카드 강화·검증 리듬·변경 커버리지·상세 맥락화) 모두 채택.
