# wimcc Fitness F1·F2 거짓 수정 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** LLM judge를 오도하는 두 거짓을 제거한다 — (F1) `/metrics`·`/usage`의 window-고정 rate scalar를 삭제하고 합성 가능한 count만 노출, (F2) verification 탐지의 Tier-2 keyword 휴리스틱을 제거해 산문 phantom run을 소거.

**Architecture:** 둘 다 결정론 layer의 정직성 수정. 신규 스키마 없음(F1은 on-demand 집계/DTO, F2는 탐지 로직). rate는 소비자(프런트/LLM)가 count에서 자기 window로 계산. verification은 known_tool(결정론 allowlist)만 인정.

**Tech Stack:** Rust(axum, sqlx, serde) 백엔드 · TypeScript/React WebUI · cargo test + vitest + 브라우저 smoke.

**근거 spec:** `docs/superpowers/specs/2026-06-08-wimcc-loop-fitness-improvements-design.md` (F1·F2). 실측 fixture: `.wimcc-analysis.sqlite`(195 measured/1539 unknown verification; 1b30ced8 2683 assistant-events vs 43 user-turns; test_keyword measured 13/195).

---

## File Structure

**F2 (탐지 — 백엔드만, 격리, 최저위험 → 먼저):**
- Modify: `src/insight/verification_allowlist.rs` — `classify_segment`에서 Tier-2 블록 제거 + 인라인 테스트 갱신.

**F1 백엔드 (집계/DTO):**
- Modify: `src/insight/metrics.rs` — `SessionMetrics`에서 rate 3종 삭제, verification count 3종으로 확장.
- Modify: `src/db/repo_usage_facet.rs` — `UsageAggregate`/`ModelUsage`의 `turns`→`assistant_events`; `SessionMetrics`(baseline)에서 `cache_hit_ratio` 유지(분포)·`turns`→`assistant_events`.
- Modify: `src/db/repo_observed.rs` — `count_distinct_turns(session_id)` 추가(user_turns 산출).
- Modify: `src/api/dto.rs` — `SessionUsageDto`/`ModelUsageDto`: `turns`→`assistant_events`, `user_turns` 추가, `cache_hit_ratio` 삭제.
- Modify: `src/api/routes.rs` — `session_usage` 핸들러: 위 DTO 채우기 + `user_turns` 조회. baseline 핸들러 turns 라벨.

**F1 프런트 (소비자 — count에서 rate 계산):**
- Modify: `webui/src/api/types.ts` — `SessionMetricsDto`/`SessionUsageDto` 타입 갱신.
- Modify: `webui/src/components/replay/analysis/AnalysisPanel.tsx` — count에서 표시 rate 계산 + unknown 표시.
- Modify: `webui/src/components/replay/insight-strip/insightCards.ts` — `cache_hit_ratio`를 /usage 토큰 component에서 계산, `turns`→`assistant_events`/`user_turns`.

**검증:**
- Tests: `tests/metrics_compute.rs`, `tests/api_metrics.rs`, `tests/api_usage.rs`, `src/insight/verification_allowlist.rs`(인라인) + 브라우저 smoke.

---

## Task 1: F2 — verification 탐지 Tier-2 keyword fallback 제거

**Files:**
- Modify: `src/insight/verification_allowlist.rs:282-319` (`classify_segment`) + 인라인 테스트(`classify_segment_tier2_keyword_fallback`)
- Test: 같은 파일 `#[cfg(test)] mod tests`

- [ ] **Step 1: 실패하는 테스트 작성** — 산문 3종이 분류되지 않음을 잠근다

`src/insight/verification_allowlist.rs`의 `mod tests`에 추가:

```rust
#[test]
fn classify_segment_drops_prose_false_positives() {
    // Real-data anchoring (.wimcc-analysis.sqlite): 아래 산문 줄들은 multi-line
    // Bash(commit -m 본문·heredoc)에서 split돼 제거 대상 Tier-2 keyword fallback에
    // phantom test run으로 잡혔다. Tier-1(known_tool)에는 매칭되지 않으므로 None이어야.
    assert_eq!(
        classify_segment("- CI 회복: scripts/run-tests.mjs 신설 (cross-platform glob)"),
        None
    );
    assert_eq!(
        classify_segment("- SA1 Metica activation was previously gated on completion of Airflux test"),
        None
    );
    assert_eq!(
        classify_segment("declare the contract at spec §1.9. Pages live as `<slug>.md`"),
        None
    );
}

#[test]
fn classify_segment_known_tool_still_matches() {
    assert_eq!(classify_segment("cargo test"), Some(("test_suite_rust", "known_tool")));
    assert_eq!(classify_segment("npx vitest run"), Some(("test_suite_js", "known_tool")));
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test --lib verification_allowlist::tests::classify_segment_drops_prose_false_positives`
Expected: FAIL — 현재 `classify_segment`이 `Some(("test_suite_other","test_keyword"))`를 반환(`test`/`tests`/`spec` 토큰 매칭).

- [ ] **Step 3: Tier-2 블록 제거**

`src/insight/verification_allowlist.rs`의 `classify_segment`(현재 282–319) 본문을 교체:

```rust
pub fn classify_segment(segment: &str) -> Option<(&'static str, &'static str)> {
    let stripped = strip_wrappers(strip_redirects(segment));

    // Dry-run / collect-only / list 세그먼트는 verification run이 아니다.
    if is_dry_run(stripped) {
        return None;
    }

    // Tier-1만: known tool(결정론 allowlist). 과거 Tier-2 keyword fallback
    // ("세그먼트에 test/spec 토큰이 있으면 테스트일 것")은 제거됨 — multi-line
    // Bash(commit 메시지·heredoc)에서 split된 산문이 phantom run을 만들었고,
    // 이는 휴리스틱 추정이다. .wimcc-analysis.sqlite 기준 test_keyword는
    // measured 13/195건만 담고 false-positive 클래스 전체를 만들었다.
    classify(stripped).map(|kind| (kind, "known_tool"))
}
```

- [ ] **Step 4: 기존 Tier-2 테스트를 None 기대로 갱신**

`classify_segment_tier2_keyword_fallback` 테스트를 교체(같은 파일):

```rust
#[test]
fn classify_segment_non_allowlist_runner_no_longer_matches() {
    // Tier-2 제거 trade-off(정직): 비-allowlist 실 러너는 더 이상 잡지 않는다.
    // 거짓 phantom보다 일부 누락이 낫다. 필요 시 allowlist를 결정론적으로 확장.
    assert_eq!(classify_segment("./run_integration_test.sh"), None);
    assert_eq!(classify_segment("make spec"), None);
}
```

(`classify_segment_tier2_path_with_tests_dir_is_not_a_run` 테스트는 여전히 `None` 기대이므로 그대로 둔다.)

- [ ] **Step 5: 전체 verification 테스트 통과 확인**

Run: `cargo test --lib verification_allowlist`
Expected: PASS (신규 2 + 갱신된 기존).
Run: `cargo test --test '*' verification` (있다면) + `cargo test verification_run`
Expected: PASS — `matched_segment` 테스트는 known_tool 케이스라 영향 없음.

- [ ] **Step 6: 커밋**

```bash
git add src/insight/verification_allowlist.rs
git commit -m "fix(insight): drop Tier-2 keyword fallback — prose phantom verification runs

test_keyword 휴리스틱이 multi-line Bash 산문(commit 메시지·heredoc)을 테스트
명령으로 추정해 phantom run을 만들었다. known_tool(결정론 allowlist)만 인정.
measured 신호 93.3%(182/195) 유지, false-positive 클래스 소거. spec F2."
```

---

## Task 2: F1 — `/metrics`에서 rate scalar 삭제, verification count 확장

