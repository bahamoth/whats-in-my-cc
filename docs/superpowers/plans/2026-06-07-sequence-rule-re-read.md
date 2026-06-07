# Sequence Rule: re-read (Plan 5) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`).

**Goal:** 첫 **시퀀스 룰 detector** — `re_read`: 동일 `file_path`를 `Read` 도구로 반복 읽음(컨텍스트 망각 신호). spec §6.1·§10.5(1차 대상 = re-read + error-retry). **error-retry는 Plan 1 tool_failure의 `retried` fact로 이미 커버**되므로 신규는 re-read만.

**Architecture:** `src/insight/extractors/re_read.rs` — `Detector`(id/detect/manifest). 같은 `file_path`를 `Read` tool_call로 `min_reads`회 이상 → file_path당 1 signal(evidence_refs = 그 Read event_id들). 임계값 `min_reads`는 `DetectorConfig`(기본 2, 매직넘버 금지). `all_detectors()`에 등록 → /v1/detectors·MCP·pipeline 자동 포함.

**Tech Stack:** Rust.

**Spec:** §6.1(시퀀스), §10.5(1차 re-read), §4.2 E그룹.

---

## File Structure
- Create: `src/insight/extractors/re_read.rs`
- Modify: `src/insight/extractors/mod.rs` (`pub mod re_read;`)
- Modify: `src/insight/pipeline.rs` (`all_detectors()`에 `ReRead`)
- Test: `tests/extractor_re_read.rs`; detector 카운트 테스트(4→5) 갱신

---

## Task 1: re_read detector

**Files:** `re_read.rs`, `extractors/mod.rs`, `tests/extractor_re_read.rs`

- [ ] **Step 1: 실패 테스트** `tests/extractor_re_read.rs` — helpers는 `tests/extractor_tool_failure.rs` 패턴 복사:
```rust
use wimcc::insight::config::DetectorConfig;
use wimcc::insight::extractor::Detector;
use wimcc::insight::extractors::re_read::ReRead;

#[test]
fn fires_when_same_file_read_twice() {
    let events = vec![read_call(0, "tid0", "/a.rs"), read_call(1, "tid1", "/a.rs")];
    let cands = ReRead.detect(&view_from_events(&events), &DetectorConfig::default());
    assert_eq!(cands.len(), 1);
    assert_eq!(cands[0].detector, "re_read");
    assert_eq!(cands[0].evidence_refs.len(), 2);
    assert_eq!(cands[0].facts["file_path"], serde_json::json!("/a.rs"));
    assert_eq!(cands[0].facts["read_count"], serde_json::json!(2));
}

#[test]
fn no_fire_for_distinct_files() {
    let events = vec![read_call(0, "t0", "/a.rs"), read_call(1, "t1", "/b.rs")];
    assert_eq!(ReRead.detect(&view_from_events(&events), &DetectorConfig::default()).len(), 0);
}

#[test]
fn min_reads_from_config() {
    let events = vec![read_call(0, "t0", "/a.rs"), read_call(1, "t1", "/a.rs")];
    let cfg = DetectorConfig::from_toml_str("[detector.re_read]\nmin_reads = 3\n");
    assert_eq!(ReRead.detect(&view_from_events(&events), &cfg).len(), 0);
}
```
`read_call(i, tid, path)` = ToolCall event, tool_name="Read", payload `{tool_name:"Read", input:{file_path: path}}`. 실제 Read tool_call payload의 file_path 포인터를 risky_action의 `/tool_use/input/command` 접근과 fixture로 확인해 맞출 것.

- [ ] **Step 2: 실패 확인** `cargo test --test extractor_re_read` → FAIL.

