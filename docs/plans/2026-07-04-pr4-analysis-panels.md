# PR-4 — 분석 패널 검증 이식 (검증 리듬 + 변경 커버리지 세션판) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 스펙 `docs/specs/2026-07-04-session-detail-improvements.md` §3b·§3c — 대시보드의 GuardRhythm·ChangeCoverage를 세션 단위로 이식해 AnalysisPanel에 검증 실행 리듬 스트립과 변경 커버리지 바를 추가한다.

**Architecture:** 백엔드는 `GET /v1/verification/summary`에 `session_id` 스코프를 추가(기존 집계 루프를 `aggregate()`로 추출해 재사용)한다. 프론트는 대시보드 컴포넌트에서 표현 계층 2개(RhythmStrip·CoverageBar)를 추출해 세션 분석 패널과 공유하고, 리듬 데이터는 기존 `/verification-runs` 응답(`trigger_event_id` 포함 — 점 클릭 점프에 사용)에서 클라이언트가 파생한다.

**Tech Stack:** Rust(axum·sqlx·axum_test) / React+TS(vitest·@testing-library/react) / i18n 카탈로그(en·ko)

**작업 브랜치:** `feat/session-analysis-verification` (main에서 분기 — main 직접 작업 금지)

## Global Constraints

- **TDD red 우선**: 모든 태스크는 실패하는 테스트를 먼저 작성하고 빨강을 확인한 뒤 구현한다. 테스트 없는 커밋 금지(doc-only 예외).
- **판정 문장 금지**: 새 카피는 숫자·관측 사실만. 섹션 제목은 서술형(질문형 금지).
- **미측정 ≠ 0**: 검증 run 0건·hunk 0건이면 값 자리에 `—` 표기(0%로 위장 금지).
- **outcome 색은 대시보드와 동일**: SSOT는 `webui/src/components/dash/echartsBase.ts`의 `OUTCOME_COLORS`(passed `#41c285` · failed `#ef4747` · unknown `#4a5162` · not_executed `#3d4351`). 커버리지 바는 커버 `#41c285`/미커버 앰버 `#f0b429`.
- **보라(#b07dff)는 코호트 경계 문법 전용** — 이 PR의 어떤 UI에도 쓰지 않는다.
- **새 `.tip` 키**: 강조 마크업 ≥1 + 120자 초과 시 `\n` 줄바꿈 + 긍정문(게이트: `webui/src/i18n/__tests__/tipStyle.test.ts`). en/ko 패리티(`parity.test.ts`).
- **커밋 메시지**: conventional commit. **AI footer(Co-Authored-By 등) 금지**(프로젝트 훅이 차단).
- **UI 변경은 브라우저 smoke 후 커밋**(Task 8). 운영 serve(:7878)는 **재시작 금지** — 스모크는 스크래치 serve(:7999, `--auto-migrate` 필수).
- 테스트 결과 확인은 exit code + 명시적 실패 grep("0 failed")로 — 요약만 보고 통과 단정 금지.

## 스펙 편차 (구현 전 확정)

1. **진행률 축은 시간 기준**: 스펙 §3b의 "(이벤트 순서 기준 %)"는 구현 불가 — 이벤트 서수는 클라이언트 윈도우 버퍼(최대 5000) 밖을 알 수 없다. 대시보드 rhythm의 확정 정의 `(started_at − session.first) / (last − first) × 100`(시간 기준, `tests/api_verification_summary.rs`가 SSOT, 대시보드 축 카피도 "시간 기준")를 그대로 재사용한다. Task 8에서 implementation-notes에 편차로 기록.
2. **ChangeCoverage 렌더는 바만 재사용**: 대시보드 컴포넌트는 다세션 목록(210px 라벨 열) 구조라 통째 재사용이 맞지 않다. 바 마크업만 `CoverageBar`로 추출해 공유한다.
3. **점프 규칙(§1.4 필터 해제)은 PR-1 소관**: 이 PR은 기존 `selectStreamCard`(=`sel.setSelectedNodeId`)만 호출한다. PR-1이 필터를 도입하면 그쪽에서 `selectStreamCard`에 해제 규칙을 넣는다 — 이 PR은 독립적으로 완결된다.

---

### Task 1: 백엔드 — verification summary `session_id` 스코프

**Files:**
- Modify: `src/insight/verification_summary.rs` (집계 루프를 `aggregate()`로 추출 + `collect_session()` 추가)
- Modify: `src/api/routes.rs:1207-1272` (`VerificationSummaryQuery`에 `session_id` + 결합 400 + 분기)
- Test: `tests/api_verification_summary.rs`

**Interfaces:**
- Consumes: `repo_observed::session_summary(pool, session_id) -> Result<Option<(i64, String, String)>>` (count, first, last — `src/db/repo_observed.rs:1071`, 기존 함수)
- Produces: `GET /v1/verification/summary?session_id=<sid>` → 기존 `VerificationSummary` DTO와 동일 형태(단일 세션 스코프). `session_id`×`project|from|to` 결합은 400. 미존재 세션은 전 필드 0/빈 배열의 정상 응답. Rust: `pub async fn collect_session(pool: &SqlitePool, session_id: &str) -> Result<VerificationSummary>`.

- [ ] **Step 1: 실패하는 테스트 3개 작성** — `tests/api_verification_summary.rs` 끝에 추가:

```rust
#[tokio::test]
async fn summary_session_scope_aggregates_single_session() {
    let server = TestServer::new(router(AppState::new_for_tests(seeded().await))).unwrap();
    let r = server
        .get("/v1/verification/summary?session_id=sess_va")
        .await;
    r.assert_status_ok();
    let v: Value = r.json();
    let d = &v["data"];
    // sess_va만: failed→passed(test) + build passed = 3 runs, hunk 2/3 covered.
    assert_eq!(d["total"], 3);
    assert_eq!(d["passed"], 2);
    assert_eq!(d["failed"], 1);
    assert_eq!(d["failures"]["recovered"], 1);
    assert_eq!(d["failures"]["abandoned"], 0);
    let rhythm = d["rhythm"].as_array().unwrap();
    assert_eq!(rhythm.len(), 1);
    assert_eq!(rhythm[0]["session_id"], "sess_va");
    assert_eq!(d["coverage"]["covered"], 2);
    assert_eq!(d["coverage"]["total"], 3);
}

#[tokio::test]
async fn summary_session_scope_rejects_window_params() {
    // session_id×project/from/to 결합은 계약상 미지원(400) — kind×around와 같은 스타일.
    let server = TestServer::new(router(AppState::new_for_tests(pool().await))).unwrap();
    for q in [
        "/v1/verification/summary?session_id=s&project=p",
        "/v1/verification/summary?session_id=s&from=2026-06-10T00:00:00%2B00:00",
        "/v1/verification/summary?session_id=s&to=2026-06-10T00:00:00%2B00:00",
    ] {
        let r = server.get(q).await;
        r.assert_status(axum::http::StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn summary_session_scope_unknown_session_is_empty() {
    let server = TestServer::new(router(AppState::new_for_tests(seeded().await))).unwrap();
    let r = server
        .get("/v1/verification/summary?session_id=nope")
        .await;
    r.assert_status_ok();
    let v: Value = r.json();
    assert_eq!(v["data"]["total"], 0);
    assert_eq!(v["data"]["coverage"]["total"], 0);
    assert_eq!(v["data"]["rhythm"].as_array().unwrap().len(), 0);
}
```

- [ ] **Step 2: 빨강 확인**

Run: `cargo test --test api_verification_summary 2>&1 | tail -20`
Expected: FAIL — 신규 3개 테스트가 404/미지원 파라미터 무시(session_id가 역직렬화엔 없으므로 전체 집계 반환 → assert 불일치)로 실패. 기존 3개는 계속 통과.

- [ ] **Step 3: `verification_summary.rs` 집계 추출 + `collect_session` 구현**

`collect()`의 세션 루프(줄 157-299의 본문)를 그대로 옮겨 내부 함수로 만들고, 두 진입점이 공유한다:

```rust
/// 집계 입력 — (session_id, first_observed_at, last_observed_at).
struct SessionSpan {
    session_id: String,
    first_observed_at: String,
    last_observed_at: String,
}

pub async fn collect(
    pool: &SqlitePool,
    project: Option<&str>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> Result<VerificationSummary> {
    let sessions = repo_observed::list_sessions_filtered(pool, CANDIDATE_CAP, project).await?;
    let in_window = |first: &str| -> bool {
        let Some(ts) = parse_ts(first) else {
            return false;
        };
        from.is_none_or(|f| ts >= f) && to.is_none_or(|t| ts <= t)
    };
    let matched: Vec<SessionSpan> = sessions
        .into_iter()
        .filter(|s| in_window(&s.first_observed_at))
        .map(|s| SessionSpan {
            session_id: s.session_id,
            first_observed_at: s.first_observed_at,
            last_observed_at: s.last_observed_at,
        })
        .collect();
    aggregate(pool, &matched).await
}

/// §3c — 단일 세션 스코프. 미존재 세션은 빈 집계(0/빈 배열)로 응답한다.
pub async fn collect_session(pool: &SqlitePool, session_id: &str) -> Result<VerificationSummary> {
    let matched: Vec<SessionSpan> = match repo_observed::session_summary(pool, session_id).await? {
        Some((_count, first, last)) => vec![SessionSpan {
            session_id: session_id.to_string(),
            first_observed_at: first,
            last_observed_at: last,
        }],
        None => Vec::new(),
    };
    aggregate(pool, &matched).await
}

async fn aggregate(pool: &SqlitePool, matched: &[SessionSpan]) -> Result<VerificationSummary> {
    // …기존 collect의 mut 누적자 선언부터 Ok(VerificationSummary{…})까지 그대로 이동.
    // 루프는 `for s in matched {`가 되고 s.session_id / s.first_observed_at /
    // s.last_observed_at 필드명이 동일하므로 본문 수정 없음.
}
```

`use crate::db::{…, repo_observed, …}`는 기존 import에 이미 있다.

- [ ] **Step 4: `routes.rs` 핸들러 분기**

`VerificationSummaryQuery`에 필드 추가:

```rust
#[derive(serde::Deserialize)]
pub struct VerificationSummaryQuery {
    pub project: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    /// §3c — 단일 세션 스코프. project/from/to와 결합 불가(400).
    pub session_id: Option<String>,
}
```

`verification_summary` 핸들러 본문 맨 앞(파라미터 파싱 전)에 분기 추가:

```rust
    if let Some(sid) = q.session_id.as_deref() {
        if q.project.is_some() || q.from.is_some() || q.to.is_some() {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "type": "about:blank",
                    "title": "INVALID_QUERY",
                    "detail": "session_id cannot be combined with project/from/to",
                })),
            )
                .into_response();
        }
        return match crate::insight::verification_summary::collect_session(&pool, sid).await {
            Ok(summary) => Json(Envelope {
                meta: ResponseMeta::now(),
                data: summary,
            })
            .into_response(),
            Err(err) => {
                tracing::error!(err = %err, "verification_summary failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "internal server error"})),
                )
                    .into_response()
            }
        };
    }
```

- [ ] **Step 5: 통과 확인 (기존 3개 무회귀 포함)**

Run: `cargo test --test api_verification_summary 2>&1 | tail -5`
Expected: `test result: ok. 6 passed; 0 failed`

- [ ] **Step 6: 전체 백엔드 게이트**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test 2>&1 | grep -E "test result|FAILED" | tail -20`
Expected: fmt/clippy 무경고, 모든 test result 줄이 `0 failed`.

- [ ] **Step 7: 커밋**

```bash
git add src/insight/verification_summary.rs src/api/routes.rs tests/api_verification_summary.rs
git commit -m "feat(api): verification summary에 session_id 스코프 추가"
```

---

### Task 2: 클라이언트 — `getVerificationSummary` session_id + 세션 훅

**Files:**
- Modify: `webui/src/api/client.ts:151-162` (`getVerificationSummary` opts에 `session_id`)
- Modify: `webui/src/lib/queries.ts` (sessionKeys + `useSessionVerificationSummaryQuery`)
- Test: `webui/src/api/__tests__/client.verificationSummary.test.ts` (신규)

**Interfaces:**
- Consumes: Task 1의 `GET /v1/verification/summary?session_id=…`
- Produces: `getVerificationSummary(opts: { project?: string; from?: string; to?: string; session_id?: string }): Promise<VerificationSummaryDto>` · `useSessionVerificationSummaryQuery(id: string, opts?: QOpts<VerificationSummaryDto>)` (queryKey `['session', id, 'verification-summary']`)

- [ ] **Step 1: 실패하는 테스트 작성** — `webui/src/api/__tests__/client.verificationSummary.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from 'vitest';
import { getVerificationSummary } from '../client';

describe('getVerificationSummary', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('session_id를 쿼리 파라미터로 전달한다', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ meta: { generated_at: 'x' }, data: { total: 0 } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );
    vi.stubGlobal('fetch', fetchMock);
    await getVerificationSummary({ session_id: 'sess_1' });
    const url = String(fetchMock.mock.calls[0][0]);
    expect(url).toContain('/v1/verification/summary?session_id=sess_1');
  });
});
```

- [ ] **Step 2: 빨강 확인**

Run: `cd webui && npx vitest run src/api/__tests__/client.verificationSummary.test.ts 2>&1 | tail -8`
Expected: FAIL — `session_id`가 opts 타입에 없어 tsc/URL 미포함으로 expect 불일치.
(참고: 기존 `jsonGet`이 fetch 기본 URL·envelope 언랩을 어떻게 하는지 `client.ts` 상단과 어긋나면 mock Response 형태를 그 구현에 맞춘다 — envelope `{meta,data}` 언랩은 확인됨.)

- [ ] **Step 3: 구현**

`client.ts`:

```ts
export function getVerificationSummary(opts: {
  project?: string;
  from?: string;
  to?: string;
  /** §3c — 단일 세션 스코프(project/from/to와 결합 불가, 서버가 400). */
  session_id?: string;
}): Promise<VerificationSummaryDto> {
  const p = new URLSearchParams();
  if (opts.session_id) p.set('session_id', opts.session_id);
  if (opts.project) p.set('project', opts.project);
  if (opts.from) p.set('from', opts.from);
  if (opts.to) p.set('to', opts.to);
  const qs = p.toString();
  return jsonGet<VerificationSummaryDto>(`/v1/verification/summary${qs ? `?${qs}` : ''}`);
}
```

`queries.ts` — `sessionKeys`에 추가:

```ts
  verificationSummary: (id: string) => ['session', id, 'verification-summary'] as const,
