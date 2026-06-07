# Behavioral Metrics Backend (Plan 3a) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`).

**Goal:** 세션 단위 **결정적 행동 지표(behavioral_metrics)**를 **온디맨드**로 계산하는 read-only API. events·signals·verification_runs·usage_facet을 집계한 사실/카운트/비율 + detector 신호분포. severity/판단 없음(spec 원칙 3).

**Architecture:** `src/insight/metrics.rs` 신규 — `compute_session_metrics(pool, session_id) -> SessionMetrics`. 기존 repo들을 1회씩 로드해 순수 집계(결정적·단일 패스). 저장 테이블 없음 — **호출 시 계산**(spec §10.1 온디맨드; 캐시·구간 range는 후속). `GET /v1/sessions/:id/metrics` → `SessionMetricsDto`. **구간 분석 뷰(프론트)는 Plan 3b**.

**Tech Stack:** Rust, sqlx, axum.

**Spec:** `2026-06-07-detail-view-derived-metrics-design.md` §4.2(카탈로그)·§5.2·§6.6(신호분포)·§10.1(온디맨드). 두 표면 분리(§8.3): 이건 구간 분석 표면용 데이터, replay(디테일)와 별개.

**1차 지표 (기존 데이터에서 결정적 집계):**
- `tool_call_total`, `tool_failure_count`(detector=tool_failure signal 수), `tool_failure_rate`
- `verification_total`, `verification_passed`, `verification_pass_rate`
- `context_bloat_count`(detector=context_bloat signal 수)
- `cache_hit_ratio`(usage_facet: cache_read / (input+cache_read+cache_creation))
- `detector_firing`: detector→count 맵 (신호분포 §6.6)
- 모두 사실/비율. 임계값·severity 없음.

---

## File Structure
- Create: `src/insight/metrics.rs` — `SessionMetrics` + `compute_session_metrics`
- Modify: `src/insight/mod.rs` (add `pub mod metrics;`)
- Modify: `src/api/dto.rs` — `SessionMetricsDto`
- Modify: `src/api/routes.rs` — `session_metrics` handler
- Modify: `src/api/mod.rs` — route `/v1/sessions/:id/metrics`
- Test: `tests/metrics_compute.rs`, `tests/api_metrics.rs`

---

## Task 1: SessionMetrics + compute_session_metrics

**Files:** `src/insight/metrics.rs`, `src/insight/mod.rs`, `tests/metrics_compute.rs`

- [ ] **Step 1: 실패 테스트** `tests/metrics_compute.rs`
```rust
use wimcc::insight::metrics::compute_session_metrics;

#[tokio::test]
async fn aggregates_tool_failure_and_detector_firing() {
    let pool = wimcc::db::test_pool().await; // 기존 헬퍼 (repo_signal.rs 테스트 패턴)
    // seed: 2 tool_call events + 1 tool_failure signal
    seed_tool_call(&pool, "s1", "tc1").await;
    seed_tool_call(&pool, "s1", "tc2").await;
    seed_signal(&pool, "s1", "tool_failure").await;
    let m = compute_session_metrics(&pool, "s1").await.unwrap();
    assert_eq!(m.tool_call_total, 2);
    assert_eq!(m.tool_failure_count, 1);
    assert!((m.tool_failure_rate - 0.5).abs() < 1e-9);
    assert_eq!(m.detector_firing.get("tool_failure"), Some(&1));
}
```
(seed helper는 repo_observed::insert / repo_signal::insert 사용 — 기존 테스트의 seed 패턴 참조.)

- [ ] **Step 2: 실패 확인** `cargo test --test metrics_compute` → FAIL (미정의).

- [ ] **Step 3: 구현** `src/insight/metrics.rs`
```rust
//! Deterministic session behavioral metrics (spec §4.2/§5.2). On-demand
//! aggregation over events/signals/verification/usage. Facts/counts/ratios
//! only — no severity/judgment (§6.3). No threshold magic numbers (§6.1).
use std::collections::BTreeMap;
use sqlx::SqlitePool;
use crate::error::Result;
use crate::db::{repo_observed, repo_signal, repo_verification_run, repo_usage_facet};
use crate::model::observed::EventKind;

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionMetrics {
    pub session_id: String,
    pub tool_call_total: i64,
    pub tool_failure_count: i64,
    pub tool_failure_rate: f64,
    pub verification_total: i64,
    pub verification_passed: i64,
    pub verification_pass_rate: f64,
    pub context_bloat_count: i64,
    pub cache_hit_ratio: f64,
    /// detector → fired signal count (신호분포, §6.6)
    pub detector_firing: BTreeMap<String, i64>,
}

fn rate(num: i64, den: i64) -> f64 { if den == 0 { 0.0 } else { num as f64 / den as f64 } }

pub async fn compute_session_metrics(pool: &SqlitePool, session_id: &str) -> Result<SessionMetrics> {
    let events = repo_observed::list_session(pool, session_id, 100_000).await?;
    let signals = repo_signal::list_by_session(pool, session_id).await?;
    let vruns = repo_verification_run::list_session(pool, session_id).await?;
    // usage: reuse existing per-session usage aggregation (repo_usage_facet).
    // Inspect repo_usage_facet for the available aggregate fn; sum tokens.

    let tool_call_total = events.iter().filter(|e| e.kind == EventKind::ToolCall).count() as i64;

    let mut detector_firing: BTreeMap<String, i64> = BTreeMap::new();
    for s in &signals { *detector_firing.entry(s.detector.clone()).or_insert(0) += 1; }
    let tool_failure_count = *detector_firing.get("tool_failure").unwrap_or(&0);
    let context_bloat_count = *detector_firing.get("context_bloat").unwrap_or(&0);

    let verification_total = vruns.len() as i64;
    let verification_passed = vruns.iter().filter(|v| v.status == "passed").count() as i64;

    // cache_hit_ratio from usage_facet rows for this session.
    let (mut cache_read, mut total_in) = (0i64, 0i64);
    let usage_rows = repo_usage_facet::list_session(pool, session_id).await?; // confirm fn name
    for u in &usage_rows {
        cache_read += u.cache_read_input_tokens;
        total_in += u.input_tokens + u.cache_read_input_tokens + u.cache_creation_input_tokens;
    }

    Ok(SessionMetrics {
        session_id: session_id.to_string(),
        tool_call_total,
        tool_failure_count,
        tool_failure_rate: rate(tool_failure_count, tool_call_total),
        verification_total,
        verification_passed,
        verification_pass_rate: rate(verification_passed, verification_total),
        context_bloat_count,
        cache_hit_ratio: rate(cache_read, total_in),
        detector_firing,
    })
}
```
> 주: `repo_usage_facet`/`repo_verification_run`의 실제 list fn 이름·필드를 열어 확인하고 맞춘다(시그니처가 다르면 조정). usage 집계 fn이 이미 있으면 재사용.

