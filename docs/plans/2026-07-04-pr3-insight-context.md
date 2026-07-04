# PR-3 — 요약 카드 강화(§3a) + 상세 패널 맥락화(§3d) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 세션 상세 InsightStrip 5카드에 프로젝트 중앙값 대비 위치(DeltaChip + x.x× + 표본 n)를, DetailPanel LLM 요청 메트릭에 세션 p50 대비 배지를 붙인다.

**Architecture:** 백엔드 두 곳 확장 — ① `GET /v1/usage/baseline`이 `session_id` 파라미터로 그 세션의 프로젝트 스코프 분포(기존 4지표 + 검증 통과율·도구 실패 수·추정 비용, 각 지표에 표본 n)를 반환, ② `SessionMetrics`(on-demand, 인메모리 캐시)에 `llm_request_p50`(ttft/duration/output_tokens/cost_usd) 추가. 프론트는 대시보드 `DeltaChip`을 공용 컴포넌트로 추출해 InsightStrip 카드 foot에 적용하고, DetailPanel 요청 메트릭 행에 "세션 중앙값의 x.x×" 배지를 단다. 파생 로직은 전부 순수 함수(`insightCards.ts`·`p50Badge.ts`)로 두어 jsdom 없이 vitest로 잠근다.

**Tech Stack:** Rust(axum·sqlx·serde) + React/TypeScript(vite·vitest·TanStack Query) + i18n 카탈로그(en/ko).

**스펙:** `docs/specs/2026-07-04-session-detail-improvements.md` §3a·§3d·§3e·§4.

## Global Constraints