**Files:**
- Modify: `src/insight/metrics.rs` (`SessionMetrics` 구조체 20-42, `compute_session_metrics` 71-113, `rate` fn 44-51)
- Modify: `tests/metrics_compute.rs`

- [ ] **Step 1: 실패하는 테스트 작성** — count 분리 + rate 부재

`tests/metrics_compute.rs`에 추가(파일의 `seed_event`/`VerificationRunRow` 패턴을 그대로 사용; vrun은 `repo_verification_run`로 삽입, 필드는 `repo_verification_run::VerificationRunRow` 전체):

```rust
#[tokio::test]
async fn metrics_separates_verification_unknown_from_measured() {
    let pool = test_pool().await;
    let run = "run_m1";
    repo_runs::insert(&pool, /* 기존 헬퍼와 동일 */).await.unwrap(); // 파일 내 기존 run-seed 패턴 사용
    let sid = "sess_metrics_unknown";
    // passed 1, failed 2, unknown 3 — 같은 파일의 VerificationRunRow 삽입 패턴을 따른다.
    insert_vrun(&pool, run, sid, "vr1", "passed").await;
    insert_vrun(&pool, run, sid, "vr2", "failed").await;
    insert_vrun(&pool, run, sid, "vr3", "failed").await;
    insert_vrun(&pool, run, sid, "vr4", "unknown").await;
    insert_vrun(&pool, run, sid, "vr5", "unknown").await;
    insert_vrun(&pool, run, sid, "vr6", "unknown").await;

    let m = compute_session_metrics(&pool, sid).await.unwrap();
    assert_eq!(m.verification_total, 6);
    assert_eq!(m.verification_passed, 1);
    assert_eq!(m.verification_failed, 2);
    assert_eq!(m.verification_unknown, 3);
    // measured = passed + failed = 3; unknown은 분모에서 분리되어 노출된다.
}
```

> `insert_vrun` 헬퍼가 파일에 없으면 `seed_event`/`make_signal` 옆에 추가:
> `VerificationRunRow { verification_run_id, schema_version:"verification_run.v1", session_id, source:"bash", command:"cargo test", command_kind:"test_suite_rust", trigger_event_id:"t", trigger_tool_use_id:None, status, status_provenance:Some(if status=="unknown"{"unknown"}else{"measured"}.into()), detection_basis:"known_tool", status_basis:"exit", started_at:<rfc3339>, ended_at:None, exit_code:None, failure_summary:None, raw_event_id:<seed>, parser_version:"test@v0" }` 를 `repo_verification_run::insert(&pool, &row)`로 삽입(트리거 event/raw FK는 `seed_event` 재사용).

- [ ] **Step 2: 실패 확인**

Run: `cargo test --test metrics_compute metrics_separates_verification_unknown_from_measured`
Expected: FAIL — `SessionMetrics`에 `verification_failed`/`verification_unknown` 필드 없음(컴파일 에러).

- [ ] **Step 3: `SessionMetrics` 구조체 교체**

