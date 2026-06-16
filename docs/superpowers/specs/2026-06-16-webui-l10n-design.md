# WebUI l10n 설계 — ko/en (영어 폴백, 경량 타입드 카탈로그)

- 작성일: 2026-06-16
- 범위: **WebUI SPA만**. API·CLI는 영어 단일 언어 유지(현지화 대상 아님).
- 상태: 설계 승인됨 → 구현 계획 작성 대기.

## 1. 목표 / 결정 사항

| 항목 | 결정 |
|------|------|
| 지원 언어 | 한국어(`ko`), 영어(`en`) 2개 |
| 폴백 / 카탈로그 기준 언어 | **영어(`en`)** — source of truth |
| 언어 결정 | 자동감지(`navigator.language`) + 수동 토글, `localStorage` 지속 |
| 구현 방식 | 경량 타입드 카탈로그 + Context/Hook (런타임 의존성 0) |
| 대상 표면 | WebUI SPA의 화면 텍스트·`title`·`aria-label`·언어가 박힌 포맷터 |

### 비목표 (Non-goals)
- API 응답 텍스트·Rust CLI 출력 현지화 (영어 단일 유지).
- URL/쿼리 기반 locale 라우팅(`?lang=`, `/en/...`).
- `relativeTime` 단어 외의 날짜·숫자 locale 포맷 변경. 숫자/바이트/duration/USD 포맷터는 단위가 언어중립이라 그대로 둔다.
- 새 i18n 라이브러리 도입(react-i18next / react-intl). ~300개 문자열·2개 언어 규모엔 과함.

## 2. 현황 (탐색 결과)

- 기존 i18n 셋업 없음. UI 문자열은 한국어로 하드코딩.
- 코멘트 제외 **23개 `.tsx` 파일**에 사용자 노출 한국어 문자열(화면 텍스트·`title`·`aria-label`).
- 보간·카운트가 섞여 있음: `` `${rc}회` ``, `` `${count}건 동시` ``, `미측정 ${n}` 등. 영어는 복수형 처리 필요("1 time" vs "3 times").
- `main.tsx`가 Provider들(`QueryClientProvider`, `BrowserRouter`)을 마운트 — 여기에 `I18nProvider` 추가.
- 전역 네비는 `components/layout/AppShell.tsx`의 navRail.
- `lib/format.ts`: 숫자/바이트/duration/USD 포맷터는 단위가 언어중립. **단 `relativeTime`만 한국어("방금"/"N분 전"/"N시간 전"/"N일 전")가 박혀 있어 현지화 대상.** `formatModel`/`clockLabel` 등은 언어중립.

## 3. 아키텍처

### 3.1 파일 레이아웃
```
webui/src/i18n/
  index.ts            # 공개 API: I18nProvider, useT, useLocale, LOCALES, Locale, Messages 타입
  catalog/en.ts       # source of truth — 키 모양 + 영어 텍스트 정의, `export type Messages = typeof en`
  catalog/ko.ts       # `export const ko: Messages = {...}` — 한국어, 키 패리티를 타입으로 강제
  detect.ts           # detectLocale(): localStorage → navigator.language → 'en'
  t.ts                # createT(catalog, fallbackCatalog): 보간 + 함수형 메시지 처리
  __tests__/
    parity.test.ts    # ko 키셋 === en 키셋 (런타임, 누락/잉여 0)
    t.test.ts         # 보간, 함수형 메시지, 폴백
    detect.test.ts    # navigator / localStorage / default 해석
```