```

훅 추가(파일 내 다른 훅 옆, `getVerificationSummary`·`VerificationSummaryDto` import 추가):

```ts
/** §3c — 세션 스코프 verification summary(변경 커버리지). 분석 패널 lazy. */
export function useSessionVerificationSummaryQuery(
  id: string,
  opts?: QOpts<VerificationSummaryDto>,
) {
  return useQuery<VerificationSummaryDto>({
    queryKey: sessionKeys.verificationSummary(id),
    queryFn: () => getVerificationSummary({ session_id: id }),
    enabled: !!id,
    ...opts,
  });
}
```

- [ ] **Step 4: 통과 확인**

Run: `cd webui && npx vitest run src/api/__tests__/client.verificationSummary.test.ts 2>&1 | tail -5`
Expected: `1 passed`

- [ ] **Step 5: 커밋**

```bash
git add webui/src/api/client.ts webui/src/lib/queries.ts webui/src/api/__tests__/client.verificationSummary.test.ts
git commit -m "feat(webui): verification summary 세션 스코프 클라이언트와 훅"
```

---

### Task 3: `RhythmStrip` 추출 (대시보드 무회귀)

**Files:**
- Create: `webui/src/components/dash/RhythmStrip.tsx`
- Modify: `webui/src/components/dash/GuardRhythm.tsx` (점 트랙을 RhythmStrip으로 교체)
- Test: `webui/src/components/dash/__tests__/RhythmStrip.test.tsx` (신규) + 기존 `verification.test.tsx` 무회귀

**Interfaces:**
- Produces: `RhythmStrip({ runs, onRunClick }: { runs: Array<{ pct: number; status: string }>; onRunClick?: (index: number) => void })` — `[data-dot]` 점 스트립. `onRunClick` 있으면 점이 `<button>`이 된다. export type `RhythmStripRun = { pct: number; status: string }`.

- [ ] **Step 1: 실패하는 테스트 작성** — `webui/src/components/dash/__tests__/RhythmStrip.test.tsx`:

```tsx
import { fireEvent, render } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, expect, it, vi } from 'vitest';
import { RhythmStrip } from '../RhythmStrip';