- **TDD red 우선**: 모든 태스크는 실패하는 테스트를 먼저 작성하고 빨강을 확인한 뒤 구현한다. test 후행/omit 커밋 금지.
- **표기 원칙**(대시보드 스펙 §0 계승): 판정 문장 금지(숫자·delta·관측 사실만) · 미측정은 `—`(0으로 위장 금지) · 표본 n 병기, **n<3이면 "표본 부족"으로 강조 해제** · 모델명은 전체 표시명 · 보라(#b07dff)는 코호트 경계 문법 전용(이 PR에서 사용 금지).
- **툴팁 카피 규칙**: 새 `.tip` i18n 키는 강조 마크업 ≥1 + 120자 초과 시 `\n` 분리 (`webui/src/i18n/__tests__/tipStyle.test.ts` 게이트). en/ko 키 패리티(`parity.test.ts`).
- **커밋**: conventional commit(한국어 제목 관례), **AI footer(Co-Authored-By 등) 금지**(프로젝트 hook이 차단), main 직접 작업 금지 — 이 PR 전용 브랜치에서만.
- **운영 serve(:7878) 재시작 금지** — 스모크는 스크래치 serve(:7999, `--auto-migrate` 필수).
- 명령 실행 위치: Rust는 저장소 루트, vitest/webui 스크립트는 `webui/`.
- Rust 테스트 검증은 exit code + 실패 부정 증명(`0 failed` 확인)으로 한다 — 요약 grep만으로 통과 주장 금지.

---

### Task 0: 브랜치 생성

**Files:** 없음 (git만)

- [ ] **Step 1: main 최신화 후 브랜치 생성**

```bash
cd /Users/bahamoth/projects/whats-in-my-cc
git checkout main && git pull
git checkout -b feat/pr3-insight-context
```

Expected: `Switched to a new branch 'feat/pr3-insight-context'`

---

### Task 1: DeltaChip 공용 컴포넌트 추출

`HeadlineStats.tsx` 내부 private 컴포넌트 `DeltaChip`(▲/▼/▬ + `betterUp` 방향색)을
`webui/src/components/DeltaChip.tsx`로 **그대로 이동**(마크업·클래스 불변 — 대시보드 무회귀)하고 export한다.

**Files:**
- Create: `webui/src/components/DeltaChip.tsx`
- Create: `webui/src/components/__tests__/DeltaChip.test.tsx`
- Modify: `webui/src/components/dash/HeadlineStats.tsx` (DeltaChip·trim1 정의 삭제, import로 대체)

**Interfaces:**
- Produces: `DeltaChip({ v: number | null; unit: string; betterUp: boolean; noCompare: string })` — React 컴포넌트. `v === null`이면 `noCompare` 텍스트, `|v| < 0.05`면 ▬(무채색), 그 외 ▲/▼(betterUp 방향이면 `#41c285`, 아니면 `#f0b429`).
- Produces: `trim1(v: number): string` — 소수 1자리 트리밍 포매터(export).

- [ ] **Step 1: 실패하는 테스트 작성**

`webui/src/components/__tests__/DeltaChip.test.tsx`:

```tsx
// PR-3 §3a — DeltaChip 공용화. 대시보드 HeadlineStats에서 추출한 계약을 잠근다:
// null → noCompare 텍스트, |v|<0.05 → ▬, 방향×betterUp → 초록/앰버.
import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { DeltaChip, trim1 } from '../DeltaChip';

describe('DeltaChip', () => {
  it('v가 null이면 noCompare 텍스트만 렌더한다', () => {
    render(<DeltaChip v={null} unit="%p" betterUp noCompare="비교 없음" />);
    expect(screen.getByText('비교 없음')).toBeInTheDocument();
  });

  it('|v| < 0.05는 ▬(무변화)로 렌더한다', () => {
    render(<DeltaChip v={0.01} unit="%p" betterUp noCompare="-" />);
    expect(screen.getByText(/▬/)).toBeInTheDocument();
  });

  it('상승이 좋은 지표(betterUp)의 +delta는 초록 계열 클래스를 얻는다', () => {
    render(<DeltaChip v={2.4} unit="%p" betterUp noCompare="-" />);
    const chip = screen.getByText(/▲ 2.4%p/);
    expect(chip.className).toContain('text-[#41c285]');
  });

  it('상승이 나쁜 지표(betterUp=false)의 +delta는 앰버 계열 클래스를 얻는다', () => {
    render(<DeltaChip v={1.2} unit="$" betterUp={false} noCompare="-" />);
    const chip = screen.getByText(/▲ 1.2\$/);
    expect(chip.className).toContain('text-[#f0b429]');
  });
});

describe('trim1', () => {
  it('소수 1자리로 반올림한다', () => {
    expect(trim1(1.26)).toBe('1.3');
    expect(trim1(2)).toBe('2');
  });
});
```

- [ ] **Step 2: 실패 확인**

```bash
cd webui && npx vitest run src/components/__tests__/DeltaChip.test.tsx
```

Expected: FAIL — `Cannot find module '../DeltaChip'`

- [ ] **Step 3: DeltaChip.tsx 생성 (HeadlineStats에서 코드 이동)**

`webui/src/components/DeltaChip.tsx`:

```tsx
/**
 * PR-3 §3a — 대시보드 HeadlineStats에서 추출한 공용 delta 칩.
 * ▲/▼/▬ + betterUp 방향색(좋음 #41c285 / 나쁨 #f0b429)으로 방향만 전달한다
 * (측정/판별 분리 — 판정 단어 없음). 마크업·클래스는 추출 전과 동일.
 */
export const trim1 = (v: number) => String(Math.round(v * 10) / 10);

export function DeltaChip({
  v,
  unit,
  betterUp,
  noCompare,
}: {
  v: number | null;
  unit: string;
  betterUp: boolean;
  noCompare: string;
}) {
  if (v === null)
    return <span className="text-[11px] text-(--wimcc-fg-subtle)">{noCompare}</span>;
  const flat = Math.abs(v) < 0.05;
  const good = v > 0 ? betterUp : !betterUp;
  const cls = flat
    ? 'text-(--wimcc-fg-subtle) bg-(--wimcc-surface-2)'
    : good
      ? 'text-[#41c285] bg-[#41c285]/10'
      : 'text-[#f0b429] bg-[#f0b429]/10';
  const arrow = flat ? '▬' : v > 0 ? '▲' : '▼';
  const num = flat ? '0.0' : `${trim1(Math.abs(v))}${unit}`;
  return (
    <span className={`rounded-[5px] px-1.5 py-0.5 font-mono text-[11.5px] whitespace-nowrap ${cls}`}>
      {arrow} {num}
    </span>
  );
}
```

- [ ] **Step 4: HeadlineStats.tsx에서 정의 제거·import 대체**

`webui/src/components/dash/HeadlineStats.tsx` 상단(1~41행)을 수정:
- `const trim1 = …`(10행)과 `function DeltaChip({…})`(14~41행) 정의를 삭제.
- import 추가: `import { DeltaChip, trim1 } from '../DeltaChip';`
- `money` 함수(11~12행)와 나머지는 그대로 유지 (money는 trim1을 쓰지 않으므로 무관).

- [ ] **Step 5: 전체 vitest로 무회귀 확인**

```bash
cd webui && npx vitest run
```

Expected: PASS, `0 failed` (대시보드 기존 테스트 포함 전부 초록)

- [ ] **Step 6: Commit**

```bash
git add webui/src/components/DeltaChip.tsx webui/src/components/__tests__/DeltaChip.test.tsx webui/src/components/dash/HeadlineStats.tsx
git commit -m "refactor(webui): DeltaChip을 공용 컴포넌트로 추출 — InsightStrip 재사용 준비 (PR-3 §3a)"
```

---

### Task 2: 백엔드 — `/v1/usage/baseline` 확장 (session_id 스코프 + 5지표 + n)

**Files:**
- Modify: `src/api/dto.rs:277-303` (`BaselineStat`에 `n`, `UsageBaselineDto`에 scope/project/신규 3지표)
- Modify: `src/api/routes.rs:898-944` (`usage_baseline` 핸들러 — `BaselineQuery` + 스코프 + 신규 지표)
- Modify: `src/db/repo_observed.rs` (`session_project` 단건 조회 추가)
- Test: `tests/api_usage_baseline.rs`

**Interfaces:**
- Consumes: `repo_usage_facet::per_session_metrics(&pool) -> Vec<SessionMetrics{session_id, cache_hit_ratio: Option<f64>, billed_tokens: i64, assistant_events: i64, output_tokens: i64}>` (기존), `repo_usage_facet::median_p25_p75(&[f64]) -> Option<Quantiles{p25, median, p75}>` (기존), `crate::insight::metrics::compute_session_metrics(&pool, &sid) -> insight::metrics::SessionMetrics` (기존, 인메모리 캐시), `repo_observed::list_sessions_filtered(&pool, cap, Option<&str /*project*/>)` (기존).
- Produces (JSON 계약):
  - `GET /v1/usage/baseline?session_id=<id>` — 그 세션의 `session_summary.project`로 스코프. project가 없으면(store 폴백) `scope:"store"`.
  - `BaselineStat = { p25, median, p75: number|null, n: i64 }` — **n = 그 지표 분포에 실제로 들어간 세션 수** (지표별 게이트 상이, 아래 주석 참조).
  - `UsageBaselineDto`에 추가: `scope: "project"|"store"`, `project: string|null`, `verification_pass_rate: BaselineStat`(게이트: passed+failed>0인 세션, 값=passed/(passed+failed)), `tool_failure_count: BaselineStat`(게이트: tool_call_total>0), `estimated_cost_usd: BaselineStat`(게이트: billed_tokens>0).
  - 기존 4지표(cache_hit_ratio·billed_tokens·assistant_events·output_tokens)는 스코프 내 usage 세션에서 종전과 동일하게 계산 + n 추가.
- Produces (Rust): `repo_observed::session_project(pool, session_id) -> Result<Option<String>>`.

- [ ] **Step 1: 실패하는 테스트 작성**

`tests/api_usage_baseline.rs`에 추가. 기존 helper `uf()`를 재사용하고, 프로젝트 스코프
검증을 위해 이벤트 seed helper를 이 파일에 추가한다(패턴: `tests/metrics_compute.rs`의
`seed_event` — raw insert 후 observed insert; **차이: `cwd` 필드를 채워
`upsert_session_summary`가 project를 파생하게 한다**):

```rust
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};
use wimcc::db::{repo_raw, repo_signal, repo_verification_run};
use wimcc::db::repo_signal::SignalRow;
use wimcc::db::repo_verification_run::VerificationRunRow;

/// observed_event 1건 seed (raw FK 포함) — cwd를 채워 project 파생을 만든다.
async fn seed_event_with_cwd(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    event_id: &str,
    kind: EventKind,
    cwd: &str,
) {
    let raw_id = format!("raw_{event_id}");
    repo_raw::insert_dedup(
        pool,
        &repo_raw::NewRaw {
            raw_event_id: raw_id.clone(),
            ingest_run_id: "run_baseline_test".into(),
            source_type: "claude_transcript".into(),
            source_uri: "/tmp/test.jsonl".into(),
            source_line_no: 0,
            source_byte_offset: 0,
            payload_sha256: format!("sha_{event_id}"),
            payload: b"{}".to_vec(),
            parse_error: None,
            captured_at: chrono::Utc::now(),
            redaction_state: "not_applicable".into(),
            redaction_manifest: None,
        },
    )
    .await
    .unwrap();
    let e = ObservedEvent {
        event_id: event_id.into(),
        raw_event_id: raw_id,
        schema_version: "observed_event.v1".into(),
        session_id: session_id.into(),
        observed_at: chrono::Utc::now(),
        actor: Actor::Assistant,
        kind,
        cwd: Some(cwd.into()),
        parser_version: "test@v0".into(),
        ..Default::default()
    };
    wimcc::db::repo_observed::insert(pool, &e).await.unwrap();
}
```

주의: `tests/metrics_compute.rs`의 seed helper가 `ingest_run` FK를 위해 `repo_runs`
로 run을 먼저 만든다면 같은 사전 seed를 복사한다(파일 상단 helper 확인 — 구현 시
metrics_compute.rs의 seed 절차를 그대로 따른다).

테스트 3건 추가:

```rust
#[tokio::test]
async fn baseline_stat_carries_sample_n() {
    let pool = empty_pool().await;
    repo_usage_facet::insert(&pool, &uf("r1", "s1", 100, 0, 0, 100)).await.unwrap();
    repo_usage_facet::insert(&pool, &uf("r2", "s2", 100, 0, 900, 300)).await.unwrap();

    let state = AppState::new_for_tests(pool);
    let server = TestServer::new(router(state)).unwrap();
    let body = server.get("/v1/usage/baseline").await.json::<Value>();
    let data = &body["data"];
    // usage 4지표: n = usage 세션 수. cache_hit_ratio도 두 세션 모두 분모>0이라 n=2.
    assert_eq!(data["billed_tokens"]["n"].as_i64().unwrap(), 2);
    assert_eq!(data["cache_hit_ratio"]["n"].as_i64().unwrap(), 2);
    // 파라미터 없음 → store 스코프.
    assert_eq!(data["scope"].as_str().unwrap(), "store");
    assert!(data["project"].is_null());
}

#[tokio::test]
async fn baseline_new_stats_gate_their_denominators() {
    let pool = empty_pool().await;
    // s1: 검증 passed 1 + failed 1 (측정 2), tool_call 2회 + tool_failure 시그널 1.
    repo_usage_facet::insert(&pool, &uf("r1", "s1", 100, 0, 0, 100)).await.unwrap();
    seed_event_with_cwd(&pool, "s1", "e1", EventKind::ToolCall, "/proj/a").await;
    seed_event_with_cwd(&pool, "s1", "e2", EventKind::ToolCall, "/proj/a").await;
    repo_signal::insert(&pool, &make_signal("s1", "sig1", "tool_failure")).await.unwrap();
    repo_verification_run::insert(&pool, &make_vrun("s1", "v1", "passed")).await.unwrap();
    repo_verification_run::insert(&pool, &make_vrun("s1", "v2", "failed")).await.unwrap();
    // s2: usage만 있음 — 검증 0건·tool_call 0건 → pass_rate/tool_failure 분포에서 제외.
    repo_usage_facet::insert(&pool, &uf("r2", "s2", 100, 0, 900, 300)).await.unwrap();

    let state = AppState::new_for_tests(pool);
    let server = TestServer::new(router(state)).unwrap();
    let body = server.get("/v1/usage/baseline").await.json::<Value>();
    let data = &body["data"];
    // pass_rate: s1만 측정(1/2=0.5), n=1.
    assert_eq!(data["verification_pass_rate"]["n"].as_i64().unwrap(), 1);
    assert!((data["verification_pass_rate"]["median"].as_f64().unwrap() - 0.5).abs() < 1e-9);
    // tool_failure_count: tool_call>0인 s1만, 값 1, n=1.
    assert_eq!(data["tool_failure_count"]["n"].as_i64().unwrap(), 1);
    assert_eq!(data["tool_failure_count"]["median"].as_f64().unwrap(), 1.0);
    // estimated_cost_usd: billed>0인 s1·s2, n=2, median>0 (opus-4-8 가격표 有).
    assert_eq!(data["estimated_cost_usd"]["n"].as_i64().unwrap(), 2);
    assert!(data["estimated_cost_usd"]["median"].as_f64().unwrap() > 0.0);
}

#[tokio::test]
async fn baseline_scopes_to_the_sessions_project() {
    let pool = empty_pool().await;
    // 프로젝트 A 세션 2개, 프로젝트 B 세션 1개 — usage 값이 뚜렷이 다름.
    repo_usage_facet::insert(&pool, &uf("r1", "sa1", 100, 0, 0, 100)).await.unwrap();  // billed 200
    repo_usage_facet::insert(&pool, &uf("r2", "sa2", 200, 0, 0, 200)).await.unwrap();  // billed 400
    repo_usage_facet::insert(&pool, &uf("r3", "sb1", 9000, 0, 0, 9000)).await.unwrap(); // billed 18000
    seed_event_with_cwd(&pool, "sa1", "ea1", EventKind::AssistantMessage, "/proj/a").await;
    seed_event_with_cwd(&pool, "sa2", "ea2", EventKind::AssistantMessage, "/proj/a").await;
    seed_event_with_cwd(&pool, "sb1", "eb1", EventKind::AssistantMessage, "/proj/b").await;
    for sid in ["sa1", "sa2", "sb1"] {
        wimcc::db::repo_observed::upsert_session_summary(&pool, sid).await.unwrap();
    }

    let state = AppState::new_for_tests(pool);
    let server = TestServer::new(router(state)).unwrap();
    let body = server
        .get("/v1/usage/baseline")
        .add_query_param("session_id", "sa1")
        .await
        .json::<Value>();
    let data = &body["data"];
    assert_eq!(data["scope"].as_str().unwrap(), "project");
    assert_eq!(data["project"].as_str().unwrap(), "/proj/a");
    // 프로젝트 A만: billed [200,400] → median 300 (B의 18000이 섞이면 400).
    assert_eq!(data["billed_tokens"]["median"].as_f64().unwrap(), 300.0);
    assert_eq!(data["session_count"].as_i64().unwrap(), 2);
}
```

(`make_signal`/`make_vrun`은 `tests/metrics_compute.rs`의 동명 helper를 이 파일로
복사 — 구현 시 필드가 다르면 그 파일 원본을 따른다.)

- [ ] **Step 2: 실패 확인**

```bash
cargo test --test api_usage_baseline 2>&1 | tail -20
```

Expected: FAIL — 컴파일 에러(`n`/`scope` 필드·`session_project` 부재) 또는 assertion 실패. `0 failed`가 아님을 확인.

- [ ] **Step 3: 구현**

3-1. `src/db/repo_observed.rs` 끝부분(파일 내 다른 pub fn들 근처)에 추가:

```rust
/// PR-3 §3a — baseline 스코프 해석용 단건 조회: session_summary facet의 project.
/// 행이 없거나 project가 NULL이면 None(→ store 폴백).
pub async fn session_project(pool: &SqlitePool, session_id: &str) -> Result<Option<String>> {
    use sqlx::Row as _;
    let row = sqlx::query("SELECT project FROM session_summary WHERE session_id = ?")
        .bind(session_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.and_then(|r| r.try_get::<Option<String>, _>("project").ok().flatten()))
}
```

3-2. `src/api/dto.rs` — `BaselineStat`에 n 추가, `UsageBaselineDto` 확장:

```rust
/// insight-redesign #6 + PR-3 §3a — one baseline metric's quantile triple.
/// `n` = 이 지표의 분포에 실제로 들어간 세션 수(지표별 게이트가 달라 서로 다를
/// 수 있다 — cache_hit은 분모>0, pass_rate는 측정>0, tool_failure는 tool_call>0,
/// cost는 billed>0). n<3이면 프론트가 "표본 부족"으로 강조를 해제한다.
#[derive(Serialize)]
pub struct BaselineStat {
    pub p25: Option<f64>,
    pub median: Option<f64>,
    pub p75: Option<f64>,
    pub n: i64,
}

/// PR-3 §3a — `?session_id=`가 오면 그 세션의 project(session_summary facet)로
/// 분포를 스코프한다. project 미상이면 store 전체로 폴백(scope="store").
#[derive(Serialize)]
pub struct UsageBaselineDto {
    pub session_count: i64,
    /// "project" | "store" — 프론트가 라벨을 정직하게 붙이기 위한 관측 사실.
    pub scope: String,
    pub project: Option<String>,
    pub cache_hit_ratio: BaselineStat,
    pub billed_tokens: BaselineStat,
    pub assistant_events: BaselineStat,
    pub output_tokens: BaselineStat,
    /// passed/(passed+failed) per session — 측정(passed+failed>0) 세션만.
    pub verification_pass_rate: BaselineStat,
    /// tool_failure 시그널 수 per session — tool_call_total>0 세션만(0-인플레 방지).
    pub tool_failure_count: BaselineStat,
    /// 공개 가격표 추정 비용 per session — billed_tokens>0 세션만.
    pub estimated_cost_usd: BaselineStat,
}
```

3-3. `src/api/routes.rs` — 핸들러 교체:

```rust
#[derive(Deserialize)]
pub struct BaselineQuery {
    pub session_id: Option<String>,
}

/// PR-3 §3a — 세션 횡단 baseline은 series와 같은 후보 cap을 쓴다(§G-3 준용).
const BASELINE_CANDIDATE_CAP: i64 = 5000;

pub async fn usage_baseline(
    State(pool): State<SqlitePool>,
    Query(q): Query<BaselineQuery>,
) -> impl IntoResponse {
    // 1) 스코프 해석: session_id → session_summary.project. 미상이면 store.
    let project = match q.session_id.as_deref() {
        Some(sid) => repo_observed::session_project(&pool, sid).await.expect("db"),
        None => None,
    };
    let rows =
        repo_observed::list_sessions_filtered(&pool, BASELINE_CANDIDATE_CAP, project.as_deref())
            .await
            .expect("db");
    let scope_ids: std::collections::HashSet<String> =
        rows.iter().map(|r| r.session_id.clone()).collect();

    // 2) usage 4지표 — 기존 per_session_metrics를 스코프로 필터(계산식 불변).
    let metrics: Vec<_> = repo_usage_facet::per_session_metrics(&pool)
        .await
        .expect("db")
        .into_iter()
        .filter(|m| scope_ids.contains(&m.session_id))
        .collect();
    let session_count = metrics.len() as i64;
    let cache_hit_vals: Vec<f64> = metrics.iter().filter_map(|m| m.cache_hit_ratio).collect();
    let billed_vals: Vec<f64> = metrics.iter().map(|m| m.billed_tokens as f64).collect();
    let assistant_events_vals: Vec<f64> =
        metrics.iter().map(|m| m.assistant_events as f64).collect();
    let output_vals: Vec<f64> = metrics.iter().map(|m| m.output_tokens as f64).collect();

    // 3) 신규 3지표 — 스코프 세션별 compute_session_metrics(인메모리 캐시 편승,
    //    series /v1/metrics와 같은 인터랙티브 경로 선례).
    let mut pass_rate_vals: Vec<f64> = Vec::new();
    let mut tool_failure_vals: Vec<f64> = Vec::new();
    let mut cost_vals: Vec<f64> = Vec::new();
    for r in &rows {
        let m = crate::insight::metrics::compute_session_metrics(&pool, &r.session_id)
            .await
            .expect("db");
        let measured = m.verification_passed + m.verification_failed;
        if measured > 0 {
            pass_rate_vals.push(m.verification_passed as f64 / measured as f64);
        }
        if m.tool_call_total > 0 {
            tool_failure_vals.push(m.tool_failure_count as f64);
        }
        let billed = m.input_tokens + m.cache_creation_input_tokens + m.output_tokens;
        if billed > 0 {
            cost_vals.push(m.estimated_cost_usd);
        }
    }

    fn stat(values: &[f64]) -> BaselineStat {
        match repo_usage_facet::median_p25_p75(values) {
            Some(qt) => BaselineStat {
                p25: Some(qt.p25),
                median: Some(qt.median),
                p75: Some(qt.p75),
                n: values.len() as i64,
            },
            None => BaselineStat { p25: None, median: None, p75: None, n: 0 },
        }
    }

    let data = UsageBaselineDto {
        session_count,
        scope: if project.is_some() { "project".into() } else { "store".into() },
        project,
        cache_hit_ratio: stat(&cache_hit_vals),
        billed_tokens: stat(&billed_vals),
        assistant_events: stat(&assistant_events_vals),
        output_tokens: stat(&output_vals),
        verification_pass_rate: stat(&pass_rate_vals),
        tool_failure_count: stat(&tool_failure_vals),
        estimated_cost_usd: stat(&cost_vals),
    };
    Json(Envelope { meta: ResponseMeta::now(), data })
}
```

주의: `Deserialize`가 routes.rs의 use에 이미 있는지 확인(다른 Query 구조체들이 쓰고
있음 — 1145행 `MetricsQuery` 참조). `list_sessions_filtered`가 반환하는 row 타입의
필드명은 `session_id`(repo_observed.rs:203) — 컴파일러가 이견을 내면 그 정의를 따른다.

- [ ] **Step 4: 통과 확인 (기존 2건 포함)**

기존 테스트 `baseline_endpoint_returns_median_across_sessions`는 이벤트 없이 usage만
seed하므로 `list_sessions_filtered`가 그 세션들을 반환하지 않을 수 있다(관측 이벤트
0건). **이 경우 scope 필터가 usage 세션을 걸러 기존 assert가 깨진다** — 깨지면
스코프 필터를 "scope_ids가 비어 있지 않을 때만 적용"이 아니라, **usage 4지표는
`project.is_some()`일 때만 scope 필터를 적용**하도록 수정한다(store 스코프 = 필터
없음 = 기존 동작 보존):

```rust
    let metrics: Vec<_> = repo_usage_facet::per_session_metrics(&pool)
        .await
        .expect("db")
        .into_iter()
        .filter(|m| project.is_none() || scope_ids.contains(&m.session_id))
        .collect();
```

(신규 3지표 루프는 `rows` 기반이라 store 스코프에서도 관측 이벤트가 있는 세션만
돈다 — usage-only 세션은 검증/도구/토큰 소스가 없으므로 값 기여가 없어 무해.)

```bash
cargo test --test api_usage_baseline 2>&1 | tail -5
```

Expected: `test result: ok. 5 passed; 0 failed`

- [ ] **Step 5: 전체 Rust 게이트**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test 2>&1 | tail -5
```

Expected: clippy 경고 0, 전체 `0 failed` (다른 통합 테스트가 UsageBaselineDto 필드를 참조하면 함께 수정)

- [ ] **Step 6: Commit**

```bash
git add src/api/dto.rs src/api/routes.rs src/db/repo_observed.rs tests/api_usage_baseline.rs
git commit -m "feat(api): usage/baseline에 session_id 프로젝트 스코프 + 검증·도구실패·비용 분포 + 표본 n (PR-3 §3a)"
```

---

### Task 3: 프론트 파생 로직 — InsightBaseline 개편 + 카드 비교 모델 + 블렌디드 단가

`insightCards.ts`의 baseline 처리를 "문자열 delta 1개"에서 "비교 모델(칩·위치·n·표본
부족)"로 교체하고, 5카드 전체로 확장한다. 렌더는 Task 4.

**Files:**
- Modify: `webui/src/components/replay/insight-strip/insightCards.ts`
- Modify: `webui/src/api/types.ts:246-262` (`BaselineStat`·`UsageBaselineDto` 동기화)
- Modify: `webui/src/i18n/catalog/ko.ts`, `webui/src/i18n/catalog/en.ts`
- Test: `webui/src/components/replay/insight-strip/__tests__/insightCards.test.ts`

**Interfaces:**
- Consumes: Task 2의 `UsageBaselineDto`(scope/project/7지표/`BaselineStat{p25,median,p75,n}`).
- Produces (Task 4·6이 사용):

```ts
export interface BaselineMedian { median: number | null; n: number }
export interface InsightBaseline {
  cache_hit_ratio?: BaselineMedian;
  billed_tokens?: BaselineMedian;
  verification_pass_rate?: BaselineMedian;
  tool_failure_count?: BaselineMedian;
  estimated_cost_usd?: BaselineMedian;
}
export interface BaselineComparison {
  /** DeltaChip 입력 (v는 이미 계산된 delta 수치) */
  chip: { v: number; unit: string; betterUp: boolean };
  /** "프로젝트 중앙값의 x.x×" 본문 — median>0일 때만 */
  position?: string;
  n: number;
  /** n<3 — 칩·위치 대신 "표본 부족 (n N)"만 표기 */
  lowSample: boolean;
}
// InsightCardModel: baselineDelta?: string 제거, baseline?: BaselineComparison 추가
export function toInsightBaseline(dto: UsageBaselineDto): InsightBaseline;
export function blendedRatePerMTok(costUsd: number, billedTokens: number): number | null;
```

- 카드별 chip 정의(모두 세션값 vs `median`):
  | 카드 | v | unit | betterUp |
  |---|---|---|---|
  | context | (ratio − median) × 100 | `%p` | true |
  | tokens | (billed/median − 1) × 100 | `%` | false |
  | verification | (rate − median) × 100 | `%p` | true |
  | tool_failure | count − median | `` (빈 문자열) | false |
  | cost | (cost/median − 1) × 100 | `%` | false |
- position: `t('insight.baselinePositionN', { x: (value/median).toFixed(1), n })`, median>0일 때만. 세션값이 미측정(`null`)인 카드는 baseline 자체를 붙이지 않는다.
- i18n 키 변경: `insight.baselineDeltaPp`/`insight.baselineDeltaPct` **제거**, 추가:
  - `insight.baselinePositionN`: ko `(p: {x: string; n: number}) => \`프로젝트 중앙값의 ${p.x}× · n ${p.n}\`` / en `` (p) => `${p.x}× project median · n ${p.n}` ``
  - `insight.baselineLowSample`: ko `(n: number) => \`표본 부족 (n ${n})\`` / en `` (n) => `low sample (n ${n})` ``
  - `insight.cost.detailUnitRate`: ko `(r: string) => \`블렌디드 $${r}/1M\`` / en `` (r) => `blended $${r}/1M` ``
  - `insight.cost.detailUnitRateNone`: ko `'블렌디드 —'` / en `'blended —'`

- [ ] **Step 1: 실패하는 테스트 작성**

`insightCards.test.ts`의 `describe('buildInsightCards — baseline delta …')` 블록
(280~289행)과 336행의 `'+100% vs 중앙값'` assert를 아래로 교체·확장:

```ts
describe('buildInsightCards — baseline comparison (PR-3 §3a)', () => {
  const baseline = {
    cache_hit_ratio: { median: 0.5, n: 12 },
    billed_tokens: { median: 1_000_000, n: 12 },
    verification_pass_rate: { median: 0.8, n: 5 },
    tool_failure_count: { median: 2, n: 12 },
    estimated_cost_usd: { median: 1.0, n: 12 },
  };

  it('context 카드: pp 칩 + 위치 + n', () => {
    // usage fixture의 cache_hit_ratio가 0.75라면 chip.v = 25(pp), position 1.5×.
    const c = byId({ ...EMPTY, usage, baseline }).get('context')!;
    expect(c.baseline).toBeDefined();
    expect(c.baseline!.chip.unit).toBe('%p');
    expect(c.baseline!.chip.betterUp).toBe(true);
    expect(c.baseline!.n).toBe(12);
    expect(c.baseline!.lowSample).toBe(false);
    expect(c.baseline!.position).toContain('×');
  });

  it('n<3이면 lowSample=true로 강조를 해제한다', () => {
    const low = { ...baseline, billed_tokens: { median: 1_000_000, n: 2 } };
    const c = byId({ ...EMPTY, usage, baseline: low }).get('tokens')!;
    expect(c.baseline!.lowSample).toBe(true);
  });

  it('median이 null이면 baseline을 붙이지 않는다', () => {
    const none = { ...baseline, estimated_cost_usd: { median: null, n: 0 } };
    const c = byId({ ...EMPTY, usage, baseline: none }).get('cost')!;
    expect(c.baseline).toBeUndefined();
  });

  it('baseline이 없으면 종전처럼 생략한다', () => {
    const c = byId({ ...EMPTY, usage }).get('context')!;
    expect(c.baseline).toBeUndefined();
  });

  it('tool_failure 카드: count−median 칩(단위 없음, 상승=앰버 방향)', () => {
    const sigs = [makeSignal('tool_failure'), makeSignal('tool_failure'), makeSignal('tool_failure')];
    const c = byId({ ...EMPTY, usage, signals: sigs, baseline }).get('tool_failure')!;
    expect(c.baseline!.chip.v).toBe(1); // 3 − 2
    expect(c.baseline!.chip.betterUp).toBe(false);
  });
});

describe('blendedRatePerMTok + 비용 카드 부제 (PR-3 §3a)', () => {
  it('비용/과금토큰 → $/1M', () => {
    expect(blendedRatePerMTok(2, 1_000_000)).toBe(2);
    expect(blendedRatePerMTok(1, 500_000)).toBe(2);
  });
  it('분모 0이면 null', () => {
    expect(blendedRatePerMTok(2, 0)).toBeNull();
  });
  it('비용 카드 detail에 블렌디드 단가가 병기된다', () => {
    const c = byId({ ...EMPTY, usage }).get('cost')!;
    expect(c.detail).toContain('블렌디드');
  });
});

describe('toInsightBaseline (PR-3 §3a)', () => {
  it('DTO 7지표 중 카드 5지표를 median+n으로 사상한다', () => {
    const dto = {
      session_count: 3, scope: 'project', project: '/p',
      cache_hit_ratio: { p25: 0.1, median: 0.5, p75: 0.9, n: 3 },
      billed_tokens: { p25: 1, median: 2, p75: 3, n: 3 },
      assistant_events: { p25: 1, median: 1, p75: 1, n: 3 },
      output_tokens: { p25: 1, median: 1, p75: 1, n: 3 },
      verification_pass_rate: { p25: null, median: null, p75: null, n: 0 },
      tool_failure_count: { p25: 0, median: 1, p75: 2, n: 3 },
      estimated_cost_usd: { p25: 0.5, median: 1, p75: 2, n: 3 },
    };
    const b = toInsightBaseline(dto);
    expect(b.cache_hit_ratio).toEqual({ median: 0.5, n: 3 });
    expect(b.verification_pass_rate).toEqual({ median: null, n: 0 });
  });
});
```

(existing helpers `byId`/`EMPTY`/`usage`/`makeSignal`은 파일 상단에 이미 있다 —
`makeSignal`이 없으면 그 파일의 기존 signal fixture 작성 방식을 그대로 따른다.
기존 baseline 테스트가 쓰던 `baseline: { cache_hit_ratio: 0.9 }` 스칼라 형태
호출부는 전부 새 `{ median, n }` 형태로 바꾼다 — 333행 포함.)

- [ ] **Step 2: 실패 확인**

```bash
cd webui && npx vitest run src/components/replay/insight-strip/__tests__/insightCards.test.ts
```

Expected: FAIL — `toInsightBaseline`/`blendedRatePerMTok` 미정의, `baseline` 형태 불일치.

- [ ] **Step 3: 구현**

3-1. `webui/src/api/types.ts` — Task 2 계약 동기화:

```ts
export type BaselineStat = {
  p25: number | null;
  median: number | null;
  p75: number | null;
  /** PR-3 — 이 지표 분포에 들어간 세션 수(지표별 게이트 상이). n<3 → 표본 부족. */
  n: number;
};

export type UsageBaselineDto = {
  session_count: number;
  /** "project" | "store" — session_id 스코프 해석 결과. */
  scope: string;
  project: string | null;
  cache_hit_ratio: BaselineStat;
  billed_tokens: BaselineStat;
  assistant_events: BaselineStat;
  output_tokens: BaselineStat;
  verification_pass_rate: BaselineStat;
  tool_failure_count: BaselineStat;
  estimated_cost_usd: BaselineStat;
};
```

3-2. `insightCards.ts`:

- `InsightBaseline`을 위 Interfaces 형태로 교체, `BaselineMedian`·`BaselineComparison`·`toInsightBaseline`·`blendedRatePerMTok` export 추가.
- `InsightCardModel`에서 `baselineDelta?: string` 삭제, `baseline?: BaselineComparison` 추가.
- 비교 빌더(파일 내 private):

```ts
function compareToBaseline(
  value: number,
  base: BaselineMedian | undefined,
  chip: { v: (value: number, median: number) => number; unit: string; betterUp: boolean },
  t: TFunction,
): BaselineComparison | undefined {
  if (!base || base.median === null) return undefined;
  if (base.n < 3) return { chip: { v: 0, unit: chip.unit, betterUp: chip.betterUp }, n: base.n, lowSample: true };
  const position =
    base.median > 0
      ? t('insight.baselinePositionN', { x: (value / base.median).toFixed(1), n: base.n })
      : undefined;
  return {
    chip: { v: chip.v(value, base.median), unit: chip.unit, betterUp: chip.betterUp },
    position,
    n: base.n,
    lowSample: false,
  };
}
```

- 각 카드 빌더에서 기존 `card.baselineDelta = …` 블록을 교체:
  - `contextCard`: `if (typeof ratio === 'number') card.baseline = compareToBaseline(ratio, inputs.baseline?.cache_hit_ratio, { v: (s, m) => (s - m) * 100, unit: '%p', betterUp: true }, t);`
  - `tokensCard`: `card.baseline = compareToBaseline(u.billed_tokens, inputs.baseline?.billed_tokens, { v: (s, m) => (m > 0 ? (s / m - 1) * 100 : 0), unit: '%', betterUp: false }, t);`
  - `verificationCard`: `measured > 0`일 때 `card.baseline = compareToBaseline(passed / measured, inputs.baseline?.verification_pass_rate, { v: (s, m) => (s - m) * 100, unit: '%p', betterUp: true }, t);` (카드 리턴 전에 변수로 조립하도록 리턴 객체를 `const card = …; …; return card;` 형태로 변경)
  - `toolFailureCard`: `card.baseline = compareToBaseline(failures.length, inputs.baseline?.tool_failure_count, { v: (s, m) => s - m, unit: '', betterUp: false }, t);` (동일하게 card 변수화)
  - `costCard`: `card.baseline = compareToBaseline(u.estimated_cost_usd, inputs.baseline?.estimated_cost_usd, { v: (s, m) => (m > 0 ? (s / m - 1) * 100 : 0), unit: '%', betterUp: false }, t);`
- `blendedRatePerMTok` + 비용 카드 detail 병기:

```ts
export function blendedRatePerMTok(costUsd: number, billedTokens: number): number | null {
  return billedTokens > 0 ? (costUsd / billedTokens) * 1_000_000 : null;
}
```

`costCard`의 `detail`을:

```ts
const rate = blendedRatePerMTok(u.estimated_cost_usd, u.billed_tokens);
const rateText =
  rate !== null ? t('insight.cost.detailUnitRate', rate.toFixed(2)) : t('insight.cost.detailUnitRateNone');
// detail: 기존 문구 + ' · ' + rateText
```

- `toInsightBaseline`:

```ts
export function toInsightBaseline(dto: UsageBaselineDto): InsightBaseline {
  const m = (s: BaselineStat): BaselineMedian => ({ median: s.median, n: s.n });
  return {
    cache_hit_ratio: m(dto.cache_hit_ratio),
    billed_tokens: m(dto.billed_tokens),
    verification_pass_rate: m(dto.verification_pass_rate),
    tool_failure_count: m(dto.tool_failure_count),
    estimated_cost_usd: m(dto.estimated_cost_usd),
  };
}
```

(import에 `UsageBaselineDto`·`BaselineStat` 추가.)

3-3. i18n 카탈로그 — ko.ts 38~39행의 `insight.baselineDeltaPp`/`Pct` 제거(en.ts 40~41행도)
하고 위 Interfaces 절의 4개 키를 같은 자리에 추가. `.tip` 키가 아니므로 tipStyle
게이트 비대상이지만 en/ko **동시** 추가(parity 게이트).

- [ ] **Step 4: 통과 확인**

```bash
cd webui && npx vitest run
```

Expected: PASS `0 failed` — insightCards + i18n parity + 기존 InsightStrip 테스트.
InsightStrip.test.tsx가 `baselineDelta`를 참조하면 이 태스크에서 함께 새 모델로 수정
(렌더 변경은 Task 4지만 타입 컴파일은 지금 맞춘다 — InsightStrip.tsx의
`card.baselineDelta` 참조(95~97행)를 임시로 `card.baseline?.position` 표시로 바꾸지
말고, **Task 4에서 한 번에** 바꾸도록 이 시점엔 렌더 블록을 삭제만 해 컴파일을 살린다).

- [ ] **Step 5: Commit**

```bash
git add webui/src/components/replay/insight-strip/insightCards.ts webui/src/api/types.ts webui/src/i18n/catalog/ko.ts webui/src/i18n/catalog/en.ts webui/src/components/replay/insight-strip/__tests__/insightCards.test.ts webui/src/components/replay/insight-strip/InsightStrip.tsx
git commit -m "feat(webui): InsightStrip baseline을 비교 모델(칩·위치·n·표본부족)로 개편 + 블렌디드 단가 파생 (PR-3 §3a)"
```

---

### Task 4: InsightStrip 렌더 + baseline 쿼리 세션 스코프 연결

**Files:**
- Modify: `webui/src/components/replay/insight-strip/InsightStrip.tsx` (cardFoot에 DeltaChip + 위치/표본부족)
- Modify: `webui/src/components/replay/insight-strip/InsightStrip.module.css` (위치 텍스트·표본부족 스타일)
- Modify: `webui/src/api/client.ts:111-114` (`getUsageBaseline(sessionId?)`)
- Modify: `webui/src/lib/queries.ts:156-160` (`useUsageBaselineQuery(sessionId?)` — queryKey에 sessionId 포함)
- Modify: `webui/src/routes/SessionDetailPage.tsx:75, 388-397` (세션 스코프 baseline + `toInsightBaseline` 매핑)
- Test: `webui/src/components/replay/insight-strip/__tests__/InsightStrip.test.tsx`

**Interfaces:**
- Consumes: Task 1 `DeltaChip`, Task 3 `BaselineComparison`·`toInsightBaseline`.
- Produces: `getUsageBaseline(sessionId?: string)` → `GET /v1/usage/baseline[?session_id=]`; `useUsageBaselineQuery(sessionId?: string)`.

- [ ] **Step 1: 실패하는 테스트 작성**

`InsightStrip.test.tsx`에 추가 (기존 렌더 헬퍼/프로바이더 래핑 방식을 그대로 따른다):

```tsx
it('baseline 비교가 있으면 DeltaChip과 위치·n을 렌더한다 (PR-3 §3a)', () => {
  renderStrip({
    usage,
    verificationRuns: [],
    signals: [],
    baseline: {
      cache_hit_ratio: { median: 0.5, n: 12 },
      billed_tokens: { median: 1_000_000, n: 12 },
    },
  });
  // 위치 텍스트("프로젝트 중앙값의 …× · n 12")가 카드 foot에 나타난다.
  expect(screen.getAllByText(/프로젝트 중앙값의 .+× · n 12/).length).toBeGreaterThan(0);
});

it('n<3이면 칩 대신 표본 부족을 렌더한다 (PR-3 §3a)', () => {
  renderStrip({
    usage,
    verificationRuns: [],
    signals: [],
    baseline: { billed_tokens: { median: 1_000_000, n: 2 } },
  });
  expect(screen.getByText(/표본 부족 \(n 2\)/)).toBeInTheDocument();
});
```

- [ ] **Step 2: 실패 확인**

```bash
cd webui && npx vitest run src/components/replay/insight-strip/__tests__/InsightStrip.test.tsx
```

Expected: FAIL — 위치 텍스트/표본 부족 미렌더.

- [ ] **Step 3: 구현**

3-1. `InsightStrip.tsx` — cardFoot 블록(93~98행)을 교체:

```tsx
import { DeltaChip } from '../../DeltaChip';
// …
<div className={styles.cardFoot}>
  <ProvenanceBadge provenance={card.provenance} />
  {card.baseline &&
    (card.baseline.lowSample ? (
      <span className={styles.baselineLow}>{t('insight.baselineLowSample', card.baseline.n)}</span>
    ) : (
      <>
        <DeltaChip
          v={card.baseline.chip.v}
          unit={card.baseline.chip.unit}
          betterUp={card.baseline.chip.betterUp}
          noCompare=""
        />
        {card.baseline.position && (
          <span className={styles.baselinePos}>{card.baseline.position}</span>
        )}
      </>
    ))}
</div>
```

3-2. `InsightStrip.module.css` — 기존 `.baselineDelta` 셀렉터를 대체:

```css
.baselinePos {
  font-size: 11px;
  color: var(--wimcc-fg-subtle);
  white-space: nowrap;
}
.baselineLow {
  font-size: 11px;
  color: var(--wimcc-fg-subtle);
}
```

3-3. `client.ts`:

```ts
export const getUsageBaseline = (sessionId?: string): Promise<UsageBaselineDto> =>
  jsonGet<UsageBaselineDto>(
    `/v1/usage/baseline${sessionId ? `?session_id=${encodeURIComponent(sessionId)}` : ''}`,
  );
```

3-4. `queries.ts`:

```ts
export function useUsageBaselineQuery(sessionId?: string, opts?: QOpts<UsageBaselineDto>) {
  return useQuery<UsageBaselineDto>({
    queryKey: [...usageKeys.baseline(), sessionId ?? 'store'],
    queryFn: () => getUsageBaseline(sessionId),
    ...(opts ?? {}),
  });
}
```

(기존 옵션 전달 형태는 파일의 다른 훅 관례를 따른다.)

3-5. `SessionDetailPage.tsx` — 75행을 `const baseline = useUsageBaselineQuery(sessionId);`로,
InsightStrip 전달부(388~397행)의 인라인 매핑을 `toInsightBaseline(baseline.data)`로 교체:

```tsx
baseline={baseline.data ? toInsightBaseline(baseline.data) : undefined}
```

(import에 `toInsightBaseline` 추가.)

- [ ] **Step 4: 통과 확인 + 다른 useUsageBaselineQuery 호출부 스캔**

```bash
cd webui && grep -rn "useUsageBaselineQuery" src/ && npx vitest run
```

Expected: 호출부는 SessionDetailPage 1곳(+ 테스트), vitest `0 failed`.

- [ ] **Step 5: Commit**

```bash
git add webui/src/components/replay/insight-strip/InsightStrip.tsx webui/src/components/replay/insight-strip/InsightStrip.module.css webui/src/api/client.ts webui/src/lib/queries.ts webui/src/routes/SessionDetailPage.tsx webui/src/components/replay/insight-strip/__tests__/InsightStrip.test.tsx
git commit -m "feat(webui): 요약 카드에 프로젝트 중앙값 대비 DeltaChip·위치·표본 n 표기 (PR-3 §3a)"
```

---

### Task 5: 백엔드 — `SessionMetrics.llm_request_p50` (+ 03 스펙 doc sync)

세션 내 LLM 요청 메트릭의 p50을 on-demand 메트릭에 추가한다. **소스는 두 갈래**
(프론트 `eventMetrics.ts` 검증 로직과 동일면):
- `otel_span` 이벤트 중 `telemetry.span_name == "claude_code.llm_request"` — flat
  `attributes`의 `ttft_ms`·`duration_ms`·`output_tokens`. request_id 당 **최초 1건**
  (프론트 `buildLlmMetricsFromEvents`의 first-match와 동일).
- `log_record` 이벤트 중 `payload.event_name == "api_request"` — `payload.attributes.cost_usd`
  (Claude Code 자체 보고 실측). request_id 당 최초 1건.

스펙 §3d의 "api_request_log 상관 이벤트 전수 기준"은 실제 소스 기준으로 위 두
갈래로 구체화된다(코드 현실 — timing/토큰은 span에만 있음). 편차로 기록.

**Files:**
- Modify: `src/insight/metrics.rs` (`P50Stat`·`LlmRequestP50` + 기존 이벤트 루프에 arm 추가)
- Modify: `webui/src/api/types.ts` (`SessionMetricsDto.llm_request_p50`)
- Modify: `docs/03_data_model_spec.html` (SessionMetrics JSON 예시 + counts-only 문구 각주)
- Test: `tests/metrics_compute.rs`

**Interfaces:**
- Produces (Rust, `src/insight/metrics.rs`):

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct P50Stat { pub p50: Option<f64>, pub n: i64 }

#[derive(Debug, Clone, serde::Serialize)]
pub struct LlmRequestP50 {
    pub ttft_ms: P50Stat,
    pub duration_ms: P50Stat,
    pub output_tokens: P50Stat,
    pub cost_usd: P50Stat,
}
// SessionMetrics에 필드 추가: pub llm_request_p50: LlmRequestP50
```

- Produces (TS, `types.ts`):

```ts
export type P50StatDto = { p50: number | null; n: number };
export type LlmRequestP50Dto = {
  ttft_ms: P50StatDto;
  duration_ms: P50StatDto;
  output_tokens: P50StatDto;
  cost_usd: P50StatDto;
};
// SessionMetricsDto에 llm_request_p50: LlmRequestP50Dto
```

- [ ] **Step 1: 실패하는 테스트 작성**

`tests/metrics_compute.rs`에 helper와 테스트 추가. 기존 `seed_event`와 같은 raw seed
절차를 쓰되 telemetry/payload/request_id를 받는 확장 helper:

```rust
use wimcc::model::observed::TelemetryFacet;

/// llm_request otel_span seed — telemetry facet의 flat attributes에 메트릭.
#[allow(clippy::too_many_arguments)]
async fn seed_llm_span(
    pool: &sqlx::SqlitePool,
    run_id: &str,
    session_id: &str,
    event_id: &str,
    rid: &str,
    ttft_ms: Option<f64>,
    duration_ms: f64,
    output_tokens: f64,
) {
    // raw seed는 seed_event와 동일 절차 (event_id로 raw_{id} 생성) — 그 코드를
    // seed_raw(pool, run_id, event_id) private helper로 추출해 둘 다 쓰게 한다.
    seed_raw(pool, run_id, event_id).await;
    let mut attrs = serde_json::json!({
        "request_id": rid,
        "duration_ms": duration_ms,
        "output_tokens": output_tokens,
    });
    if let Some(t) = ttft_ms {
        attrs["ttft_ms"] = serde_json::json!(t);
    }
    let e = ObservedEvent {
        event_id: event_id.into(),
        raw_event_id: format!("raw_{event_id}"),
        schema_version: "observed_event.v1".into(),
        session_id: session_id.into(),
        observed_at: chrono::Utc::now(),
        actor: Actor::Assistant,
        kind: EventKind::OtelSpan,
        request_id: Some(rid.into()),
        telemetry: Some(TelemetryFacet {
            span_name: "claude_code.llm_request".into(),
            attributes: attrs,
            ..Default::default()
        }),
        parser_version: "test@v0".into(),
        ..Default::default()
    };
    repo_observed::insert(pool, &e).await.unwrap();
}

/// api_request log_record seed — payload.attributes.cost_usd.
async fn seed_api_request_log(
    pool: &sqlx::SqlitePool,
    run_id: &str,
    session_id: &str,
    event_id: &str,
    rid: &str,
    cost_usd: f64,
) {
    seed_raw(pool, run_id, event_id).await;
    let e = ObservedEvent {
        event_id: event_id.into(),
        raw_event_id: format!("raw_{event_id}"),
        schema_version: "observed_event.v1".into(),
        session_id: session_id.into(),
        observed_at: chrono::Utc::now(),
        actor: Actor::Assistant,
        kind: EventKind::LogRecord,
        request_id: Some(rid.into()),
        payload: serde_json::json!({
            "event_name": "api_request",
            "attributes": { "request_id": rid, "cost_usd": cost_usd }
        }),
        parser_version: "test@v0".into(),
        ..Default::default()
    };
    repo_observed::insert(pool, &e).await.unwrap();
}
```

테스트 3건:

```rust
#[tokio::test]
async fn llm_request_p50_odd_sample_is_middle() {
    let pool = test_pool().await;
    seed_run(&pool, "run1").await; // 기존 run seed 관례를 따른다
    seed_llm_span(&pool, "run1", "s", "sp1", "r1", Some(100.0), 1000.0, 10.0).await;
    seed_llm_span(&pool, "run1", "s", "sp2", "r2", Some(300.0), 3000.0, 30.0).await;
    seed_llm_span(&pool, "run1", "s", "sp3", "r3", Some(200.0), 2000.0, 20.0).await;
    let m = compute_session_metrics(&pool, "s").await.unwrap();
    assert_eq!(m.llm_request_p50.ttft_ms.n, 3);
    assert_eq!(m.llm_request_p50.ttft_ms.p50, Some(200.0));
    assert_eq!(m.llm_request_p50.duration_ms.p50, Some(2000.0));
    assert_eq!(m.llm_request_p50.output_tokens.p50, Some(20.0));
    // cost 로그가 없으므로 미측정 = null (0 위장 금지).
    assert_eq!(m.llm_request_p50.cost_usd.n, 0);
    assert_eq!(m.llm_request_p50.cost_usd.p50, None);
}

#[tokio::test]
async fn llm_request_p50_even_sample_interpolates_and_dedups_by_request_id() {
    let pool = test_pool().await;
    seed_run(&pool, "run1").await;
    seed_llm_span(&pool, "run1", "s", "sp1", "r1", None, 1000.0, 10.0).await;
    seed_llm_span(&pool, "run1", "s", "sp2", "r2", None, 3000.0, 30.0).await;
    // 같은 request_id 중복 span — 최초 1건만 세어야 한다.
    seed_llm_span(&pool, "run1", "s", "sp3", "r2", None, 9999.0, 99.0).await;
    seed_api_request_log(&pool, "run1", "s", "lg1", "r1", 0.40).await;
    seed_api_request_log(&pool, "run1", "s", "lg2", "r2", 0.60).await;
    let m = compute_session_metrics(&pool, "s").await.unwrap();
    assert_eq!(m.llm_request_p50.duration_ms.n, 2);
    assert_eq!(m.llm_request_p50.duration_ms.p50, Some(2000.0)); // (1000+3000)/2
    // ttft 미제공 → n=0, null.
    assert_eq!(m.llm_request_p50.ttft_ms.n, 0);
    assert_eq!(m.llm_request_p50.ttft_ms.p50, None);
    assert_eq!(m.llm_request_p50.cost_usd.n, 2);
    assert!((m.llm_request_p50.cost_usd.p50.unwrap() - 0.50).abs() < 1e-9);
}

#[tokio::test]
async fn llm_request_p50_empty_session_is_all_null() {
    let pool = test_pool().await;
    let m = compute_session_metrics(&pool, "none").await.unwrap();
    assert_eq!(m.llm_request_p50.ttft_ms.n, 0);
    assert_eq!(m.llm_request_p50.cost_usd.p50, None);
}
```

(`seed_run`은 이 파일에서 run FK를 seed하는 기존 방식 이름에 맞춘다 — 없으면
기존 테스트가 run을 어떻게 만드는지 보고 동일하게. `seed_raw` 추출 시 기존
`seed_event`도 그걸 쓰도록 리팩터.)

- [ ] **Step 2: 실패 확인**

```bash
cargo test --test metrics_compute 2>&1 | tail -10
```

Expected: FAIL — `llm_request_p50` 필드 부재 컴파일 에러.

- [ ] **Step 3: 구현 (`src/insight/metrics.rs`)**

- 구조체 2개 추가(Interfaces 절 코드 그대로) + `SessionMetrics`에 `pub llm_request_p50: LlmRequestP50` (필드 주석: "§3d — 세션 내 LLM 요청 p50. count가 아닌 분포 통계로, 전수 이벤트에서만 계산 가능해 소비자 재계산이 불가하므로 예외적으로 서버가 반환한다(2026-07-04 세션 상세 개선 스펙). 미측정은 null.").
- flat attribute 숫자 파서(프론트 `num()`과 동일 관용 — 숫자 또는 숫자 문자열):

```rust
fn num_attr(attrs: &serde_json::Value, key: &str) -> Option<f64> {
    let v = attrs.get(key)?;
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
}
```

- `compute_session_metrics_uncached`의 기존 `for e in &events` 루프 match에 arm 추가
  (request_id 당 최초 1건 — `entry().or_insert…`; **사전 확인**: `repo_observed::list_session`의
  ORDER BY가 observed_at 오름차순인지 SQL을 확인하고, 아니면 dedup 전에 정렬):

```rust
    // §3d — LLM 요청 p50 수집 버퍼 (루프 앞에 선언)
    let mut span_seen: std::collections::BTreeMap<String, (Option<f64>, Option<f64>, Option<f64>)> =
        std::collections::BTreeMap::new(); // rid → (ttft, duration, output)
    let mut cost_seen: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
```

```rust
            EventKind::OtelSpan => {
                let Some(tf) = &e.telemetry else { continue };
                if tf.span_name != "claude_code.llm_request" {
                    continue;
                }
                let rid = tf
                    .attributes
                    .get("request_id")
                    .and_then(|v| v.as_str())
                    .or_else(|| tf.attributes.get("gen_ai.response.id").and_then(|v| v.as_str()));
                let Some(rid) = rid else { continue };
                span_seen.entry(rid.to_string()).or_insert_with(|| {
                    (
                        num_attr(&tf.attributes, "ttft_ms"),
                        num_attr(&tf.attributes, "duration_ms"),
                        num_attr(&tf.attributes, "output_tokens"),
                    )
                });
            }
            EventKind::LogRecord => {
                if e.payload.pointer("/event_name").and_then(|v| v.as_str()) != Some("api_request") {
                    continue;
                }
                let Some(attrs) = e.payload.pointer("/attributes") else { continue };
                let Some(rid) = attrs.get("request_id").and_then(|v| v.as_str()) else { continue };
                if let Some(c) = num_attr(attrs, "cost_usd") {
                    cost_seen.entry(rid.to_string()).or_insert(c);
                }
            }
```

- 루프 뒤 p50 조립(중앙값은 기존 `repo_usage_facet::median_p25_p75` 재사용 — DRY):

```rust
    fn p50_stat(values: Vec<f64>) -> P50Stat {
        let n = values.len() as i64;
        let p50 = crate::db::repo_usage_facet::median_p25_p75(&values).map(|q| q.median);
        P50Stat { p50, n }
    }
    let llm_request_p50 = LlmRequestP50 {
        ttft_ms: p50_stat(span_seen.values().filter_map(|v| v.0).collect()),
        duration_ms: p50_stat(span_seen.values().filter_map(|v| v.1).collect()),
        output_tokens: p50_stat(span_seen.values().filter_map(|v| v.2).collect()),
        cost_usd: p50_stat(cost_seen.values().copied().collect()),
    };
```

- `Ok(SessionMetrics { …, llm_request_p50, … })`에 포함.

- [ ] **Step 4: 통과 + 전체 게이트**

```bash
cargo test --test metrics_compute 2>&1 | tail -5
cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test 2>&1 | tail -5
```

Expected: 신규 3건 포함 `0 failed`. (`tests/metrics_compute.rs`·MCP digest 등 SessionMetrics를 구조체 리터럴로 만드는 다른 테스트가 있으면 새 필드 추가로 컴파일이 깨진다 — 함께 수정.)

- [ ] **Step 5: TS 타입 + 03 스펙 doc sync**

- `webui/src/api/types.ts`: Interfaces 절의 `P50StatDto`/`LlmRequestP50Dto` 추가, `SessionMetricsDto`에 `llm_request_p50: LlmRequestP50Dto;` 추가(주석: `/** §3d — 세션 내 LLM 요청 p50(전수 계산). p50=null이면 미측정. n<3이면 배지 대신 표본 부족. */`).
- `docs/03_data_model_spec.html`의 SessionMetrics JSON 예시(§ "SessionMetrics — on-demand")에 아래 줄을 추가하고, "합성 가능한 count만 반환한다" 문장에 각주를 덧붙인다: "예외: `llm_request_p50`은 분포 통계(p50)다 — 전수 이벤트 없이는 소비자가 재계산할 수 없어 서버가 반환한다(2026-07-04 세션 상세 개선 스펙 §3d)."

```json
"llm_request_p50": {                 // §3d — 세션 내 LLM 요청 p50 (전수, 미측정 null)
  "ttft_ms":       { "p50": 812.0,  "n": 41 },
  "duration_ms":   { "p50": 7250.0, "n": 41 },
  "output_tokens": { "p50": 380.0,  "n": 41 },
  "cost_usd":      { "p50": 0.041,  "n": 38 }
}
```

- [ ] **Step 6: Commit**

```bash
git add src/insight/metrics.rs tests/metrics_compute.rs webui/src/api/types.ts docs/03_data_model_spec.html
git commit -m "feat(insight): SessionMetrics에 llm_request_p50(ttft·duration·output·cost) 추가 (PR-3 §3d)"
```

---

### Task 6: 프론트 — DetailPanel 요청 메트릭 p50 배지

**Files:**
- Create: `webui/src/components/replay/detail/p50Badge.ts`
- Create: `webui/src/components/replay/detail/__tests__/p50Badge.test.ts`
- Modify: `webui/src/components/replay/detail/metricsRows.tsx` (Row `badge` prop + ResponseMetricsRows `p50` prop)
- Modify: `webui/src/components/replay/detail/metricsRows.module.css`
- Modify: `webui/src/components/replay/detail/EntityMetricsPanel.tsx` (prop 전달)
- Modify: `webui/src/components/replay/detail/InsightTab.tsx` (prop 전달)
- Modify: `webui/src/components/replay/detail/DetailPanel.tsx` (prop 전달)
- Modify: `webui/src/routes/SessionDetailPage.tsx` (metrics 쿼리 enable 조건 + prop)
- Modify: `webui/src/i18n/catalog/ko.ts`, `webui/src/i18n/catalog/en.ts`
- Test: `webui/src/components/replay/detail/EntityMetricsPanel.test.tsx`

**Interfaces:**
- Consumes: Task 5 `LlmRequestP50Dto`/`P50StatDto`, `useSessionMetricsQuery`(기존).
- Produces:

```ts
// p50Badge.ts
export type P50Badge = { text: string; lowSample: boolean };
export function p50Badge(
  value: number | null,
  stat: P50StatDto | undefined,
  t: TFunction,
): P50Badge | null;
// 규칙: value null 또는 stat 없음 → null(배지 없음) · stat.n < 3 → 표본 부족 배지
//       · p50 null/≤0 → null · 그 외 "세션 중앙값의 (value/p50).toFixed(1)×"
```

- `Row`에 `badge?: P50Badge | null` prop 추가; `ResponseMetricsRows`에 `p50?: LlmRequestP50Dto | null` prop 추가 — duration/ttft/outputTokens/billedCost 4행에만 배지.
- prop 체인: `SessionDetailPage(metricsQuery.data?.llm_request_p50 ?? null)` → `DetailPanel llmP50` → `InsightTab llmP50` → `EntityMetricsPanel llmP50` → `ResponseMetricsRows p50`. `ResponseMetricsPanel`(thinking 마커 표면)은 prop 미전달 → 배지 없음(범위 밖, 선택 이벤트가 아님).
- i18n: `metric.badge.median`: ko `(x: string) => \`세션 중앙값의 ${x}×\`` / en `` (x) => `${x}× session median` ``; `metric.badge.lowSample`: ko `'표본 부족'` / en `'low sample'`.
- `useSessionMetricsQuery` enable 조건: `analysisOpen || sel.selectedNodeId !== null` (선택이 생기면 p50을 당겨온다 — 백엔드 인메모리 캐시가 반복 호출을 흡수).

- [ ] **Step 1: 실패하는 테스트 작성**

`p50Badge.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { p50Badge } from '../p50Badge';

// t()는 key+arg를 그대로 합성하는 스텁 — 카탈로그 실값은 parity 테스트가 잠근다.
const t = ((key: string, arg?: unknown) => `${key}:${JSON.stringify(arg)}`) as never;

describe('p50Badge (PR-3 §3d)', () => {
  it('값/중앙값 비율을 x.x×로 만든다', () => {
    const b = p50Badge(3000, { p50: 1000, n: 10 }, t);
    expect(b).toEqual({ text: 'metric.badge.median:"3.0"', lowSample: false });
  });
  it('n<3이면 표본 부족 배지', () => {
    const b = p50Badge(3000, { p50: 1000, n: 2 }, t);
    expect(b!.lowSample).toBe(true);
  });
  it('값 없음·stat 없음·p50 null이면 배지 없음', () => {
    expect(p50Badge(null, { p50: 1000, n: 10 }, t)).toBeNull();
    expect(p50Badge(3000, undefined, t)).toBeNull();
    expect(p50Badge(3000, { p50: null, n: 5 }, t)).toBeNull();
  });
});
```

`EntityMetricsPanel.test.tsx`에 추가 (기존 렌더 헬퍼를 따른다):

```tsx
it('llmP50이 있으면 소요시간 행에 세션 중앙값 배지를 렌더한다 (PR-3 §3d)', () => {
  renderPanel({
    llmMetrics: { ...llmFixture, durationMs: 3000 },
    llmP50: {
      ttft_ms: { p50: null, n: 0 },
      duration_ms: { p50: 1000, n: 10 },
      output_tokens: { p50: null, n: 0 },
      cost_usd: { p50: null, n: 0 },
    },
  });
  expect(screen.getByText(/세션 중앙값의 3.0×/)).toBeInTheDocument();
});
```

- [ ] **Step 2: 실패 확인**

```bash
cd webui && npx vitest run src/components/replay/detail/__tests__/p50Badge.test.ts src/components/replay/detail/EntityMetricsPanel.test.tsx
```

Expected: FAIL — 모듈/텍스트 부재.

- [ ] **Step 3: 구현**

3-1. `p50Badge.ts`:

```ts
// PR-3 §3d — DetailPanel 요청 메트릭 행의 "세션 중앙값의 x.x×" 배지 파생.
// 백엔드 전수 p50(SessionMetrics.llm_request_p50) 기준 — 로드 윈도우 근사 아님.
import type { P50StatDto } from '../../../api/types';
import type { TFunction } from '../../../i18n';

export type P50Badge = { text: string; lowSample: boolean };

export function p50Badge(
  value: number | null,
  stat: P50StatDto | undefined,
  t: TFunction,
): P50Badge | null {
  if (value == null || !stat) return null;
  if (stat.n < 3) return { text: t('metric.badge.lowSample'), lowSample: true };
  if (stat.p50 == null || stat.p50 <= 0) return null;
  return { text: t('metric.badge.median', (value / stat.p50).toFixed(1)), lowSample: false };
}
```

3-2. `metricsRows.tsx` — `Row`에 badge prop:

```tsx
export function Row({
  labelKey,
  tipKey,
  value,
  warn = false,
  badge = null,
}: {
  labelKey: MessageKey;
  tipKey?: MessageKey;
  value: string;
  warn?: boolean;
  badge?: P50Badge | null;
}) {
  // …기존 본문…
  <span className={styles.v}>
    {value}
    {badge && (
      <span className={styles.p50Badge} data-low={String(badge.lowSample)}>
        {badge.text}
      </span>
    )}
  </span>
```

`ResponseMetricsRows`에 p50 prop + 4행 배지:

```tsx
export function ResponseMetricsRows({
  metrics,
  p50 = null,
}: {
  metrics: LlmRequestMetrics;
  p50?: LlmRequestP50Dto | null;
}) {
  const t = useT();
  return (
    <>
      <MetricGroup title={t('metric.group.llmActivity')} provenance="measured">
        <Row
          labelKey="metric.label.duration"
          value={formatDuration(metrics.durationMs) ?? '—'}
          badge={p50Badge(metrics.durationMs, p50?.duration_ms, t)}
        />
        <Row
          labelKey="metric.label.ttft"
          value={formatDuration(metrics.ttftMs) ?? '—'}
          badge={p50Badge(metrics.ttftMs, p50?.ttft_ms, t)}
        />
        {/* stopReason/attempts/success/model/querySource 행은 기존 그대로 */}
```

`metric.label.outputTokens` 행에 `badge={p50Badge(metrics.outputTokens, p50?.output_tokens, t)}`,
`metric.label.billedCost` 행에 `badge={p50Badge(metrics.costUsd, p50?.cost_usd, t)}`.
import: `import { p50Badge, type P50Badge } from './p50Badge';`, `import type { LlmRequestP50Dto } from '../../../api/types';`

3-3. `metricsRows.module.css`:

```css
.p50Badge {
  margin-left: 6px;
  padding: 1px 5px;
  border-radius: 4px;
  font-size: 10.5px;
  font-family: var(--wimcc-mono, monospace);
  color: var(--wimcc-fg-subtle);
  background: var(--wimcc-surface-2);
  white-space: nowrap;
}
.p50Badge[data-low='true'] {
  background: transparent;
}
```

3-4. prop 체인 — 각 파일의 props 인터페이스에 `llmP50?: LlmRequestP50Dto | null` 추가
후 그대로 내려보낸다:
- `DetailPanel.tsx`: `DetailPanelProps`에 추가 → `<InsightTab … llmP50={llmP50 ?? null} />`
- `InsightTab.tsx`: `InsightTabProps`에 추가 → `<EntityMetricsPanel … llmP50={llmP50} />`
- `EntityMetricsPanel.tsx`: props에 추가 → `{llmMetrics ? <ResponseMetricsRows metrics={llmMetrics} p50={llmP50} /> : <Uncollected />}`
- `SessionDetailPage.tsx` 84~85행:

```tsx
const metricsQuery = useSessionMetricsQuery(sessionId, {
  enabled: (analysisOpen || sel.selectedNodeId !== null) && !!sessionId,
});
```

DetailPanel 렌더부(471행 근처)에 `llmP50={metricsQuery.data?.llm_request_p50 ?? null}` 추가.

3-5. i18n 두 카탈로그에 `metric.badge.median`/`metric.badge.lowSample` 추가(기존
`metric.*` 키 군집 위치에).

- [ ] **Step 4: 통과 확인**

```bash
cd webui && npx vitest run
```

Expected: `0 failed` (parity 포함).

- [ ] **Step 5: Commit**

```bash
git add webui/src/components/replay/detail/p50Badge.ts webui/src/components/replay/detail/__tests__/p50Badge.test.ts webui/src/components/replay/detail/metricsRows.tsx webui/src/components/replay/detail/metricsRows.module.css webui/src/components/replay/detail/EntityMetricsPanel.tsx webui/src/components/replay/detail/EntityMetricsPanel.test.tsx webui/src/components/replay/detail/InsightTab.tsx webui/src/components/replay/detail/DetailPanel.tsx webui/src/routes/SessionDetailPage.tsx webui/src/i18n/catalog/ko.ts webui/src/i18n/catalog/en.ts
git commit -m "feat(webui): 상세 패널 요청 메트릭에 세션 p50 대비 배지 (PR-3 §3d)"
```

---

### Task 7: 브라우저 smoke + 개선 루프 + 노트 갱신 + PR

**Files:**
- Modify: `docs/implementation-notes.html` (append-only 항목 추가), `docs/notes-index.md` (토픽 갱신)

- [ ] **Step 1: 프론트 빌드 + 스크래치 스택 기동**

운영 serve(:7878)는 **절대 재시작하지 않는다**. 스크래치 DB로:

```bash
cd webui && npm run build && cd ..
cargo build
SCRATCH=$(mktemp -d)/pr3.sqlite
./target/debug/wimcc --db-path "$SCRATCH" ingest --all   # 로컬 코퍼스 재ingest
./target/debug/wimcc --db-path "$SCRATCH" serve --port 7999 --auto-migrate &
cd webui && WIMCC_PROXY_TARGET=http://127.0.0.1:7999 npx vite --port 5174 &
```

(`--auto-migrate` 누락 금지 — 2026-07-04 실사고. ingest 명령 플래그가 다르면 `wimcc --help`로 확인.)

- [ ] **Step 2: API 계약 수동 확인**

```bash
curl -s "http://127.0.0.1:7999/v1/usage/baseline" | python3 -m json.tool | head -30
curl -s "http://127.0.0.1:7999/v1/usage/baseline?session_id=<코퍼스의 세션 id>" | python3 -m json.tool | head -30
curl -s "http://127.0.0.1:7999/v1/sessions/<같은 id>/metrics" | python3 -m json.tool | grep -A7 llm_request_p50
```

Expected: `scope`/`n`/신규 3지표, `llm_request_p50` 4항목이 실데이터로 채워짐(OTel 없는 세션은 null·n=0 — 그것도 정상 관측).

- [ ] **Step 3: 브라우저 시각 검증**

`http://localhost:5174/sessions/<세션 id>`에서:
1. InsightStrip 5카드 foot — DeltaChip + "프로젝트 중앙값의 x.x× · n N" 표기(또는 표본 부족).
2. 비용 카드 detail — "블렌디드 $x.xx/1M".
3. assistant 카드 선택 → DetailPanel 소요시간/TTFT/출력 토큰/청구 비용 행의 "세션 중앙값의 x.x×" 배지.
4. 대시보드(`/`) 헤드라인 delta 칩 무회귀(모양 동일).

Chrome 확장 미연결 환경이면 CDP 스크립트로 스크린샷(cdp-shot.mjs 패턴 — 메모리
`headless-smoke-cdp-script`). 시각 확인 없이 이 태스크를 완료 처리하지 않는다.

- [ ] **Step 4: 개선 루프 (PR-전 필수)**

```bash
cd webui && node scripts/untagged-bash.ts --all
cd webui && node scripts/unknown-verification.ts --all
cd webui && node scripts/unidentified-plugins.ts --all
cd webui && node scripts/tagging-gate.ts
```

Expected: tagging-gate exit 0. 실패 시 사전 추가(원칙) 또는 baseline에 사유 커밋.

- [ ] **Step 5: implementation-notes + notes-index 갱신**

`docs/implementation-notes.html`에 앵커 `#session-insight-context-2026-07-04`로 항목 추가:
- baseline의 session_id→project 스코프 해석(스코프 미상 store 폴백, scope 필드로 정직 표기), 지표별 분모 게이트(측정>0·tool_call>0·billed>0) 결정.
- `llm_request_p50` — counts-only 표면(F1)에 분포 통계를 추가한 **의도적 편차**: 전수 이벤트 없이는 소비자가 재계산 불가하므로 서버 반환(03 스펙 각주와 동일 문구). 소스 이중화(llm_request span + api_request log)와 request_id 최초 1건 dedup 규칙.
- 스펙 §3d 문구("api_request_log 상관 이벤트")를 실소스 2갈래로 구체화한 편차.

`docs/notes-index.md`의 "WebUI replay·목록" 행(또는 신설 행)의 현재 진실 앵커를
`#session-insight-context-2026-07-04`로 갱신.

- [ ] **Step 6: 최종 게이트 + PR**

```bash
cd webui && npx vitest run && npm run build && cd ..
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test 2>&1 | tail -5
git add docs/implementation-notes.html docs/notes-index.md
git commit -m "docs(notes): 세션 인사이트 맥락화(PR-3) 결정·편차 기록"
git push -u origin feat/pr3-insight-context
gh pr create --title "feat: 요약 카드 중앙값 대비 + 상세 p50 배지 (스펙 §3a·§3d)" --body "docs/specs/2026-07-04-session-detail-improvements.md §3a·§3d 구현. 상세는 커밋·implementation-notes #session-insight-context-2026-07-04 참조."
```

**병합 금지** — PR 생성·CI 확인까지만(사용자 검수 후 rebase 병합).

---

## Self-Review 결과 (작성 시 반영 완료)

1. **스펙 커버리지**: §3a DeltaChip 공용화(Task 1·4)·프로젝트 중앙값+n(Task 2·3·4)·블렌디드 단가(Task 3) / §3d p50 백엔드(Task 5)·배지(Task 6) / §3e 해당 테스트 전부 매핑. §3b·§3c는 PR-4 별도 계획.
2. **플레이스홀더**: 코드 스텝 전부 실제 코드. 단 기존 파일의 helper 이름 2곳(`seed_run`·`renderStrip`/`renderPanel` fixture)은 "그 파일의 기존 관례를 따른다"로 지시 — 구현자가 해당 파일을 열었을 때 즉시 해소되는 참조라 허용.
3. **타입 일관성**: `BaselineStat{p25,median,p75,n}`(Rust/TS 동형), `BaselineMedian`/`BaselineComparison`(Task 3 정의 → Task 4 소비), `P50Stat(Rust)`↔`P50StatDto(TS)`, `LlmRequestP50`↔`LlmRequestP50Dto`, prop 이름 `llmP50`(체인 전 구간)·`p50`(ResponseMetricsRows) 일치 확인.