### 3.2 카탈로그 모양 & 메시지 타입
- **flat dotted 키**: 중첩 대신 평면 점표기 키 → `keyof typeof en`으로 직접 타이핑.
  ```ts
  // catalog/en.ts
  export const en = {
    'nav.sessions': 'Sessions',
    'workflow.jumpToCall': 'Jump to spawning Workflow call',
    'workflow.concurrent': (n: number) => `⟂ ${n} concurrent on main`,
    'analysis.toolFails': (n: number) => `${n} ${n === 1 ? 'time' : 'times'}`,
  } as const;
  export type Messages = typeof en;
  export type MessageKey = keyof Messages;
  ```
  ```ts
  // catalog/ko.ts
  import type { Messages } from './en';
  export const ko: Messages = {
    'nav.sessions': '세션',
    'workflow.jumpToCall': '이 워크플로우를 띄운 Workflow 호출로 이동',
    'workflow.concurrent': (n) => `⟂ main ${n}건 동시`,
    'analysis.toolFails': (n) => `${n}회`,
  };
  ```
- 메시지 값 = `string | ((params) => string)`. **함수형 메시지로 보간·영어 복수형을 파서 없이 처리**한다. ICU MessageFormat 같은 런타임 파서가 불필요.
- `ko: Messages` 타입 주석이 키 누락·오타·잉여·값 형태 불일치를 **컴파일 에러**로 만든다 → SSOT·token-precise 검증 기조와 일치.

### 3.3 `t()` 의미론
시그니처: `t(key: MessageKey, arg?: number | Record<string, string | number>)`. 단일 규칙으로 통일한다:

- 메시지 값이 **함수**면 → `message(arg)` 호출 결과를 반환. (카운트/복수형 메시지가 여기 해당. 예: `t('analysis.toolFails', 3)` → en `"3 times"`, ko `"3회"`.)
- 메시지 값이 **문자열**이고 `arg`가 객체면 → `{name}` 자리표시자를 `arg[name]`으로 regex 치환.
- 메시지 값이 **문자열**이고 `arg`가 없으면 → 그대로 반환. (단순 라벨. 예: `t('nav.sessions')` → `"Sessions"`.)
- **키 타입 안전이 주목표**이며 `arg`별 완전 타입추론은 gold-plating(YAGNI) — 위 단순 시그니처 채택.
- 폴백: 현재 locale 카탈로그에 값이 없으면 `en` 카탈로그 값 사용(타입 강제로 사실상 미발생, 방어용). dev에서 키 자체가 없으면 키 문자열 반환 + `console.warn`.

### 3.4 감지 & 지속
- `LOCALES = ['en', 'ko'] as const`, `DEFAULT_LOCALE = 'en'`, 저장 키 `'wimcc.lang'`.
- `detectLocale()`:
  1. `localStorage['wimcc.lang']`가 `LOCALES`에 속하면 그 값.
  2. 아니면 `navigator.language` 접두 — `ko`로 시작하면 `'ko'`, 그 외 `'en'`.
  3. 아니면 `'en'`.
- `setLocale(l)`: `localStorage['wimcc.lang'] = l` 기록 + context state 갱신 + `document.documentElement.lang = l` 설정.
- `I18nProvider` 마운트 시 `detectLocale()`로 초기화하고 `<html lang>`을 동기화.

### 3.5 공개 훅
- `useT()` → `t` 함수 반환.
- `useLocale()` → `{ locale, setLocale }` 반환.
- (분리 이유: 대부분 컴포넌트는 `t`만 필요. locale 토글 컴포넌트만 `setLocale` 필요.)

### 3.6 Provider 배치
`main.tsx`에서 `BrowserRouter` 바깥 또는 안쪽 어디든 무방하나, 라우팅/쿼리와 독립이므로 최상위에 가깝게 배치:
```tsx
<I18nProvider>
  <QueryClientProvider client={queryClient}>
    <BrowserRouter><App /></BrowserRouter>
  </QueryClientProvider>
</I18nProvider>
```

## 4. 언어 전환 UI

- `AppShell` navRail **하단**에 컴팩트한 `KO / EN` 세그먼트 토글(항상 노출, 현재 언어 강조 표시).
- 클릭 시 `setLocale` 호출. 버튼 `aria-label`도 현지화(예: en에서 "Switch to Korean").
- 정확한 위치·스타일은 구현 중 **브라우저 smoke**로 미세조정(navRail 하단 vs TopBar는 smoke 결과로 확정).