const runs = [
  { pct: 25, status: 'failed' },
  { pct: 75, status: 'passed' },
];

describe('RhythmStrip', () => {
  it('점 수 = runs 수, 위치는 pct, 색은 status(OUTCOME_COLORS)', () => {
    render(<RhythmStrip runs={runs} />);
    const dots = document.querySelectorAll('[data-dot]');
    expect(dots).toHaveLength(2);
    expect((dots[0] as HTMLElement).style.left).toBe('25%');
    // OUTCOME_COLORS.failed = #ef4747
    expect((dots[0] as HTMLElement).style.background).toBe('rgb(239, 71, 71)');
  });

  it('onRunClick가 있으면 점이 버튼이 되고 인덱스로 콜백한다', () => {
    const onClick = vi.fn();
    render(<RhythmStrip runs={runs} onRunClick={onClick} />);
    const dots = document.querySelectorAll('button[data-dot]');
    expect(dots).toHaveLength(2);
    fireEvent.click(dots[1]);
    expect(onClick).toHaveBeenCalledWith(1);
  });
});
```

- [ ] **Step 2: 빨강 확인**

Run: `cd webui && npx vitest run src/components/dash/__tests__/RhythmStrip.test.tsx 2>&1 | tail -5`
Expected: FAIL — `Cannot find module '../RhythmStrip'`

- [ ] **Step 3: `RhythmStrip.tsx` 작성**

```tsx
/** 진행률(0–100%) 축 위 outcome 점 스트립 — 대시보드 GuardRhythm과 세션
 *  분석 패널(검증 리듬)이 공유하는 표현 계층. 색 SSOT: OUTCOME_COLORS. */