`src/insight/metrics.rs`의 구조체(20-42)를 교체 — rate 3종 삭제, count 2종 추가:

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionMetrics {
    pub session_id: String,
    /// Total `tool_call` events in the session.
    pub tool_call_total: i64,
    /// Number of `tool_failure` detector signals fired.
    pub tool_failure_count: i64,
    /// Total verification runs recorded.
    pub verification_total: i64,
    /// Runs whose `status == "passed"`.
    pub verification_passed: i64,
    /// Runs whose `status == "failed"`.
    pub verification_failed: i64,
    /// Runs whose `status == "unknown"` (measurement failed — NOT a failure).
    pub verification_unknown: i64,
    /// Number of `context_bloat` detector signals fired.
    pub context_bloat_count: i64,
    /// detector → number of signals fired (signal distribution).
    pub detector_firing: BTreeMap<String, i64>,
}
```

> Rate(`tool_failure_rate`/`verification_pass_rate`/`cache_hit_ratio`)는 window-고정 파생값이라 삭제.
> 소비자가 count에서 자기 window로 계산. cache는 토큰 component(`/usage`)로 계산.

- [ ] **Step 4: `compute_session_metrics` 본문 교체**

`src/insight/metrics.rs`의 함수 본문(71-113)을 교체 — usage 로드/cache 계산 제거, count 산출:

```rust
pub async fn compute_session_metrics(pool: &SqlitePool, session_id: &str) -> Result<SessionMetrics> {
    let events = repo_observed::list_session(pool, session_id, 100_000).await?;
    let signals = repo_signal::list_by_session(pool, session_id).await?;
    let vruns = repo_verification_run::list_session(pool, session_id).await?;

    let tool_call_total = events
        .iter()
        .filter(|e| e.kind == EventKind::ToolCall)
        .count() as i64;

    let mut detector_firing: BTreeMap<String, i64> = BTreeMap::new();
    for s in &signals {
        *detector_firing.entry(s.detector.clone()).or_insert(0) += 1;
    }
    let tool_failure_count = *detector_firing.get("tool_failure").unwrap_or(&0);
    let context_bloat_count = *detector_firing.get("context_bloat").unwrap_or(&0);

    let verification_total = vruns.len() as i64;
    let verification_passed = vruns.iter().filter(|v| v.status == "passed").count() as i64;
    let verification_failed = vruns.iter().filter(|v| v.status == "failed").count() as i64;
    let verification_unknown = vruns.iter().filter(|v| v.status == "unknown").count() as i64;

    Ok(SessionMetrics {
        session_id: session_id.to_string(),
        tool_call_total,
        tool_failure_count,
        verification_total,
        verification_passed,
        verification_failed,
        verification_unknown,
        context_bloat_count,
        detector_firing,
    })
}
```

그리고 미사용이 된 `rate` fn(44-51)과 `repo_usage_facet` import(12행에서 해당 항목)를 삭제. 컴파일러 경고로 확인.

- [ ] **Step 5: 통과 + 기존 테스트 정정**

Run: `cargo test --test metrics_compute`
Expected: 신규 PASS. 기존 테스트 중 `*_rate`/`cache_hit_ratio`를 assert하던 것이 있으면 count assert로 정정.
Run: `cargo test --test api_metrics`
Expected: FAIL 시 — `api_metrics.rs`에서 rate 필드를 검사하던 단언을 count(passed/failed/unknown) 검사로 정정.

- [ ] **Step 6: 커밋**

```bash
git add src/insight/metrics.rs tests/metrics_compute.rs tests/api_metrics.rs
git commit -m "fix(insight): /metrics drop window-fixed rates, expose composable counts

