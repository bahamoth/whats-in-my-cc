# Signal Backend Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 해석적 `finding`을 결정적 `signal`로 전환한다 — severity/confidence/benign/internal 판단을 제거하고 사실(facts)만 남기며, detector 파라미터를 외부 TOML config로 빼고, `DiffHunk` EventKind 잔재를 제거한다.

**Architecture:** signal 테이블 신규 생성 + finding 테이블 폐기(migration 0021). `SignalCandidate`/`SignalRow`는 severity/confidence 없이 `facts: JSON`만 담는다. 기존 4개 extractor(tool_failure·risky_action·context_bloat·final_state_mismatch)를 signal 생산으로 전환하면서 tool_failure의 가정 3종(severity 판단·BENIGN_EXIT_MARKERS·INTERNAL_RETRY_TOOLS+RETRY_WINDOW)을 제거한다. detector 파라미터는 `DetectorConfig`(TOML + 코드 fallback)로 외부화. API는 `/v1/signals`로 노출. **프론트 전환은 Plan 2(디테일 뷰)에서 다룬다 — 이 plan은 백엔드 코어만.**

**Tech Stack:** Rust, sqlx, SQLite, axum, serde, toml.

**Spec:** `docs/superpowers/specs/2026-06-07-detail-view-derived-metrics-design.md` §5.3, §6.1~6.3, §10.2·3·4·6.

---

## File Structure

- Create: `src/insight/config.rs` — `DetectorConfig` TOML 로드 + fallback
- Create: `migrations/20260607140000_0021_signal.sql` — signal 테이블 + finding drop
- Create: `src/db/repo_signal.rs` — `SignalRow` + insert/list/get
- Modify: `src/insight/types.rs` — `SignalCandidate`(severity/confidence 제거, facts 추가)
- Modify: `src/insight/extractor.rs` — trait 반환을 `Vec<SignalCandidate>`로
- Modify: `src/insight/pipeline.rs` — `run_detectors`(구 run_extractors), `build_signal_row`
- Modify: `src/insight/extractors/{tool_failure,risky_action,context_bloat,final_state_mismatch}.rs`
- Modify: `src/model/observed.rs` — `EventKind::DiffHunk` 제거
- Modify: `src/db/repo_observed.rs`, `src/api/sse.rs` — DiffHunk 매핑 제거
- Modify: `src/api/routes.rs`, `src/api/dto.rs`, `src/api/mod.rs` — `/v1/signals` 라우트 + `SignalDto`
- Modify: `src/ingest/store.rs` — `run_extractors` 호출명 변경
- Test: `tests/detector_config.rs`, `tests/repo_signal.rs`, 기존 `tests/extractor_*.rs` 갱신

---

## Task 1: DetectorConfig 인프라 (TOML + fallback)

**Files:**
- Create: `src/insight/config.rs`
- Test: `tests/detector_config.rs`
- Modify: `src/insight/mod.rs` (add `pub mod config;`)
- Modify: `Cargo.toml` (add `toml` dep if absent)

- [ ] **Step 1: `toml` 의존성 확인/추가**

Run: `grep '^toml' Cargo.toml || cargo add toml`
Expected: `toml` in `[dependencies]`.

- [ ] **Step 2: 실패 테스트 작성** — `tests/detector_config.rs`

```rust
use wimcc::insight::config::DetectorConfig;

#[test]
fn defaults_when_no_file() {
    let cfg = DetectorConfig::from_toml_str("");
    // tool_failure enabled by default, retry_window default 5
    assert!(cfg.enabled("tool_failure"));
    assert_eq!(cfg.usize_param("tool_failure", "retry_window", 5), 5);
}

#[test]
fn override_and_fallback() {
    let cfg = DetectorConfig::from_toml_str(
        "[detector.tool_failure]\nenabled = false\nretry_window = 9\n",
    );
    assert!(!cfg.enabled("tool_failure"));
    assert_eq!(cfg.usize_param("tool_failure", "retry_window", 5), 9);
    // missing detector → enabled default true, missing key → fallback
    assert!(cfg.enabled("risky_action"));
    assert_eq!(cfg.usize_param("risky_action", "window", 7), 7);
}
```

- [ ] **Step 3: 테스트 실패 확인**

Run: `cargo test --test detector_config`
Expected: FAIL — `DetectorConfig` 미정의 (compile error).

- [ ] **Step 4: 구현** — `src/insight/config.rs`