import { OUTCOME_COLORS } from './echartsBase';

const DOT: Record<string, string> = {
  passed: OUTCOME_COLORS.passed,
  failed: OUTCOME_COLORS.failed,
  unknown: OUTCOME_COLORS.unknown,
  not_executed: OUTCOME_COLORS.not_executed,
};

export type RhythmStripRun = { pct: number; status: string };

export function RhythmStrip({
  runs,
  onRunClick,
}: {
  runs: RhythmStripRun[];
  /** 점 클릭 — 세션판이 trigger 이벤트 점프에 쓴다. 없으면 정적 렌더. */
  onRunClick?: (index: number) => void;
}) {
  const dotClass = 'absolute top-[5px] h-4 w-2 -translate-x-1 rounded-[2.5px]';
  return (
    <div className="relative h-[26px] min-w-0 flex-1 rounded-md bg-(--wimcc-surface-2)">
      {runs.map((run, i) => {
        const style = {
          left: `${run.pct}%`,
          background: DOT[run.status] ?? OUTCOME_COLORS.unknown,
        };
        const title = `${run.pct}% · ${run.status}`;
        return onRunClick ? (
          <button
            key={i}
            type="button"
            data-dot
            title={title}
            className={`${dotClass} cursor-pointer border-0 p-0`}
            style={style}
            onClick={() => onRunClick(i)}
          />
        ) : (
          <b key={i} data-dot title={title} className={dotClass} style={style} />
        );
      })}
    </div>
  );
}
```

- [ ] **Step 4: `GuardRhythm.tsx` 교체**

`DOT` 상수와 `OUTCOME_COLORS` import를 제거하고, 트랙 div(줄 49-59)를 교체:

```tsx
import { RhythmStrip } from './RhythmStrip';
```

```tsx
            <RhythmStrip runs={r.runs} />
```

(주변 flex 행·210px 라벨 열·% 축 라벨 행은 그대로.)

- [ ] **Step 5: 신규 + 무회귀 확인**

Run: `cd webui && npx vitest run src/components/dash/__tests__/RhythmStrip.test.tsx src/components/dash/__tests__/verification.test.tsx 2>&1 | tail -6`
Expected: 전부 passed, `0 failed` (기존 verification.test.tsx의 `[data-dot]` 3개 assert 무회귀).

- [ ] **Step 6: 커밋**

```bash
git add webui/src/components/dash/RhythmStrip.tsx webui/src/components/dash/GuardRhythm.tsx webui/src/components/dash/__tests__/RhythmStrip.test.tsx
git commit -m "refactor(webui): GuardRhythm 점 스트립을 RhythmStrip으로 추출"
```

---

### Task 4: `CoverageBar` 추출 (대시보드 무회귀)

**Files:**
- Create: `webui/src/components/dash/CoverageBar.tsx`
- Modify: `webui/src/components/dash/ChangeCoverage.tsx` (바 마크업 교체)
- Test: `webui/src/components/dash/__tests__/CoverageBar.test.tsx` (신규) + 기존 `verification.test.tsx` 무회귀

**Interfaces:**
- Produces: `CoverageBar({ covered, total }: { covered: number; total: number })` — `[data-coverage-bar]` 루트, 커버 초록/미커버 앰버 2분할 바. `total === 0`이면 커버 폭 0%.

- [ ] **Step 1: 실패하는 테스트 작성** — `webui/src/components/dash/__tests__/CoverageBar.test.tsx`:

```tsx
import { render } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, expect, it } from 'vitest';
import { CoverageBar } from '../CoverageBar';

describe('CoverageBar', () => {
  it('커버 비율만큼 초록, 나머지 앰버 폭', () => {
    render(<CoverageBar covered={3} total={4} />);
    const bar = document.querySelector('[data-coverage-bar]') as HTMLElement;
    const [green, amber] = Array.from(bar.querySelectorAll('i')) as HTMLElement[];
    expect(green.style.width).toBe('75%');
    expect(amber.style.width).toBe('25%');
    expect(green.style.background).toBe('rgb(65, 194, 133)'); // #41c285
    expect(amber.style.background).toBe('rgb(240, 180, 41)'); // #f0b429
  });
});
```

- [ ] **Step 2: 빨강 확인**

Run: `cd webui && npx vitest run src/components/dash/__tests__/CoverageBar.test.tsx 2>&1 | tail -5`
Expected: FAIL — `Cannot find module '../CoverageBar'`

- [ ] **Step 3: `CoverageBar.tsx` 작성**

```tsx
/** 커버/미커버 hunk 2분할 바 — 대시보드 ChangeCoverage 행과 세션 분석
 *  패널이 공유. 커버 초록 #41c285 · 미커버 앰버 #f0b429(승인 목업 값). */
export function CoverageBar({ covered, total }: { covered: number; total: number }) {
  const pct = total > 0 ? Math.round((covered / total) * 100) : 0;
  return (
    <div
      data-coverage-bar
      className="flex h-[18px] min-w-0 flex-1 overflow-hidden rounded-[5px] bg-(--wimcc-surface-2)"
    >
      <i style={{ width: `${pct}%`, background: '#41c285', opacity: 0.9 }} />
      <i style={{ width: `${100 - pct}%`, background: '#f0b429', opacity: 0.75 }} />
    </div>
  );
}
```

- [ ] **Step 4: `ChangeCoverage.tsx` 교체**

세션 행의 바 div(줄 45-48)를 교체(그 행의 `pct` 계산은 우측 라벨에서 계속 쓴다):

```tsx
import { CoverageBar } from './CoverageBar';
```

```tsx
              <CoverageBar covered={s.covered} total={s.total} />