verification_pass_rate가 unknown(89%)을 분모에 섞어 오도. rate 3종 삭제,
verification passed/failed/unknown count로 분리(measured=passed+failed).
rate는 소비자가 count에서 자기 window로 계산. spec F1."
```

---

## Task 3: F1 — `/usage` turns 개명 + user_turns 추가 + cache_hit_ratio 삭제

**Files:**
- Modify: `src/db/repo_usage_facet.rs` (`UsageAggregate` 35-43, `ModelUsage` 45-56, `session_aggregate` SQL 154-, baseline `SessionMetrics` 58-69)
- Modify: `src/db/repo_observed.rs` (신규 `count_distinct_turns`)
- Modify: `src/api/dto.rs` (`SessionUsageDto` 215-237, `ModelUsageDto` 239-251)
- Modify: `src/api/routes.rs` (`session_usage` 364-420)
- Test: `tests/api_usage.rs`

- [ ] **Step 1: 실패하는 테스트 작성** — assistant_events ≠ user_turns

`tests/api_usage.rs`에 추가(파일의 세션 seed 패턴 사용 — usage_facet N행 + observed_event에 distinct turn_id M개):

```rust
#[tokio::test]
async fn usage_reports_assistant_events_and_user_turns_separately() {
    let (pool, sid) = seed_usage_session().await; // 파일 기존 헬퍼: usage_facet 3행 + user_message 2개(turn_id "t1","t1","t2")
    let body = get_usage(&pool, &sid).await;       // 파일 기존 GET 헬퍼 → serde_json::Value
    let data = &body["data"];
    assert_eq!(data["assistant_events"].as_i64().unwrap(), 3); // usage_facet 행 수
    assert_eq!(data["user_turns"].as_i64().unwrap(), 2);       // distinct turn_id
    assert!(data.get("turns").is_none(), "거짓 'turns' 라벨 제거됨");
    assert!(data.get("cache_hit_ratio").is_none(), "window-고정 rate 제거됨");
}
```

> 기존 `seed_usage_session`/`get_usage` 헬퍼가 다른 이름이면 파일 내 동등 헬퍼로 대체하고, observed_event에 turn_id가 다른 user_message 2건을 넣어 distinct=2를 만든다.

- [ ] **Step 2: 실패 확인**

Run: `cargo test --test api_usage usage_reports_assistant_events_and_user_turns_separately`
Expected: FAIL — `assistant_events`/`user_turns` 키 없음, `turns`/`cache_hit_ratio` 존재.

- [ ] **Step 3: `repo_observed::count_distinct_turns` 추가**

`src/db/repo_observed.rs`에 추가:

```rust
/// Distinct `turn_id` count for a session = number of user turns (prompts).
/// `turn_id` is the prompt_id carried by user + assistant events; counting
/// distinct non-null values yields user-turn count (NOT assistant-event count).
pub async fn count_distinct_turns(pool: &SqlitePool, session_id: &str) -> Result<i64> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(DISTINCT turn_id) FROM observed_event \
         WHERE session_id = ? AND turn_id IS NOT NULL",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}
```

- [ ] **Step 4: 집계 구조체 `turns`→`assistant_events`**

`src/db/repo_usage_facet.rs`에서 `UsageAggregate.turns`·`ModelUsage.turns`·baseline `SessionMetrics.turns`를 `assistant_events`로 개명하고, `session_aggregate` SQL의 `COUNT(*) AS turns`를 `COUNT(*) AS assistant_events`로 바꾼 뒤 `row.get`/struct 채움도 일치시킨다. baseline `SessionMetrics.cache_hit_ratio`는 **유지**(cross-session 분포 통계 — rate scalar가 아니라 분위수 분포라 정당). 코드 전반의 `.turns` 참조를 `.assistant_events`로 컴파일러가 가리키는 대로 정정.

- [ ] **Step 5: DTO 갱신**

`src/api/dto.rs` `SessionUsageDto`(215): `turns: i64`를 `assistant_events: i64`로 바꾸고 그 아래 `user_turns: i64` 추가, `cache_hit_ratio: Option<f64>` 필드 **삭제**. `ModelUsageDto`(239): `turns`→`assistant_events`.

```rust
pub struct SessionUsageDto {
    pub session_id: String,
    pub assistant_events: i64,   // (was `turns`) usage_facet 행 수 = assistant 산출 이벤트
    pub user_turns: i64,         // distinct turn_id = 사용자 턴(프롬프트) 수
    pub input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub output_tokens: i64,
    pub billed_tokens: i64,
    // cache_hit_ratio 삭제 — 소비자가 위 토큰 component로 계산(window-고정 rate 금지)
    pub estimated_cost_usd: f64,
    pub cost_basis: String,
    pub pricing_version: String,
    pub models_without_pricing: Vec<String>,
    pub by_model: Vec<ModelUsageDto>,
}
```

- [ ] **Step 6: `session_usage` 핸들러 갱신**

`src/api/routes.rs` `session_usage`(364): `cache_hit_ratio` 계산/필드 삭제, `user_turns` 조회 추가, DTO 필드명 정정:

```rust
let user_turns = repo_observed::count_distinct_turns(&pool, &id).await.expect("db");
// ... billed 계산 유지, cache_hit_ratio 블록 삭제 ...
let data = SessionUsageDto {
    session_id: id,
    assistant_events: agg.assistant_events,
    user_turns,
    input_tokens: agg.input_tokens,
    cache_creation_input_tokens: agg.cache_creation_input_tokens,
    cache_read_input_tokens: agg.cache_read_input_tokens,
    output_tokens: agg.output_tokens,
    billed_tokens: billed,
    estimated_cost_usd: cost.total_usd,
    cost_basis: crate::insight::pricing::COST_BASIS_ESTIMATE.to_string(),
    pricing_version: crate::insight::pricing::PRICING_VERSION.to_string(),
    models_without_pricing: cost.models_without_pricing.clone(),
    by_model: agg.by_model.into_iter().map(|m| {
        let est = priced.get(m.model.as_str()).copied().unwrap_or(0.0);
        let is_priced = crate::insight::pricing::rates_for(&m.model).is_some();
        ModelUsageDto {
            model: m.model,
            assistant_events: m.assistant_events,
            input_tokens: m.input_tokens,
            cache_creation_input_tokens: m.cache_creation_input_tokens,
            cache_read_input_tokens: m.cache_read_input_tokens,
            output_tokens: m.output_tokens,
            estimated_cost_usd: est,
            priced: is_priced,
        }
    }).collect(),
};
```

- [ ] **Step 7: 통과 + baseline turns 라벨 정정**

Run: `cargo test --test api_usage`
Expected: PASS.
Run: `cargo test --test api_usage_baseline`
Expected: FAIL 시 — baseline DTO/응답의 `turns` 라벨을 `assistant_events`로 정정(분위수 의미는 동일, 이름만). `cache_hit_ratio` 분위수는 유지.

- [ ] **Step 8: 커밋**

```bash
git add src/db/repo_usage_facet.rs src/db/repo_observed.rs src/api/dto.rs src/api/routes.rs tests/api_usage.rs tests/api_usage_baseline.rs
git commit -m "fix(api): /usage rename turns→assistant_events, add user_turns, drop cache_hit_ratio