```rust
//! Detector configuration (rule pack). Parameters only — predicate logic stays
//! in code (versioned, like redaction rule_pack). TOML format; missing
//! file/section/key falls back to per-detector code defaults (spec §10.4).
use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct DetectorConfig {
    /// detector_id → (enabled, params map of String→toml value)
    sections: HashMap<String, toml::Table>,
}

impl DetectorConfig {
    /// Parse from a TOML string. Shape: `[detector.<id>] enabled = .. key = ..`.
    /// An empty/invalid string yields all-defaults.
    pub fn from_toml_str(s: &str) -> Self {
        let root: toml::Table = toml::from_str(s).unwrap_or_default();
        let mut sections = HashMap::new();
        if let Some(toml::Value::Table(dets)) = root.get("detector") {
            for (id, v) in dets {
                if let toml::Value::Table(t) = v {
                    sections.insert(id.clone(), t.clone());
                }
            }
        }
        Self { sections }
    }

    /// Enabled unless explicitly `enabled = false`. Missing detector → true.
    pub fn enabled(&self, detector: &str) -> bool {
        self.sections
            .get(detector)
            .and_then(|t| t.get("enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
    }

    /// usize param with code-supplied fallback.
    pub fn usize_param(&self, detector: &str, key: &str, fallback: usize) -> usize {
        self.sections
            .get(detector)
            .and_then(|t| t.get(key))
            .and_then(|v| v.as_integer())
            .map(|i| i.max(0) as usize)
            .unwrap_or(fallback)
    }
}
```

- [ ] **Step 5: `mod.rs` 등록** — `src/insight/mod.rs`에 `pub mod config;` 추가.

- [ ] **Step 6: 테스트 통과 확인**

Run: `cargo test --test detector_config`
Expected: PASS (2 tests).

- [ ] **Step 7: Commit**

```bash
git add src/insight/config.rs src/insight/mod.rs tests/detector_config.rs Cargo.toml Cargo.lock
git commit -m "feat(insight): DetectorConfig (TOML rule pack + code fallback)"
```

---

## Task 2: signal 테이블 migration (0021)

**Files:**
- Create: `migrations/20260607140000_0021_signal.sql`

- [ ] **Step 1: 마이그레이션 작성**

```sql
-- Plan 1: finding → signal. severity/confidence(판단)를 제거하고 facts(사실)만 남긴다.
-- finding 테이블 폐기(신규+폐기, spec §10.2).
CREATE TABLE IF NOT EXISTS signal (
    signal_id           TEXT PRIMARY KEY,
    schema_version      TEXT NOT NULL DEFAULT 'signal.v1',
    session_id          TEXT NOT NULL,
    detector            TEXT NOT NULL,            -- detector id (구 category)
    subkind             TEXT,                     -- optional, 사실 분류 (해석 아님)
    summary             TEXT NOT NULL,            -- 사실 요약 (판단어 없음)
    evidence_refs       TEXT NOT NULL,            -- JSON array of event_id
    facts               TEXT NOT NULL,            -- JSON object — 결정적 사실 projection
    provenance          TEXT NOT NULL,            -- JSON: { detector, version, rule_pack }
    created_at          TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_signal_session  ON signal(session_id);
CREATE INDEX IF NOT EXISTS idx_signal_detector ON signal(detector);
CREATE INDEX IF NOT EXISTS idx_signal_sess_det ON signal(session_id, detector);

DROP TABLE IF EXISTS finding;
```

- [ ] **Step 2: DB 재생성 확인**

Run: `cargo run -- init-db`
Expected: 성공, 새 `signal` 테이블 생성, `finding` 없음.

- [ ] **Step 3: Commit**

```bash
git add migrations/20260607140000_0021_signal.sql
git commit -m "feat(db): migration 0021 — signal table, drop finding"
```

---

## Task 3: SignalCandidate + SignalRow + repo_signal

**Files:**
- Modify: `src/insight/types.rs`
- Create: `src/db/repo_signal.rs`
- Modify: `src/db/mod.rs` (add `pub mod repo_signal;`)
- Modify: `src/ids.rs` (rename `derive_finding_id` → `derive_signal_id`)
- Test: `tests/repo_signal.rs`

- [ ] **Step 1: `types.rs` — SignalCandidate (severity/confidence 제거)**

기존 `FindingCandidate`를 대체:

```rust
/// A deterministic signal produced by a detector. NO severity/confidence —
/// those are judgments (spec §6.3). Only facts. evidence_refs must be non-empty.
#[derive(Debug, Clone)]
pub struct SignalCandidate {
    pub detector: &'static str,
    pub subkind: Option<&'static str>,   // 사실 분류만 (해석 아님)
    pub summary: String,                  // 사실 요약 (판단어 없음)
    pub evidence_refs: Vec<String>,
    pub facts: serde_json::Value,         // 결정적 사실 projection
}

#[derive(Debug, Clone)]
pub struct Provenance {
    pub detector: String,                 // "<id>@v1"
    pub version: &'static str,            // "L1" 유지 (deterministic)
    pub rule_pack: Option<String>,
}

impl Provenance {
    pub fn to_json_string(&self) -> String {
        serde_json::json!({
            "detector": self.detector,
            "version": self.version,
            "rule_pack": self.rule_pack,
        })
        .to_string()
    }
}
```

- [ ] **Step 2: `ids.rs` — derive_signal_id**

`derive_finding_id`를 rename(시그니처 동일, prefix `sig_`):

```rust
pub fn derive_signal_id(detector: &str, session_id: &str, evidence_refs: &[String]) -> String {
    let mut refs: Vec<&str> = evidence_refs.iter().map(|s| s.as_str()).collect();
    refs.sort_unstable();
    let mut h = Sha256::new();
    h.update(detector.as_bytes());
    h.update(b"\x00");
    h.update(session_id.as_bytes());
    h.update(b"\x00");
    h.update(refs.join(",").as_bytes());
    format!("sig_{}", hex::encode(&h.finalize()[..12]))
}
```

- [ ] **Step 3: 실패 테스트** — `tests/repo_signal.rs`

```rust
use wimcc::db::repo_signal::{self, SignalRow};

#[tokio::test]
async fn insert_and_list_by_session() {
    let pool = wimcc::db::test_pool().await; // 기존 테스트 헬퍼 패턴 사용
    let row = SignalRow {
        signal_id: "sig_abc".into(),
        schema_version: "signal.v1".into(),
        session_id: "sess_1".into(),
        detector: "tool_failure".into(),
        subkind: None,
        summary: "Tool Bash returned is_error=true".into(),
        evidence_refs: "[\"ev_1\"]".into(),
        facts: "{\"is_error\":true}".into(),
        provenance: "{\"detector\":\"tool_failure@v1\"}".into(),
        created_at: "2026-06-07T00:00:00Z".into(),
    };
    repo_signal::insert(&pool, &row).await.unwrap();
    let rows = repo_signal::list_by_session(&pool, "sess_1").await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].detector, "tool_failure");
}
```

> 주: `test_pool` 헬퍼가 없으면 기존 `tests/repo_finding*.rs` 또는 `src/db/mod.rs`의 인메모리 풀 생성 패턴을 그대로 따른다 (조사에서 확인된 sqlx `SqlitePool::connect("sqlite::memory:")` + migrate).

- [ ] **Step 4: 테스트 실패 확인**

Run: `cargo test --test repo_signal`
Expected: FAIL — `repo_signal` 미정의.

- [ ] **Step 5: 구현** — `src/db/repo_signal.rs`

(repo_finding.rs 구조를 그대로 따르되 severity/confidence/status 제거, facts 추가)

```rust
use sqlx::{Row, SqlitePool};
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct SignalRow {
    pub signal_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub detector: String,
    pub subkind: Option<String>,
    pub summary: String,
    pub evidence_refs: String,
    pub facts: String,
    pub provenance: String,
    pub created_at: String,
}

pub async fn insert(pool: &SqlitePool, row: &SignalRow) -> Result<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO signal \
         (signal_id, schema_version, session_id, detector, subkind, summary, \
          evidence_refs, facts, provenance, created_at) \
         VALUES (?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&row.signal_id).bind(&row.schema_version).bind(&row.session_id)
    .bind(&row.detector).bind(&row.subkind).bind(&row.summary)
    .bind(&row.evidence_refs).bind(&row.facts).bind(&row.provenance)
    .bind(&row.created_at)
    .execute(pool).await?;
    Ok(())
}

fn map_row(r: sqlx::sqlite::SqliteRow) -> SignalRow {
    SignalRow {
        signal_id: r.get("signal_id"),
        schema_version: r.get("schema_version"),
        session_id: r.get("session_id"),
        detector: r.get("detector"),
        subkind: r.get("subkind"),
        summary: r.get("summary"),
        evidence_refs: r.get("evidence_refs"),
        facts: r.get("facts"),
        provenance: r.get("provenance"),
        created_at: r.get("created_at"),
    }
}

pub async fn list_by_session(pool: &SqlitePool, session_id: &str) -> Result<Vec<SignalRow>> {
    let rows = sqlx::query("SELECT * FROM signal WHERE session_id=? ORDER BY created_at DESC")
        .bind(session_id).fetch_all(pool).await?;
    Ok(rows.into_iter().map(map_row).collect())
}

pub async fn get(pool: &SqlitePool, signal_id: &str) -> Result<Option<SignalRow>> {
    let row = sqlx::query("SELECT * FROM signal WHERE signal_id=?")
        .bind(signal_id).fetch_optional(pool).await?;
    Ok(row.map(map_row))
}
```