```

- [ ] **Step 5: 신규 + 무회귀 확인**

Run: `cd webui && npx vitest run src/components/dash/__tests__/CoverageBar.test.tsx src/components/dash/__tests__/verification.test.tsx 2>&1 | tail -6`
Expected: 전부 passed, `0 failed`.

- [ ] **Step 6: 커밋**

```bash
git add webui/src/components/dash/CoverageBar.tsx webui/src/components/dash/ChangeCoverage.tsx webui/src/components/dash/__tests__/CoverageBar.test.tsx
git commit -m "refactor(webui): ChangeCoverage 바를 CoverageBar로 추출"
```

---

### Task 5: AnalysisPanel — 검증 실행 리듬 섹션 (§3b)

**Files:**
- Modify: `webui/src/components/replay/analysis/AnalysisPanel.tsx`
- Modify: `webui/src/components/replay/analysis/AnalysisPanel.module.css`
- Modify: `webui/src/i18n/catalog/ko.ts`, `webui/src/i18n/catalog/en.ts`
- Test: `webui/src/components/replay/analysis/__tests__/AnalysisPanel.test.tsx`

**Interfaces:**
- Consumes: Task 3의 `RhythmStrip`. `VerificationRunDto`(`webui/src/api/types.ts:163` — `started_at`·`status`·`trigger_event_id` 사용).
- Produces: `AnalysisPanelProps`에 `verificationRuns?: VerificationRunDto[]`·`sessionSpan?: { first: string; last: string } | null` 추가. pct 정의는 백엔드 rhythm과 동일: `(started_at − first) / (last − first) × 100` 소수 1자리, span 0 → 50 (SSOT: `tests/api_verification_summary.rs`).

- [ ] **Step 1: 실패하는 테스트 작성** — `AnalysisPanel.test.tsx`에 추가 (파일 상단 import에 `fireEvent`, `vi`는 이미 있음; `VerificationRunDto` 타입 import 추가):

```tsx
import type { VerificationRunDto } from '../../../../api/types';

function mkRun(over: Partial<VerificationRunDto>): VerificationRunDto {
  return {
    verification_run_id: 'vr1',
    schema_version: 'verification_run.v1',
    session_id: 's1',
    source: 'bash',
    command: 'cargo test',
    command_kind: 'test_suite_rust',
    trigger_event_id: 'ev_t1',
    trigger_tool_use_id: null,
    status: 'passed',
    status_provenance: 'measured',
    detection_basis: 'known_tool',
    status_basis: 'exit',
    started_at: '2026-06-10T02:30:00+00:00',
    ended_at: null,
    exit_code: 0,
    failure_summary: null,
    covered_diff_hunk_ids: [],
    ...over,
  };
}

const SPAN = { first: '2026-06-10T00:00:00+00:00', last: '2026-06-10T10:00:00+00:00' };

describe('AnalysisPanel — 검증 리듬 (§3b)', () => {
  test('run을 시간 기준 pct 점으로 렌더한다 (02:30/10h → 25%)', () => {
    render(
      <AnalysisPanel
        metrics={m}
        verificationRuns={[
          mkRun({ verification_run_id: 'vr1', started_at: '2026-06-10T02:30:00+00:00', status: 'failed', trigger_event_id: 'ev_f' }),
          mkRun({ verification_run_id: 'vr2', started_at: '2026-06-10T05:00:00+00:00', status: 'passed', trigger_event_id: 'ev_p' }),
        ]}
        sessionSpan={SPAN}
      />,
    );
    const dots = document.querySelectorAll('[data-dot]');
    expect(dots).toHaveLength(2);
    expect((dots[0] as HTMLElement).style.left).toBe('25%');
    expect((dots[1] as HTMLElement).style.left).toBe('50%');
  });

  test('점 클릭 → onSelectEvent(trigger_event_id)', () => {
    const onSelect = vi.fn();
    render(
      <AnalysisPanel
        metrics={m}
        verificationRuns={[mkRun({ trigger_event_id: 'ev_jump' })]}
        sessionSpan={SPAN}
        onSelectEvent={onSelect}
      />,
    );
    fireEvent.click(document.querySelector('button[data-dot]')!);
    expect(onSelect).toHaveBeenCalledWith('ev_jump');
  });

  test('run 0건이면 리듬 값 자리에 —', () => {
    render(<AnalysisPanel metrics={m} verificationRuns={[]} sessionSpan={SPAN} />);
    expect(screen.getByTestId('rhythm-empty')).toHaveTextContent('—');
  });
});
```

- [ ] **Step 2: 빨강 확인**

Run: `cd webui && npx vitest run src/components/replay/analysis/__tests__/AnalysisPanel.test.tsx 2>&1 | tail -8`
Expected: FAIL — 신규 3개 실패(props 미존재·data-dot 0개), 기존 테스트는 통과.

- [ ] **Step 3: i18n 키 추가** — `ko.ts`의 `analysis.*` 블록(줄 95-104 인근)에:

```ts
  'analysis.rhythm.title': '검증 실행 리듬 — 세션 진행률 위 실행 위치',
  'analysis.rhythm.meta': (a: { g: number; p: number }) => `가드 ${a.g} · 통과 ${a.p}`,
  'analysis.rhythm.tip':
    '**x = 세션 진행률(시간 기준)**, 점 하나가 검증 실행 하나이고 색이 결과입니다.\n' +
    '점을 클릭하면 그 실행을 유발한 이벤트 카드로 이동합니다.',
```

`en.ts`의 대응 블록(줄 97-106 인근)에:

```ts
  'analysis.rhythm.title': 'Verification rhythm — run positions over session progress',
  'analysis.rhythm.meta': (a: { g: number; p: number }) => `${a.g} guards · ${a.p} passed`,
  'analysis.rhythm.tip':
    '**x = session progress (time-based)**; each dot is one verification run and its color is the outcome.\n' +
    'Click a dot to jump to the event card that triggered the run.',