turns가 assistant 산출 이벤트 수(2683)를 user turn(43)으로 라벨해 62배 오도.
assistant_events로 개명 + user_turns(distinct turn_id) 추가. cache_hit_ratio는
window-고정 rate라 삭제(소비자가 토큰 component로 계산). spec F1."
```

---

## Task 4: F1 프런트 — 타입 갱신

**Files:**
- Modify: `webui/src/api/types.ts` (`SessionUsageDto` 149-165, `SessionMetricsDto` 187-198)

- [ ] **Step 1: 타입 정정**

`webui/src/api/types.ts`:

```typescript
// SessionUsageDto
  session_id: string;
  assistant_events: number;   // (was turns)
  user_turns: number;         // distinct turn_id
  input_tokens: number;
  cache_creation_input_tokens: number;
  cache_read_input_tokens: number;
  output_tokens: number;
  billed_tokens: number;
  // cache_hit_ratio 삭제
  estimated_cost_usd: number;
  cost_basis: string;
  pricing_version: string;
  models_without_pricing: string[];
  by_model: ModelUsageDto[];
```

`SessionMetricsDto`(187): `tool_failure_rate`·`verification_pass_rate`·`cache_hit_ratio` 삭제, `verification_failed`·`verification_unknown` 추가:

```typescript
export type SessionMetricsDto = {
  session_id: string;
  tool_call_total: number;
  tool_failure_count: number;
  verification_total: number;
  verification_passed: number;
  verification_failed: number;
  verification_unknown: number;
  context_bloat_count: number;
  detector_firing: Record<string, number>;
};
```

`ModelUsageDto`의 `turns`→`assistant_events`, `UsageBaselineDto`의 `turns`→`assistant_events`도 정정.

- [ ] **Step 2: 타입 체크**

Run: `cd webui && npx tsc --noEmit`
Expected: FAIL — `AnalysisPanel.tsx`/`insightCards.ts`가 삭제된 필드를 참조(다음 Task에서 수정).

- [ ] **Step 3: 커밋** (Task 5와 함께 커밋하므로 여기선 생략 — 타입+소비자 한 커밋)

---

## Task 5: F1 프런트 — 소비자가 count에서 rate 계산

**Files:**
- Modify: `webui/src/components/replay/analysis/AnalysisPanel.tsx` (44-67)
- Modify: `webui/src/components/replay/insight-strip/insightCards.ts` (89-130)

- [ ] **Step 1: AnalysisPanel — count에서 표시 rate 계산 + unknown 노출**

`AnalysisPanel.tsx`의 metrics 테이블(44-67) 3개 row 교체. rate는 컴포넌트가 whole-session window로 count에서 계산:

```tsx
{/* 도구 실패: rate는 count에서 계산 */}
<div className={styles.metricRow}>
  <span className={styles.metricLabel}>도구 실패</span>
  <span className={styles.metricCount}>
    {metrics.tool_failure_count}/{metrics.tool_call_total}
  </span>
  <span className={styles.metricRate}>
    {metrics.tool_call_total > 0
      ? pct(metrics.tool_failure_count / metrics.tool_call_total)
      : '—'}
  </span>