- [ ] **Step 6: `db/mod.rs` 등록** + 테스트 통과

Run: `cargo test --test repo_signal`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/insight/types.rs src/db/repo_signal.rs src/db/mod.rs src/ids.rs tests/repo_signal.rs
git commit -m "feat(insight): SignalCandidate + repo_signal (drop severity/confidence)"
```

---

## Task 4: signal pipeline (run_detectors)

**Files:**
- Modify: `src/insight/extractor.rs` (trait → `Vec<SignalCandidate>`, `detect` + config)
- Modify: `src/insight/pipeline.rs`
- Modify: `src/ingest/store.rs:264` (호출명)

- [ ] **Step 1: trait 갱신** — `src/insight/extractor.rs`

```rust
use crate::insight::config::DetectorConfig;
use crate::insight::types::SignalCandidate;
use crate::insight::view::SessionInsightView;

pub trait Detector: Send + Sync {
    /// Stable detector id (구 category) — appears in `signal.detector`.
    fn id(&self) -> &'static str;
    /// Pure CPU detection. Deterministic. No severity/confidence — facts only.
    fn detect(&self, view: &SessionInsightView<'_>, cfg: &DetectorConfig) -> Vec<SignalCandidate>;
}
```

> `InsightExtractor` trait·`category`·`floor`·`FindingCandidate`는 모두 제거된다.

- [ ] **Step 2: pipeline 갱신** — `src/insight/pipeline.rs`

```rust
use sqlx::SqlitePool;
use crate::db::repo_signal::{self, SignalRow};
use crate::error::Result;
use crate::ids::derive_signal_id;
use crate::insight::config::DetectorConfig;
use crate::insight::types::{Provenance, SignalCandidate};
use crate::insight::view::OwnedSessionInsightData;

/// Deterministic detector pipeline. Idempotent (INSERT OR REPLACE).
pub async fn run_detectors(pool: &SqlitePool, session_id: &str) -> Result<Vec<SignalRow>> {
    let cfg = DetectorConfig::default(); // Plan 4에서 파일 로드로 교체; 지금은 코드 default
    let data = OwnedSessionInsightData::load(pool, session_id).await?;
    let view = data.as_view(session_id);
    let mut rows = Vec::new();
    for det in all_detectors() {
        if !cfg.enabled(det.id()) { continue; }
        let cands = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| det.detect(&view, &cfg))) {
            Ok(c) => c,
            Err(_) => { tracing::warn!(session_id, id = det.id(), "detector panicked; skipping"); continue; }
        };
        for c in cands {
            let row = build_signal_row(session_id, &c);
            repo_signal::insert(pool, &row).await?;
            rows.push(row);
        }
    }
    Ok(rows)
}

fn build_signal_row(session_id: &str, c: &SignalCandidate) -> SignalRow {
    let prov = Provenance { detector: format!("{}@v1", c.detector), version: "L1", rule_pack: None };
    SignalRow {
        signal_id: derive_signal_id(c.detector, session_id, &c.evidence_refs),
        schema_version: "signal.v1".into(),
        session_id: session_id.to_string(),
        detector: c.detector.to_string(),
        subkind: c.subkind.map(|s| s.to_string()),
        summary: c.summary.clone(),
        evidence_refs: serde_json::to_string(&c.evidence_refs).unwrap_or_else(|_| "[]".into()),
        facts: c.facts.to_string(),
        provenance: prov.to_json_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    }
}