- [ ] **Step 4: mod 등록 + 테스트 통과** `cargo test --test metrics_compute` → PASS.

- [ ] **Step 5: Commit**
```bash
git add src/insight/metrics.rs src/insight/mod.rs tests/metrics_compute.rs
git commit -m "feat(insight): on-demand session behavioral metrics (deterministic aggregation)"
```

---

## Task 2: GET /v1/sessions/:id/metrics

**Files:** `src/api/dto.rs`, `src/api/routes.rs`, `src/api/mod.rs`, `tests/api_metrics.rs`

- [ ] **Step 1: DTO** `src/api/dto.rs` — `SessionMetrics`는 이미 `Serialize`라 그대로 `data`로 감싸도 됨. 래퍼:
```rust
#[derive(Serialize)]
pub struct SessionMetricsResponse { pub data: crate::insight::metrics::SessionMetrics }
```

- [ ] **Step 2: 핸들러** `src/api/routes.rs`
```rust
pub async fn session_metrics(State(pool): State<SqlitePool>, Path(id): Path<String>) -> impl IntoResponse {
    match crate::insight::metrics::compute_session_metrics(&pool, &id).await {
        Ok(m) => Json(SessionMetricsResponse { data: m }).into_response(),
        Err(err) => { tracing::error!(err=%err, "session_metrics failed");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":"internal server error"}))).into_response() }
    }
}
```

- [ ] **Step 3: route** `src/api/mod.rs` — `.route("/v1/sessions/:id/metrics", get(routes::session_metrics))` (authed router에).

- [ ] **Step 4: 통합 테스트** `tests/api_metrics.rs` — seed 후 `GET /v1/sessions/s1/metrics` → 200, `data.tool_failure_rate` 등 확인. (기존 api 통합 테스트 하니스 패턴.)

- [ ] **Step 5: build + test** `cargo build && cargo test --test api_metrics --test metrics_compute` → PASS.

- [ ] **Step 6: Commit**
```bash
git add src/api/
git add tests/api_metrics.rs
git commit -m "feat(api): GET /v1/sessions/:id/metrics (on-demand behavioral metrics)"
```

---

## Task 3: 검증 + 재ingest 스모크

- [ ] **Step 1: 전체 테스트** `cargo test` → 0 fail.
- [ ] **Step 2: clippy** `cargo clippy --all-targets 2>&1 | tail` — 새 경고 0.
- [ ] **Step 3: 실데이터 확인** serve(별도 포트) 후 `curl localhost:<port>/v1/sessions/653ea169-.../metrics | jq` → tool_failure_rate·detector_firing 등 합리적 값(653ea169은 tool_failure 多). 또는 `cargo test` 통합으로 충분하면 생략.
- [ ] **Step 4: 구현노트** `docs/implementation-notes.html`에 behavioral_metrics 온디맨드 집계 노트.
```bash
git add docs/implementation-notes.html && git commit -m "docs: note behavioral_metrics on-demand aggregation (Plan 3a)"
```

---

## Self-Review 메모
- 온디맨드 계산이라 저장 테이블·마이그레이션 없음. 캐시는 조회 빈번 확인 후(§10.1). 구간(range) 파라미터·MCP 노출은 후속(Plan 3b/4).
- `repo_usage_facet`/`repo_verification_run` list fn 시그니처를 실제로 열어 맞출 것(이 plan의 fn명은 추정 — 다르면 조정, 추정 일반화 금지).
- detector_firing은 신호분포 메타지표의 1차 형태(§6.6). 발화율(이벤트 대비)은 detector별 분모가 달라 후속 정교화.
- severity/임계값 없음 — 비율·카운트 사실만. "나쁜가"는 소비자(LLM/사람) 판단.