</div>
{/* 검증: 분모는 measured(passed+failed), unknown 별도 노출 */}
<div className={styles.metricRow}>
  <span className={styles.metricLabel}>검증 통과 (측정분)</span>
  <span className={styles.metricCount}>
    {metrics.verification_passed}/{metrics.verification_passed + metrics.verification_failed}
    {metrics.verification_unknown > 0 && ` · 미측정 ${metrics.verification_unknown}`}
  </span>
  <span className={styles.metricRate}>
    {metrics.verification_passed + metrics.verification_failed > 0
      ? pct(metrics.verification_passed / (metrics.verification_passed + metrics.verification_failed))
      : '측정 없음'}
  </span>
</div>
{/* context bloat */}
<div className={styles.metricRow}>
  <span className={styles.metricLabel}>Context bloat 횟수</span>
  <span className={styles.metricCount}>{metrics.context_bloat_count}</span>
  <span className={styles.metricRate} />
</div>
```

(캐시 히트율 row 삭제 — `/metrics`에 더는 없음. 캐시 지표는 insight-strip이 `/usage`에서 보여준다.)

- [ ] **Step 2: insightCards — cache_hit를 토큰 component로 계산, turns 라벨 정정**

`insightCards.ts`: `u.cache_hit_ratio` 참조를 토큰 component 계산으로 대체하는 헬퍼 추가, `u.turns`→`u.user_turns`(또는 `assistant_events` — 표기 의도에 맞게; "턴 수"는 user_turns가 정확):

```typescript
// usage 토큰 component에서 cache hit ratio 계산(window=세션 전체)
function cacheHitRatio(u: SessionUsageDto): number | null {
  const denom = u.cache_read_input_tokens + u.cache_creation_input_tokens + u.input_tokens;
  return denom > 0 ? u.cache_read_input_tokens / denom : null;
}
```

그리고 89·94·101-103행의 `u.cache_hit_ratio`를 `cacheHitRatio(u)`로, 97행 `턴 수 ${u.turns}`를 `사용자 턴 ${u.user_turns}`로, 130행 `${m.turns}턴`을 `${m.assistant_events} 산출`로 교체. baseline delta(101-103)는 `inputs.baseline?.cache_hit_ratio`(baseline은 cache_hit_ratio 분위수 유지)와 `cacheHitRatio(u)` 비교로.

- [ ] **Step 3: 타입 체크 + vitest**

Run: `cd webui && npx tsc --noEmit`
Expected: PASS.
Run: `cd webui && npx vitest run`
Expected: PASS — 기존 insightCards 테스트가 있으면 turns/cache 필드 변경에 맞춰 정정.

- [ ] **Step 4: 커밋**

```bash
git add webui/src/api/types.ts webui/src/components/replay/analysis/AnalysisPanel.tsx webui/src/components/replay/insight-strip/insightCards.ts
git commit -m "fix(webui): compute display rates from counts; assistant_events/user_turns