## 5. 포맷 정책 (`lib/format.ts`)

- `formatMs` / `formatUsd` / `formatTokens` / `formatPct` / `formatBytes` / `formatModel` / `clockLabel`: **변경 없음**(단위 ms/s/k/M/KiB·USD·HH:MM이 언어중립).
- `relativeTime(iso, nowMs)` → `relativeTime(iso, nowMs, locale: Locale)`로 시그니처 확장.
  - en: `just now` / `N min ago` / `N hr ago` / `N days ago` / `YYYY-MM-DD`(30일 초과).
  - ko: 기존 `방금` / `N분 전` / `N시간 전` / `N일 전` / `YYYY-MM-DD`.
  - **단어 로직은 format.ts 내부에 인라인 유지**(임계값과 강하게 결합). i18n Provider/catalog에 의존하지 않아 format ↔ i18n 순환 의존을 피하고 순수 함수로 테스트 가능. 호출부(`SessionListPage` 등)에서 `useLocale()`의 `locale`을 주입한다.

## 6. 테스트 전략 (TDD red 우선)

각 테스트는 **실패를 먼저 확인**한 뒤 구현한다. doc 변경만 예외.

1. `parity.test.ts` — `Object.keys(ko)`와 `Object.keys(en)` 집합 일치(누락/잉여 0). (타입 강제의 런타임 안전망.)
2. `t.test.ts` — 문자열 보간(`{name}` 치환), 함수형 메시지 호출, 미존재 키 폴백/경고.
3. `detect.test.ts` — `localStorage` 우선 / `navigator.language` 접두 매핑(`ko-KR`→ko, `en-US`→en, `fr`→en) / 기본값.
4. `relativeTime` — locale별 단어 출력(en/ko 각각), 경계(방금/분/시간/일/날짜).
5. 컴포넌트 렌더 테스트 1~2개 — `I18nProvider`로 `locale='en'` 강제 후 영어 텍스트가 보이는지 확인(회귀 방지 표본).

## 7. 추출·마이그레이션 계획 (작업의 대부분)

23개 파일. 그룹별로 incremental하게:

| 단계 | 내용 |
|------|------|
| ① 코어 | `i18n/` (Provider, catalog en/ko 스켈레톤, t, detect) + 테스트. `main.tsx`에 Provider 마운트. |
| ② 토글 | navRail 토글 컴포넌트 + 테스트 + 브라우저 smoke. |
| ③ 포맷 | `relativeTime` locale화 + 테스트 + 호출부 수정. |
| ④ 컴포넌트 그룹 | layout → replay/stream → replay/analysis → replay/detail → insight-strip → routes 순. |

각 그룹 절차: 테스트 작성/조정(red) → 한국어 리터럴을 `t()`로 치환 → `vitest` → **브라우저 smoke**(`wimcc serve` + claude-in-chrome 시각 검증) → incremental commit(conventional commit).

## 8. 트레이드오프 / 열린 질문

- **함수형 메시지 vs 문자열+보간**: 함수형은 영어 복수형/단위를 파서 없이 처리하나 카탈로그가 코드(번역 외주 어려움). 2개 언어·개발자 유지 전제라 수용. 단순 라벨은 문자열로, 카운트/복수형만 함수형으로 — 혼용.
- **키 인자 타입 안전**: 키는 완전 타입 안전, 인자(params)는 단순 시그니처. 필요 시 차후 per-key 제네릭으로 강화 가능(현재 YAGNI).
- **토글 위치**: navRail 하단을 기본안으로 하되 smoke 결과로 확정.
- **카탈로그 분할**: 초기엔 단일 `en.ts`/`ko.ts`. 파일이 커지면 namespace별 분할 검토(현재 ~300개라 단일로 충분).