- [ ] **Step 3: 구현** `src/insight/extractors/re_read.rs`:
```rust
//! re_read sequence detector (spec §6.1/§10.5): same file_path Read'd repeatedly.
//! Deterministic; fires per path with Read count >= min_reads (config, default 2).
//! Facts only. evidence_refs = the Read event_ids for that path.
use std::collections::BTreeMap;
use serde_json::json;
use crate::insight::config::DetectorConfig;
use crate::insight::extractor::Detector;
use crate::insight::manifest::DetectorManifest;
use crate::insight::types::SignalCandidate;
use crate::insight::view::SessionInsightView;
use crate::model::observed::EventKind;

const MIN_READS_DEFAULT: usize = 2;
pub struct ReRead;

impl Detector for ReRead {
    fn id(&self) -> &'static str { "re_read" }

    fn detect(&self, view: &SessionInsightView<'_>, cfg: &DetectorConfig) -> Vec<SignalCandidate> {
        let min_reads = cfg.usize_param("re_read", "min_reads", MIN_READS_DEFAULT);
        let mut by_path: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for ev in view.events {
            if ev.kind != EventKind::ToolCall { continue; }
            if ev.tool_name.as_deref() != Some("Read") { continue; }
            let path = ev.payload.pointer("/input/file_path")
                .or_else(|| ev.payload.pointer("/tool_use/input/file_path"))
                .and_then(|v| v.as_str());
            if let Some(p) = path {
                by_path.entry(p.to_string()).or_default().push(ev.event_id.clone());
            }
        }
        let mut out = Vec::new();
        for (path, ids) in by_path {
            if ids.len() < min_reads { continue; }
            let facts = json!({ "file_path": path, "read_count": ids.len() });
            let summary = format!("File {} read {} times (re-read).", path, ids.len());
            out.push(SignalCandidate { detector: "re_read", subkind: None, summary, evidence_refs: ids, facts });
        }
        out
    }

    fn manifest(&self) -> DetectorManifest {
        DetectorManifest {
            id: "re_read",
            intent: "동일 file_path를 Read 도구로 반복 읽음 (컨텍스트 망각 신호)",
            inputs: vec!["tool_call.tool_name(Read)", "tool_call.input.file_path"],
            rule: "같은 file_path의 Read tool_call 수 >= min_reads",
            output: "{file_path, read_count}",
            config_keys: vec!["min_reads"],
            rationale: "spec §4.2 E그룹 · §6.1 시퀀스 · §10.5 1차 (실데이터로 임계값 잠금 예정)",
        }
    }
}
```
DetectorManifest 필드는 Plan 4 실제 정의와 일치시킬 것(필드명/타입 확인 — Vec<&str> 등).

- [ ] **Step 4: mod 등록 + 통과** `cargo test --test extractor_re_read` → PASS.

- [ ] **Step 5: Commit**
```bash
git add src/insight/extractors/re_read.rs src/insight/extractors/mod.rs tests/extractor_re_read.rs
git commit -m "feat(insight): re_read sequence detector"
```

---

## Task 2: pipeline 등록 + 카운트 갱신 + 검증

**Files:** `pipeline.rs`, detector 카운트 테스트

- [ ] **Step 1: pipeline** — `all_detectors()`에 `Box::new(ReRead)` 추가(5개).
- [ ] **Step 2: 카운트 갱신** — `/v1/detectors` 5개(`tests/api_detectors.rs`), MCP `list_detectors` 5 manifests(`tests/mcp_tools_call.rs`). 갱신.
- [ ] **Step 3: 전체 테스트** `cargo build && cargo test` → 0 fail.
- [ ] **Step 4: clippy** `cargo clippy --all-targets` → 새 경고 0.
- [ ] **Step 5: Commit**
```bash
git add src/insight/pipeline.rs tests/api_detectors.rs tests/mcp_tools_call.rs
git commit -m "feat(insight): register re_read detector (5 detectors)"
```

---

## Self-Review 메모
- error-retry는 신규 불필요 — tool_failure `retried` fact가 이미 표현.
- `min_reads` 매직넘버 금지(config), 기본 2 보수적, 실데이터 후 조정.
- "동일"=file_path 완전 일치(정규화 없음, 결정적). 상대/절대 경로 혼재는 별개 카운트(알려진 한계).
- 세션 전체 카운트(윈도우 무제한) — window 파라미터는 후속.