```

- [ ] **Step 4: `AnalysisPanel.tsx` 구현**

import 추가:

```tsx
import type { SessionMetricsDto, SignalDto, EvidenceRef, VerificationRunDto } from '../../../api/types';
import { InfoTip } from '../insight-strip/InfoTip';
import { RhythmStrip } from '../../dash/RhythmStrip';
```

props 확장:

```tsx
interface AnalysisPanelProps {
  metrics: SessionMetricsDto | null;
  signals?: SignalDto[];
  /** §3b 검증 리듬 — 세션 run 목록(기존 /verification-runs 재사용). */
  verificationRuns?: VerificationRunDto[];
  /** 세션 시간 범위(first/last observed_at) — pct 분모. */
  sessionSpan?: { first: string; last: string } | null;
  onSelectEvent?: (eventId: string) => void;
  'data-testid'?: string;
}
```

파생 함수(컴포넌트 밖, 파일 하단 helper들 옆):

```tsx
/** 백엔드 rhythm pct와 동일 정의: (started_at−first)/(last−first)×100,
 *  소수 1자리, span 0 → 50 (SSOT: tests/api_verification_summary.rs). */
function rhythmRunsOf(
  runs: VerificationRunDto[],
  span: { first: string; last: string },
): Array<{ pct: number; status: string; eventId: string }> {
  const a = Date.parse(span.first);
  const b = Date.parse(span.last);
  if (Number.isNaN(a) || Number.isNaN(b)) return [];
  const ms = b - a;
  return runs
    .filter((r) => !Number.isNaN(Date.parse(r.started_at)))
    .sort((x, y) => x.started_at.localeCompare(y.started_at))
    .map((r) => ({
      pct:
        ms > 0
          ? Math.min(100, Math.max(0, Math.round(((Date.parse(r.started_at) - a) / ms) * 1000) / 10))
          : 50,
      status: r.status,
      eventId: r.trigger_event_id,
    }));
}
```

컴포넌트 본문에서 파생(`if (!metrics)` 가드 앞):

```tsx
  const rhythmRuns = useMemo(
    () => (verificationRuns && sessionSpan ? rhythmRunsOf(verificationRuns, sessionSpan) : []),
    [verificationRuns, sessionSpan],
  );
```

렌더 — 기존 지표 테이블 div와 디텍터 분포 div 사이에 섹션 추가:

```tsx
      {/* --- 검증 실행 리듬 (§3b) — 진행률은 시간 기준(대시보드 rhythm과 동일) --- */}
      <div className={styles.detectorSection}>
        <div className={styles.sectionTitle}>
          {t('analysis.rhythm.title')}
          <InfoTip label={t('analysis.rhythm.title')} text={t('analysis.rhythm.tip')} />
        </div>
        {rhythmRuns.length === 0 ? (
          <p className={styles.noDetectors} data-testid="rhythm-empty">—</p>
        ) : (
          <>
            <div className={styles.rhythmMeta}>
              {t('analysis.rhythm.meta', {
                g: rhythmRuns.length,
                p: rhythmRuns.filter((r) => r.status === 'passed').length,
              })}
            </div>
            <RhythmStrip
              runs={rhythmRuns}
              onRunClick={(i) => {
                const eid = rhythmRuns[i]?.eventId;
                if (eid) onSelectEvent?.(eid);
              }}
            />
            <div className={styles.rhythmAxis}>
              <span>0%</span>
              <span>25%</span>
              <span>50%</span>
              <span>75%</span>
              <span>100%</span>
            </div>
          </>
        )}
      </div>
```

`AnalysisPanel.module.css`에 추가:

```css
.rhythmMeta {
  margin-bottom: 6px;
  font-family: ui-monospace, Menlo, monospace;
  font-size: 10.5px;
  color: var(--wimcc-fg-subtle);
}