백엔드가 rate scalar를 제거(spec F1) → 프런트가 count에서 whole-session rate를
계산. 검증은 measured 분모 + 미측정(unknown) 별도 노출. turns→user_turns 라벨 정정."
```

---

## Task 6: 통합 검증 — 재ingest + API + 브라우저 smoke

**Files:** (없음 — 검증 전용)

- [ ] **Step 1: 빌드 + 전체 테스트**

Run: `cargo test` · `cd webui && npx vitest run`
Expected: 전부 PASS.

- [ ] **Step 2: 재ingest 후 실측 확인**

```bash
WIMCC_DB=.wimcc-analysis.sqlite ./target/release/wimcc init-db   # 또는 rm 후 재생성
WIMCC_DB=.wimcc-analysis.sqlite cargo run --release -- ingest --all
```
Run: verification 분포 재확인 — `sqlite3 .wimcc-analysis.sqlite "SELECT detection_basis, COUNT(*) FROM verification_run GROUP BY detection_basis;"`
Expected: `test_keyword` 행이 사라지고 `known_tool`만 남음(프로즈 phantom 0).

- [ ] **Step 3: API 정직성 확인**

```bash
WIMCC_DB=.wimcc-analysis.sqlite ./target/release/wimcc serve --auth off --no-watch-transcripts --port 7879 &
S=$(sqlite3 .wimcc-analysis.sqlite "SELECT session_id FROM observed_event WHERE session_id LIKE '1b30ced8%' LIMIT 1;")
curl -s "http://127.0.0.1:7879/v1/sessions/$S/metrics" | python3 -m json.tool   # rate 필드 없음, passed/failed/unknown 있음
curl -s "http://127.0.0.1:7879/v1/sessions/$S/usage"   | python3 -m json.tool   # assistant_events=2683, user_turns=43, cache_hit_ratio 없음
```
Expected: `/metrics`에 `*_rate`/`cache_hit_ratio` 부재, verification count 3종 존재; `/usage`에 `assistant_events`·`user_turns`, `turns`/`cache_hit_ratio` 부재.

- [ ] **Step 4: 브라우저 smoke (CLAUDE.md 의무)**

claude-in-chrome으로 `http://127.0.0.1:7879` 접속 → 세션 1b30ced8 열기 → AnalysisPanel에 "검증 통과 (측정분)"이 measured 분모 + 미측정 카운트로 표시되는지, insight-strip의 캐시/턴 표기가 정상인지 시각 확인. 서버 종료.

- [ ] **Step 5: 커밋(없으면 생략)** — 검증 전용이라 코드 변경 없으면 커밋 불필요.

---

## Self-Review 메모

- **Spec 커버리지:** F1(rate 삭제+count+turns 개명) = Task 2·3·4·5; F2(Tier-2 제거) = Task 1; 검증 = Task 6. F1-3(verification_run turn_id)·F1-4(baseline)는 선택/경미 — baseline turns 라벨만 Task 3 Step 7에 포함.
- **타입 일관성:** `assistant_events`/`user_turns`/`verification_failed`/`verification_unknown` 명칭을 Rust(dto.rs)·TS(types.ts)·테스트에서 동일 사용. `turns`/`*_rate`/`cache_hit_ratio`(`/metrics`·`/usage`) 전부 제거.
- **placeholder:** 테스트의 seed 헬퍼는 각 테스트 파일의 기존 패턴(`seed_event`/`seed_usage_session`)을 재사용하라고 명시 — 단언과 production 코드는 완전 제공.
- **blast radius:** baseline의 `cache_hit_ratio` 분위수는 유지(분포 통계, rate scalar 아님). 프런트 캐시 표기는 `/usage` 토큰 component로 계산.