fn all_detectors() -> Vec<Box<dyn crate::insight::extractor::Detector>> {
    use crate::insight::extractors::{
        context_bloat::ContextBloat, final_state_mismatch::FinalStateMismatch,
        risky_action::RiskyAction, tool_failure::ToolFailure,
    };
    vec![Box::new(ToolFailure), Box::new(RiskyAction), Box::new(ContextBloat), Box::new(FinalStateMismatch)]
}
```

> `CONFIDENCE_FLOOR`·`registry.rs`는 제거. `registry.rs`의 `all_extractors`를 쓰는 곳이 있으면 `all_detectors`로 정리.

- [ ] **Step 3: store.rs 호출명 변경** — `src/ingest/store.rs:264`

`crate::insight::pipeline::run_extractors(pool, session_id).await?;`
→ `crate::insight::pipeline::run_detectors(pool, session_id).await?;`

- [ ] **Step 4: 컴파일 확인 (extractor 미전환이라 실패 예상)**

Run: `cargo build 2>&1 | head -30`
Expected: 4개 extractor가 아직 구 trait이라 컴파일 에러 — Task 5·6에서 해소. (이 단계는 빌드 그린이 아님; Task 6 끝에서 그린.)

- [ ] **Step 5: Commit (WIP, 컴파일 미완 허용 — 다음 task와 한 흐름)**

```bash
git add src/insight/extractor.rs src/insight/pipeline.rs src/ingest/store.rs
git commit -m "feat(insight): signal pipeline (run_detectors) — extractor port follows"
```

---

## Task 5: tool_failure → signal (가정 3종 제거)

**Files:**
- Modify: `src/insight/extractors/tool_failure.rs`
- Modify: `tests/extractor_tool_failure.rs`

- [ ] **Step 1: 테스트 갱신 (사실만 검증)** — `tests/extractor_tool_failure.rs`

`classify_failure`/`FailureClass`/severity 검증을 제거하고, 사실 검증으로 교체:

```rust
use wimcc::insight::config::DetectorConfig;
use wimcc::insight::extractor::Detector;
use wimcc::insight::extractors::tool_failure::ToolFailure;

#[test]
fn fires_on_is_error_true_with_no_retry() {
    let events = vec![tool_call_ev(0, "tid_0", "Bash"), tool_result_ev(1, "tid_0", true)];
    let view = view_from_events(&events);
    let cands = ToolFailure.detect(&view, &DetectorConfig::default());
    assert_eq!(cands.len(), 1);
    assert_eq!(cands[0].detector, "tool_failure");
    assert!(!cands[0].evidence_refs.is_empty());
    // facts carry raw is_error + tool_name; NO severity/class judgment
    assert_eq!(cands[0].facts["is_error"], serde_json::json!(true));
    assert_eq!(cands[0].facts["tool_name"], serde_json::json!("Bash"));
}

#[test]
fn retry_window_from_config() {
    // 성공 재시도가 기본 윈도우(5) 밖이면 발화, config로 늘리면 미발화
    let mut events = vec![tool_call_ev(0, "tid", "Bash"), tool_result_ev(1, "tid", true)];
    for i in 2..8 { events.push(base_filler(i)); }
    events.push(tool_result_ev(8, "tid", false)); // 성공 재시도 (거리 7)
    let view = view_from_events(&events);
    assert_eq!(ToolFailure.detect(&view, &DetectorConfig::default()).len(), 1); // window=5 → 발화
    let cfg = DetectorConfig::from_toml_str("[detector.tool_failure]\nretry_window = 10\n");
    assert_eq!(ToolFailure.detect(&view, &cfg).len(), 0); // window=10 → 미발화
}
```

> `base_filler(i)`는 `base_event(i, Actor::Assistant, EventKind::AssistantMessage)` 헬퍼.

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test --test extractor_tool_failure 2>&1 | head`
Expected: FAIL (compile — `Detector`/`facts` 미구현).

- [ ] **Step 3: 구현 — 가정 3종 제거**

`tool_failure.rs`에서 삭제: `INTERNAL_RETRY_TOOLS`, `BENIGN_EXIT_MARKERS`, `FailureClass` enum 전체, `classify_failure`, `const RETRY_WINDOW`. trait 구현을 `Detector`로 교체:

```rust
use crate::insight::config::DetectorConfig;
use crate::insight::extractor::Detector;
use crate::insight::types::SignalCandidate;
use crate::insight::view::SessionInsightView;
use crate::model::observed::EventKind;
use serde_json::json;

pub struct ToolFailure;

const RETRY_WINDOW_DEFAULT: usize = 5;

impl Detector for ToolFailure {
    fn id(&self) -> &'static str { "tool_failure" }

    fn detect(&self, view: &SessionInsightView<'_>, cfg: &DetectorConfig) -> Vec<SignalCandidate> {
        let window = cfg.usize_param("tool_failure", "retry_window", RETRY_WINDOW_DEFAULT);
        let events = view.events;
        let mut out = Vec::new();
        let mut emitted = std::collections::HashSet::new();
        for (i, ev) in events.iter().enumerate() {
            if ev.kind != EventKind::ToolResult { continue; }
            let is_error = ev.payload.pointer("/tool_result/is_error").and_then(|v| v.as_bool()).unwrap_or(false);
            if !is_error { continue; }
            let tid = ev.tool_use_id.clone();
            if let Some(ref t) = tid { if emitted.contains(t) { continue; } }
            // forward window: same tool_use_id success → retried (fact, not judgment)
            let retried = tid.as_ref().map(|t| {
                let end = (i + 1 + window).min(events.len());
                events[i+1..end].iter().any(|e2|
                    e2.kind == EventKind::ToolResult
                    && e2.tool_use_id.as_deref() == Some(t.as_str())
                    && !e2.payload.pointer("/tool_result/is_error").and_then(|v| v.as_bool()).unwrap_or(false))
            }).unwrap_or(false);
            // find paired call
            let call = events[..i].iter().rev().find(|e2|
                e2.kind == EventKind::ToolCall && e2.tool_use_id == tid);
            let tool_name = call.and_then(|e| e.tool_name.as_deref())
                .or(ev.tool_name.as_deref()).unwrap_or("unknown");
            let error_excerpt: String = ev.payload.pointer("/tool_result/content")
                .and_then(|v| v.as_str()).unwrap_or("").chars().take(512).collect();
            let mut refs = vec![ev.event_id.clone()];
            if let Some(c) = call { refs.push(c.event_id.clone()); }
            // facts ONLY — exit/benign/internal 판단 없음. 원자료를 그대로 노출.
            let facts = json!({
                "is_error": true,
                "retried": retried,
                "tool_name": tool_name,
                "tool_use_id": tid,
                "error_excerpt": error_excerpt,
                "tool_result_event_id": ev.event_id,
                "paired_call_event_id": call.map(|c| c.event_id.clone()),
            });
            let summary = format!("Tool {tool_name} returned is_error=true (retried={retried}).");
            out.push(SignalCandidate { detector: "tool_failure", subkind: None, summary, evidence_refs: refs, facts });
            if let Some(t) = tid { emitted.insert(t); }
        }
        out
    }
}
```

> 변경 핵심: severity/FailureClass/benign/internal 전부 제거. `retried`·`error_excerpt`·`tool_name`을 **사실로** 노출 — "benign인가/internal인가"는 LLM·사람이 판단(spec §6.3). retry_window는 config.

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test --test extractor_tool_failure`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/insight/extractors/tool_failure.rs tests/extractor_tool_failure.rs
git commit -m "refactor(insight): tool_failure → signal, drop 3 assumptions (severity/benign/internal)"
```

---

## Task 6: risky_action · context_bloat · final_state_mismatch → signal

**Files:**
- Modify: 위 3개 extractor + 각 `tests/extractor_*.rs`

각 detector를 `Detector` trait으로 포팅한다. 패턴 동일: `category()/floor()` 제거, `id()` 추가, `extract(view)` → `detect(view, cfg)`, `FindingCandidate{severity,confidence_l1,evidence_projection}` → `SignalCandidate{facts}` (severity 제거, `evidence_projection`을 `facts`로 그대로 이름만). 임계값 상수는 config로:

- `risky_action`: `DESTRUCTIVE_PATTERNS`는 코드 유지(룰 로직). severity 제거. `facts`에 trigger/command_redacted 그대로.
- `context_bloat`: `BLOAT_THRESHOLD_BYTES`·`NEXT_EVENT_WINDOW`·`MIN_OVERLAP_STEMS`를 `cfg.usize_param("context_bloat", ...)`로. severity 제거.
- `final_state_mismatch`: severity 제거. `facts`에 goal/final_state 그대로.

- [ ] **Step 1: risky_action 테스트 갱신** — severity assert 제거, `detect(&view, &cfg)` + `detector=="risky_action"` 검증.

```rust
#[test]
fn fires_on_destructive_bash() {
    let events = vec![bash_call(0, "rm -rf /tmp/x")];
    let cands = RiskyAction.detect(&view_from_events(&events), &DetectorConfig::default());
    assert_eq!(cands.len(), 1);
    assert_eq!(cands[0].detector, "risky_action");
    assert!(cands[0].facts["trigger"]["kind"] == serde_json::json!("destructive_bash"));
}
```