.rhythmAxis {
  display: flex;
  justify-content: space-between;
  margin-top: 6px;
  font-family: ui-monospace, Menlo, monospace;
  font-size: 9.5px;
  color: var(--wimcc-fg-subtle);
}
```

- [ ] **Step 5: 통과 + 카피 게이트 확인**

Run: `cd webui && npx vitest run src/components/replay/analysis/__tests__/AnalysisPanel.test.tsx src/i18n/__tests__/tipStyle.test.ts src/i18n/__tests__/parity.test.ts 2>&1 | tail -6`
Expected: 전부 passed, `0 failed`.

- [ ] **Step 6: 커밋**

```bash
git add webui/src/components/replay/analysis/AnalysisPanel.tsx webui/src/components/replay/analysis/AnalysisPanel.module.css webui/src/components/replay/analysis/__tests__/AnalysisPanel.test.tsx webui/src/i18n/catalog/ko.ts webui/src/i18n/catalog/en.ts
git commit -m "feat(webui): 분석 패널에 검증 실행 리듬 스트립 추가"
```

---

### Task 6: AnalysisPanel — 변경 커버리지 섹션 (§3c)

**Files:**
- Modify: `webui/src/components/replay/analysis/AnalysisPanel.tsx`
- Modify: `webui/src/components/replay/analysis/AnalysisPanel.module.css`
- Modify: `webui/src/i18n/catalog/ko.ts`, `webui/src/i18n/catalog/en.ts`
- Test: `webui/src/components/replay/analysis/__tests__/AnalysisPanel.test.tsx`

**Interfaces:**
- Consumes: Task 4의 `CoverageBar`.
- Produces: `AnalysisPanelProps`에 `coverage?: { covered: number; total: number } | null` 추가 (Task 7이 `VerificationSummaryDto['coverage']`를 그대로 넘긴다 — 구조적 부분집합이라 호환).

- [ ] **Step 1: 실패하는 테스트 작성** — `AnalysisPanel.test.tsx`에 추가:

```tsx
describe('AnalysisPanel — 변경 커버리지 (§3c)', () => {
  test('coverage 바와 커버 %·미커버 수를 렌더한다', () => {
    render(<AnalysisPanel metrics={m} coverage={{ covered: 3, total: 4 }} />);
    const bar = document.querySelector('[data-coverage-bar]');
    expect(bar).not.toBeNull();
    // 주의: /75%/ 단독 매칭은 기존 검증률 75% 행과 중복돼 getByText가 throw한다.
    expect(screen.getByText(/커버 75% · 미커버 1|covered 75% · 1 uncovered/)).toBeInTheDocument();
  });

  test('hunk 0건이면 커버리지 값 자리에 — (0%로 위장 금지)', () => {
    render(<AnalysisPanel metrics={m} coverage={{ covered: 0, total: 0 }} />);
    expect(screen.getByTestId('coverage-empty')).toHaveTextContent('—');
    expect(document.querySelector('[data-coverage-bar]')).toBeNull();
  });

  test('coverage 미전달(로딩/미지원)에도 — 표기', () => {
    render(<AnalysisPanel metrics={m} />);
    expect(screen.getByTestId('coverage-empty')).toHaveTextContent('—');
  });
});
```

- [ ] **Step 2: 빨강 확인**

Run: `cd webui && npx vitest run src/components/replay/analysis/__tests__/AnalysisPanel.test.tsx 2>&1 | tail -8`
Expected: FAIL — 신규 3개 실패(`coverage` prop 미존재).

- [ ] **Step 3: i18n 키 추가** — `ko.ts`:

```ts
  'analysis.cov.title': '변경 커버리지 — 검증 통과가 거친 diff hunk',
  'analysis.cov.summary': (a: { pct: number; n: number }) => `커버 ${a.pct}% · 미커버 ${a.n}`,
  'analysis.cov.tip':
    '이 세션의 [green]커버[/green] / [amber]미커버[/amber] diff hunk 비율입니다.\n' +
    '**도입 이후 통과한 검증이 있는 hunk**만 커버로 셉니다 — 집계 기준은 서버 verification summary와 동일합니다.',
```

`en.ts`:

```ts
  'analysis.cov.title': 'Change coverage — diff hunks a passing verification ran after',
  'analysis.cov.summary': (a: { pct: number; n: number }) => `covered ${a.pct}% · ${a.n} uncovered`,
  'analysis.cov.tip':
    'The [green]covered[/green] / [amber]uncovered[/amber] diff hunk ratio of this session.\n' +
    'A hunk counts as covered only when **a passing verification ran after it was introduced** — same definition as the server verification summary.',
```

- [ ] **Step 4: `AnalysisPanel.tsx` 구현**

import 추가:

```tsx
import { CoverageBar } from '../../dash/CoverageBar';
```

props에 추가:

```tsx
  /** §3c 변경 커버리지 — summary?session_id 응답의 coverage 합계. */
  coverage?: { covered: number; total: number } | null;
```

리듬 섹션 바로 아래(디텍터 분포 앞)에 렌더 추가:

```tsx
      {/* --- 변경 커버리지 (§3c) — hunk 0건은 미측정(—), 0%가 아니다 --- */}
      <div className={styles.detectorSection}>
        <div className={styles.sectionTitle}>
          {t('analysis.cov.title')}
          <InfoTip label={t('analysis.cov.title')} text={t('analysis.cov.tip')} />
        </div>
        {!coverage || coverage.total === 0 ? (
          <p className={styles.noDetectors} data-testid="coverage-empty">—</p>
        ) : (
          <div className={styles.covRow}>
            <CoverageBar covered={coverage.covered} total={coverage.total} />
            <span className={styles.covLabel}>
              {t('analysis.cov.summary', {
                pct: Math.round((coverage.covered / coverage.total) * 100),
                n: coverage.total - coverage.covered,
              })}
            </span>
          </div>
        )}
      </div>
```

`AnalysisPanel.module.css`에 추가:

```css
.covRow {
  display: flex;
  align-items: center;
  gap: 10px;
}

.covLabel {
  flex: none;
  font-family: ui-monospace, Menlo, monospace;
  font-size: 11px;
  color: var(--wimcc-fg-muted);
}
```

- [ ] **Step 5: 통과 + 카피 게이트 확인**

Run: `cd webui && npx vitest run src/components/replay/analysis/__tests__/AnalysisPanel.test.tsx src/i18n/__tests__/tipStyle.test.ts src/i18n/__tests__/parity.test.ts 2>&1 | tail -6`
Expected: 전부 passed, `0 failed`.

- [ ] **Step 6: 커밋**

```bash
git add webui/src/components/replay/analysis/AnalysisPanel.tsx webui/src/components/replay/analysis/AnalysisPanel.module.css webui/src/components/replay/analysis/__tests__/AnalysisPanel.test.tsx webui/src/i18n/catalog/ko.ts webui/src/i18n/catalog/en.ts
git commit -m "feat(webui): 분석 패널에 변경 커버리지 섹션 추가"
```

---

### Task 7: SessionDetailPage 배선

**Files:**
- Modify: `webui/src/routes/SessionDetailPage.tsx` (훅 lazy fetch + AnalysisPanel props 4개 전달)

**Interfaces:**
- Consumes: Task 2 `useSessionVerificationSummaryQuery`, Task 5·6 `AnalysisPanelProps`. `verificationRuns`(줄 73)·`detail`(줄 62)은 기존 쿼리 재사용.
- Produces: 분석 패널 열림 시에만 summary fetch(`enabled: analysisOpen`), 점 클릭은 기존 `selectStreamCard` 경유.

- [ ] **Step 1: 훅 추가** — `metricsQuery`(줄 85) 옆:

```tsx
  const verificationSummary = useSessionVerificationSummaryQuery(sessionId, {
    enabled: analysisOpen && !!sessionId,
  });