- [ ] **Step 2: risky_action 포팅** (위 패턴; `confidence_l1`/`severity` 줄 삭제, `evidence_projection:` → `facts:`, trait `Detector`).

- [ ] **Step 3: context_bloat 테스트 갱신 + 포팅** (임계값 config화: `cfg.usize_param("context_bloat","threshold_bytes",50*1024)` 등).

- [ ] **Step 4: final_state_mismatch 테스트 갱신 + 포팅** (severity 제거).

- [ ] **Step 5: 전체 빌드 + 인사이트 테스트 그린**

Run: `cargo build && cargo test --test extractor_tool_failure --test extractor_risky_action --test extractor_context_bloat --test extractor_final_state_mismatch`
Expected: PASS, 빌드 그린 (Task 4의 컴파일 에러 해소됨).

- [ ] **Step 6: Commit**

```bash
git add src/insight/extractors/ tests/extractor_*.rs
git commit -m "refactor(insight): port risky_action/context_bloat/final_state_mismatch to signal"
```

---

## Task 7: DiffHunk EventKind 잔재 제거

**Files:**
- Modify: `src/model/observed.rs`, `src/db/repo_observed.rs`, `src/api/sse.rs`

- [ ] **Step 1: 실패 테스트** — `tests/event_kind_no_diffhunk.rs`

```rust
use wimcc::model::observed::EventKind;
use strum::IntoEnumIterator;

#[test]
fn diffhunk_variant_removed() {
    // DiffHunk는 사이드테이블 전용 — EventKind enum에 있으면 안 됨 (spec §10.3).
    assert!(EventKind::iter().all(|k| k.as_str() != "diff_hunk"));
}
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test --test event_kind_no_diffhunk`
Expected: FAIL (현재 DiffHunk 존재).

- [ ] **Step 3: 제거**

- `src/model/observed.rs`: `EventKind` enum에서 `DiffHunk,` 줄 삭제 + `as_str()`의 `EventKind::DiffHunk => "diff_hunk",` 줄 삭제.
- `src/db/repo_observed.rs:~504`: `"diff_hunk" => EventKind::DiffHunk,` 줄 삭제 (→ `_ => Unknown` fallback).
- `src/api/sse.rs:~232`: `"diff_hunk" => EventKind::DiffHunk,` 줄 삭제. `:~245`: `derive_source_type`의 `EventKind::DiffHunk => "transcript".into(),` 줄 삭제 (기본 분기가 transcript라 무해).

- [ ] **Step 4: 빌드 + 테스트 통과**

Run: `cargo build && cargo test --test event_kind_no_diffhunk`
Expected: PASS. (diff_hunk 사이드테이블·extract_diff_hunks는 그대로 — EventKind 잔재만 제거.)

- [ ] **Step 5: Commit**

```bash
git add src/model/observed.rs src/db/repo_observed.rs src/api/sse.rs tests/event_kind_no_diffhunk.rs
git commit -m "refactor(model): drop vestigial EventKind::DiffHunk (side-table only)"
```

---

## Task 8: API /v1/signals + DTO

**Files:**
- Modify: `src/api/dto.rs`, `src/api/routes.rs`, `src/api/mod.rs`
- Test: `tests/api_signals.rs` (또는 기존 api 테스트 갱신)

- [ ] **Step 1: DTO** — `src/api/dto.rs`

`FindingDto`/`FindingsResponse`/`ToolFailureSummaryDto`/`FindingEvidence*`를 제거하고:

```rust
#[derive(Serialize)]
pub struct SignalDto {
    pub signal_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub detector: String,
    pub subkind: Option<String>,
    pub summary: String,
    pub evidence_refs: Vec<serde_json::Value>,
    pub facts: serde_json::Value,
    pub provenance: serde_json::Value,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct SignalsResponse { pub data: Vec<SignalDto> }
```

- [ ] **Step 2: 라우트** — `src/api/routes.rs`

`list_findings`/`finding_detail`/`finding_evidence`/`session_findings`/`session_tool_failures` 제거하고 `session_signals` + `signal_detail` 추가:

```rust
fn signal_row_to_dto(r: repo_signal::SignalRow) -> SignalDto {
    SignalDto {
        signal_id: r.signal_id, schema_version: r.schema_version, session_id: r.session_id,
        detector: r.detector, subkind: r.subkind, summary: r.summary,
        evidence_refs: serde_json::from_str(&r.evidence_refs).unwrap_or_default(),
        facts: serde_json::from_str(&r.facts).unwrap_or(serde_json::Value::Null),
        provenance: serde_json::from_str(&r.provenance).unwrap_or(serde_json::Value::Null),
        created_at: r.created_at,
    }
}

pub async fn session_signals(State(pool): State<SqlitePool>, Path(session_id): Path<String>) -> impl IntoResponse {
    match repo_signal::list_by_session(&pool, &session_id).await {
        Ok(rows) => Json(SignalsResponse { data: rows.into_iter().map(signal_row_to_dto).collect() }).into_response(),
        Err(err) => { tracing::error!(err=%err, "session_signals failed");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":"internal server error"}))).into_response() }
    }
}

pub async fn signal_detail(State(pool): State<SqlitePool>, Path(id): Path<String>) -> impl IntoResponse {
    match repo_signal::get(&pool, &id).await {
        Ok(Some(r)) => Json(json!({ "data": signal_row_to_dto(r) })).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({"error":"signal not found","signal_id":id}))).into_response(),
        Err(err) => { tracing::error!(err=%err, "signal_detail failed");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":"internal server error"}))).into_response() }
    }
}
```

- [ ] **Step 3: mod.rs 라우트** — `src/api/mod.rs`

finding 라우트 5줄을 제거하고:

```rust
.route("/v1/sessions/:id/signals", get(routes::session_signals))
.route("/v1/signals/:id", get(routes::signal_detail))
```

- [ ] **Step 4: 통합 테스트** — `tests/api_signals.rs`

```rust
#[tokio::test]
async fn session_signals_returns_inserted() {
    let pool = wimcc::db::test_pool().await;
    repo_signal::insert(&pool, &sample_row("sess_1")).await.unwrap();
    let app = wimcc::api::router(pool.clone());
    let resp = request(&app, "GET", "/v1/sessions/sess_1/signals").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await;
    assert_eq!(body["data"][0]["detector"], "tool_failure");
    assert!(body["data"][0].get("severity").is_none()); // 판단 필드 없음
}
```

> `wimcc::api::router`·`test_pool`·`request` 헬퍼는 기존 api 통합테스트(`tests/api_*.rs`)의 패턴을 그대로 따른다.

- [ ] **Step 5: 빌드 + 전체 테스트**

Run: `cargo build && cargo test`
Expected: PASS (finding 참조가 남아 있으면 컴파일 에러로 드러남 → 모두 signal로 정리).

- [ ] **Step 6: Commit**

```bash
git add src/api/
git add tests/api_signals.rs
git commit -m "feat(api): /v1/sessions/:id/signals + signal_detail (remove finding endpoints)"
```

---

## Task 9: 최종 검증 + 재ingest 스모크

- [ ] **Step 1: 전체 테스트 그린**

Run: `cargo test`
Expected: 전부 PASS. finding 잔여 참조 0.

- [ ] **Step 2: clippy**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: 경고 0.

- [ ] **Step 3: DB 재생성 + 재ingest (실데이터 스모크)**

Run:
```bash
cargo run -- init-db
cargo run -- ingest --all   # 또는 기존 ingest 명령
```
Expected: signal 테이블에 행 생성. `sqlite3 <db> "SELECT detector, count(*) FROM signal GROUP BY detector"`로 4개 detector 분포 확인.

- [ ] **Step 4: API 스모크**

Run: serve 후 `curl -s localhost:7878/v1/sessions/<id>/signals | jq '.data[0]'`
Expected: signal DTO (detector·facts·evidence_refs, severity/confidence 없음).

- [ ] **Step 5: Commit (문서)**

`docs/implementation-notes.html`에 finding→signal 전환 노트 추가 후:

```bash
git add docs/implementation-notes.html
git commit -m "docs: note finding→signal transition (Plan 1)"
```

---

## Self-Review 메모

- 프론트(`webui/`)는 이 plan에서 **건드리지 않는다** — `getFindings`/`FindingDto`/`findingEventIds`/`InsightTab`은 Plan 2에서 signal로 전환. 그때까지 프론트는 빌드되지만 finding API 404가 날 수 있음(디테일 뷰 인사이트 빈 상태) — Plan 2가 곧 잇는다는 전제(integration line 유지).
- `evidence_projection` → `facts` 이름 변경은 의미 동일(L1 사실 projection). 혼선 방지 위해 전부 `facts`로 통일.
- redaction_shim 사용부는 그대로 유지(facts 안의 excerpt redaction).
- AC-4(evidence_refs 비어있지 않음)는 signal에도 적용 — 각 detector가 최소 1개 ref 보장(기존 로직 유지).