```

import 목록(`useSessionMetricsQuery` 등이 있는 `../lib/queries` import)에 `useSessionVerificationSummaryQuery` 추가.

- [ ] **Step 2: AnalysisPanel props 전달** — 줄 428-433의 JSX 교체:

```tsx
              <AnalysisPanel
                metrics={metricsQuery.data ?? null}
                signals={signalsData}
                verificationRuns={verificationRuns.data}
                sessionSpan={
                  detail.data
                    ? {
                        first: detail.data.first_observed_at,
                        last: detail.data.last_observed_at,
                      }
                    : null
                }
                coverage={verificationSummary.data?.coverage ?? null}
                onSelectEvent={selectStreamCard}
                data-testid="analysis-panel"
              />
```

- [ ] **Step 3: 타입·전체 프론트 게이트**

Run: `cd webui && npx tsc --noEmit && npx vitest run 2>&1 | tail -6`
Expected: tsc 무오류, vitest 전체 `0 failed`. (`detail.data`의 `first_observed_at`/`last_observed_at`은 MetaStrip이 이미 소비하는 필드 — tsc가 어긋나면 `SessionDetail` 타입 정의를 확인해 실제 필드명에 맞춘다.)

- [ ] **Step 4: 커밋**

```bash
git add webui/src/routes/SessionDetailPage.tsx
git commit -m "feat(webui): 세션 상세 분석 패널에 검증 리듬·커버리지 배선"
```

---

### Task 8: 브라우저 smoke + 개선 루프 + implementation-notes

**Files:**
- Modify: `docs/implementation-notes.html` (append-only 원장 — 새 앵커 `#analysis-verification-panels-2026-07-04`)
- Modify: `docs/notes-index.md` (WebUI replay 토픽 행 갱신)
- (게이트 실패 시) Modify: `src/insight/event_tags.rs` 또는 `webui/scripts/tagging-gate-baseline.json`

- [ ] **Step 1: SPA + 바이너리 빌드**

Run: `cd webui && npm run build && cd .. && cargo build 2>&1 | tail -3`
Expected: 빌드 성공(embedded dist 갱신 — 재빌드 없이는 새 API가 스크래치 serve에 실리지 않는다).

- [ ] **Step 2: 스크래치 스택 기동** (운영 :7878 재시작 금지 — 라이브 CC 세션이 물려 있다)

```bash
SCRATCH=$(mktemp -d)/smoke.sqlite
./target/debug/wimcc --db-path "$SCRATCH" ingest --all
./target/debug/wimcc --db-path "$SCRATCH" serve --port 7999 --auto-migrate &
cd webui && WIMCC_PROXY_TARGET=http://127.0.0.1:7999 npx vite --port 5174 &
```

Expected: serve가 :7999에서 기동(`--auto-migrate` 누락 시 신규 테이블 부재가 런타임 WARN으로만 드러난다 — 반드시 포함), Vite가 :5174.

- [ ] **Step 3: API smoke**

Run: `curl -s "http://127.0.0.1:7999/v1/verification/summary?session_id=<검증 run이 있는 실제 세션 id>" | python3 -m json.tool | head -30`
(세션 id는 `curl -s http://127.0.0.1:7999/v1/sessions | python3 -c "import json,sys; [print(s['session_id']) for s in json.load(sys.stdin)['data'][:5]]"`로 고른다.)
Expected: 단일 세션 rhythm/coverage 집계 JSON.

- [ ] **Step 4: 브라우저 시각 검증** — `http://localhost:5174/sessions/<세션 id>`에서:
  1. "분석" 토글 → 검증 실행 리듬 스트립(점 색 = outcome)·변경 커버리지 바 렌더 확인.
  2. 리듬 점 클릭 → 스트림에서 해당 이벤트 카드 선택(점프) 확인.
  3. 검증 run 없는 세션에서 두 섹션이 `—` 표기 확인.
  4. InfoTip 2개(hover) — 마크업(굵게·색) 렌더 확인.
  5. 대시보드 검증 탭(:5174/dashboard) — GuardRhythm·ChangeCoverage 시각 무회귀 확인.

- [ ] **Step 5: 개선 루프 + 게이트** (재빌드된 바이너리 기준)

```bash
cd webui && node scripts/untagged-bash.ts --all
node scripts/unknown-verification.ts --all
node scripts/unidentified-plugins.ts --all
node scripts/tagging-gate.ts
```

Expected: tagging-gate exit 0. 실패 시 보편 항목은 사전 추가(TDD)·비보편은 `tagging-gate-baseline.json`에 사유와 함께 보류 커밋.

- [ ] **Step 6: implementation-notes 기록** — `docs/implementation-notes.html` 원장 끝에 앵커 `analysis-verification-panels-2026-07-04`로 추가: §3b/§3c 구현, **편차 — 진행률 축을 스펙의 "이벤트 순서 기준"에서 시간 기준으로 조정**(사유: 윈도우 버퍼 밖 이벤트 서수 미상, 대시보드 rhythm 정의 재사용), `session_id`×창 파라미터 400 계약, RhythmStrip/CoverageBar 추출. `docs/notes-index.md`의 "WebUI replay·목록" 행 현재 진실을 새 앵커로 갱신.

- [ ] **Step 7: 스모크 프로세스 정리 + 최종 게이트 + 커밋**

```bash
kill %1 %2 2>/dev/null
cargo fmt --check && cargo clippy --all-targets -- -D warnings
cargo test 2>&1 | grep "test result" | grep -v "0 failed"   # 출력이 비어야 통과(0 failed 부정 증명)
cd webui && npx vitest run 2>&1 | tail -3                    # exit 0 + "N passed" 확인
git add docs/implementation-notes.html docs/notes-index.md
git commit -m "docs(notes): 분석 패널 검증 이식 기록 — 진행률 축 편차 포함"
```

Expected: fmt/clippy 무경고, `grep -v "0 failed"` 출력 없음, vitest exit 0. 이후 PR 생성(제목 `feat(webui): 세션 분석 패널에 검증 리듬·변경 커버리지 이식`) — 병합은 사용자 몫(self-merge 금지, rebase 병합).
