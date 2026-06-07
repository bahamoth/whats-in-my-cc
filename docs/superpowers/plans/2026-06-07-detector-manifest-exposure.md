# Detector Manifest Exposure (Plan 4) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`).

**Goal:** 각 detector에 **manifest**(intent·inputs·rule·output·rationale)를 부여하고, **read-only API/MCP로 노출**해 LLM이 detector 구성·근거를 조회하고 개선할 수 있게 한다(spec §6.4·6.5). predicate는 코드, config(파라미터)는 Plan 1의 `DetectorConfig`, **manifest는 LLM이 읽는 선언**.

**Architecture:** `Detector` trait에 `manifest() -> DetectorManifest` 추가. 4개 detector(tool_failure·risky_action·context_bloat·final_state_mismatch)가 자기기술 manifest 반환. `GET /v1/detectors` 카탈로그 API + MCP tool `list_detectors`. LLM 개선 루프(spec §6.5)는 "manifest+config+최근 signal 조회 → 제안 → TDD 잠금 → 재실행" 워크플로우로, CLAUDE.md tagging-loop 확장 문서로 기록(코드 실체는 노출 API/MCP).

**Tech Stack:** Rust, axum, MCP (Streamable HTTP).

**Spec:** §6.4(detector 3분해: manifest/config/predicate)·§6.5(LLM 개선 루프)·§6.6(신호분포는 Plan 3a metrics). 선례: `eventTags` untagged-bash 루프, redaction rule_pack.

---

## File Structure
- Create: `src/insight/manifest.rs` — `DetectorManifest` struct
- Modify: `src/insight/extractor.rs` — `Detector` trait에 `manifest()`
- Modify: 4 detectors — `manifest()` 구현
- Modify: `src/api/dto.rs`, `src/api/routes.rs`, `src/api/mod.rs` — `GET /v1/detectors`
- Modify: `src/api/mcp/` — `list_detectors` tool + registry/golden fixtures
- Modify: `docs/implementation-notes.html` (or CLAUDE.md) — detector 개선 루프 문서
- Test: `tests/detector_manifest.rs`, `tests/api_detectors.rs`, MCP 테스트 갱신

---

## Task 1: DetectorManifest + Detector::manifest() + 4 구현

**Files:** `src/insight/manifest.rs`, `extractor.rs`, 4 detectors, `tests/detector_manifest.rs`

- [ ] **Step 1: 실패 테스트** `tests/detector_manifest.rs`
```rust
use wimcc::insight::extractor::Detector;
use wimcc::insight::extractors::tool_failure::ToolFailure;

#[test]
fn tool_failure_manifest_is_self_describing() {
    let m = ToolFailure.manifest();
    assert_eq!(m.id, "tool_failure");
    assert!(!m.intent.is_empty());
    assert!(m.inputs.iter().any(|i| i.contains("is_error")));
    assert!(!m.rationale.is_empty()); // docs/fixture anchor
}
```

- [ ] **Step 2: 구현** `src/insight/manifest.rs`
```rust
//! Detector manifest — the LLM-readable declaration of a detector (spec §6.4).
//! predicate=code, config=rule pack, manifest=this. Read-only; exposed via API/MCP.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DetectorManifest {
    pub id: &'static str,
    pub intent: &'static str,          // 무엇을 탐지하나 (사람·LLM이 읽음)
    pub inputs: Vec<&'static str>,     // raw 필드 의존성 (e.g. "tool_result.is_error")
    pub rule: &'static str,            // 의사코드/설명
    pub output: &'static str,          // signal facts 형태
    pub config_keys: Vec<&'static str>,// DetectorConfig에서 읽는 파라미터 키
    pub rationale: &'static str,       // 근거 앵커 (docs#... / fixture path)
}
```

- [ ] **Step 3: trait** `extractor.rs` — `Detector`에 `fn manifest(&self) -> crate::insight::manifest::DetectorManifest;` 추가.

- [ ] **Step 4: 4 detector 구현** — 각 `manifest()`:
  - **tool_failure**: id="tool_failure", intent="도구 실행이 is_error로 끝나고 재시도로 복구되지 않음", inputs=["tool_result.is_error","tool_result.tool_use_id"], rule="is_error==true tool_result 후 retry_window 내 동일 tool_use_id 성공 없음", output="{is_error,retried,tool_name,error_excerpt}", config_keys=["retry_window"], rationale="tests/fixtures/transcripts/real/tool_failure_v01.jsonl + spec §6.3".
  - **risky_action**: inputs=["tool_use.input.command","diff_hunk.user_modified"], rule="destructive Bash 패턴 OR user_modified hunk", config_keys=[], rationale="DESTRUCTIVE_PATTERNS + spec §4.2 C".
  - **context_bloat**: inputs=["tool_result.content size"], rule="큰 tool_result가 다음 턴에 미사용", config_keys=["threshold_bytes","next_event_window","min_overlap_stems"], rationale="spec §4.2 B".
  - **final_state_mismatch**: inputs=["user_message goal verbs","verification_run.status"], rule="목표 동사 + 미완료 마커 + 마지막 검증 실패", config_keys=[], rationale="spec §4.2 D".
  (실제 inputs/rule은 각 detector 코드의 실제 동작과 일치시킬 것 — 추정 금지, 코드 확인.)

- [ ] **Step 5: 테스트 통과** `cargo test --test detector_manifest`. (mod 등록.)

- [ ] **Step 6: Commit**
```bash
git add src/insight/manifest.rs src/insight/extractor.rs src/insight/extractors/ src/insight/mod.rs tests/detector_manifest.rs
git commit -m "feat(insight): detector manifest (LLM-readable self-description)"
```

---

## Task 2: GET /v1/detectors

**Files:** `dto.rs`, `routes.rs`, `mod.rs`, `tests/api_detectors.rs`

- [ ] **Step 1: 핸들러** — `all_detectors()`(pipeline.rs)를 순회해 각 `manifest()`를 모은 카탈로그. `DetectorManifest`는 이미 Serialize.
```rust
pub async fn list_detectors() -> impl IntoResponse {
    let cat: Vec<_> = crate::insight::pipeline::all_detectors().iter().map(|d| d.manifest()).collect();
    Json(json!({ "data": cat }))
}
```
(`all_detectors`가 private이면 pub(crate) 또는 manifest 카탈로그 fn을 pipeline에 추가.)

- [ ] **Step 2: route** `mod.rs` — `.route("/v1/detectors", get(routes::list_detectors))` (authed).

- [ ] **Step 3: 테스트** `tests/api_detectors.rs` — `GET /v1/detectors` → 200, `data`에 4개, 각 id/intent/inputs/rationale 존재.

- [ ] **Step 4: build + test** `cargo build && cargo test --test api_detectors --test detector_manifest` → PASS.

- [ ] **Step 5: Commit**
```bash
git add src/api/ tests/api_detectors.rs
git commit -m "feat(api): GET /v1/detectors (manifest catalog)"
```

---

## Task 3: MCP list_detectors tool

**Files:** `src/api/mcp/` (tools + registry + golden fixtures), MCP 테스트

- [ ] **Step 1: 기존 MCP 구조 파악** — `src/api/mcp/tools/`의 tool 패턴(Plan 1 후 3 tools: search_sessions·get_file_lineage·get_otel_trace) + registry(`tools/mod.rs`) + golden fixtures(`tools_list_expected.json`, `protocol_compat.json`)를 읽는다.

- [ ] **Step 2: list_detectors tool 추가** — 기존 tool 패턴대로 `list_detectors`(인자 없음, manifest 카탈로그 반환). registry(tool 목록 3→4) + dispatch에 등록. golden fixtures 갱신(4 tools).

- [ ] **Step 3: MCP 테스트** — `mcp_tools_list`(3→4), `mcp_tools_call`(list_detectors 호출 → 4 manifest), `mcp_spec_compat`(fixture). 갱신/추가.

- [ ] **Step 4: build + test** `cargo build && cargo test --test mcp_tools_list --test mcp_tools_call --test mcp_spec_compat` → PASS.

- [ ] **Step 5: Commit**
```bash
git add src/api/mcp/ tests/mcp_*.rs
git commit -m "feat(mcp): list_detectors tool (manifest catalog)"
```

---

## Task 4: LLM 개선 루프 문서

**Files:** `docs/implementation-notes.html` (+ CLAUDE.md 가능)

- [ ] **Step 1: 문서** — `#detector-improvement-loop` 섹션: detector = manifest(선언)+config(rule pack)+predicate(코드). LLM이 ① `GET /v1/detectors`(or MCP `list_detectors`) + `detectors.toml`(config) + 최근 `/signals`·`/metrics`(신호분포) 조회 → ② config 조정 또는 새 detector 제안 → ③ fixture 실패 테스트로 잠금(TDD) → ④ 재ingest로 신호분포 변화 확인. eventTags untagged-bash 루프의 일반화.

- [ ] **Step 2: Commit**
```bash
git add docs/implementation-notes.html
git commit -m "docs: detector improvement loop (manifest+config+predicate, §6.5)"
```

---

## Task 5: 검증
- [ ] **Step 1**: `cargo test` → 0 fail. `cargo clippy --all-targets` 새 경고 0.
- [ ] **Step 2 (controller)**: serve(별도 포트) 후 `curl /v1/detectors | jq` → 4 manifest 합리적. MCP는 통합 테스트로 게이트.

---

## Self-Review 메모
- manifest의 inputs/rule은 **각 detector의 실제 동작과 일치**시킬 것(코드 확인, 추정 금지). manifest가 거짓이면 LLM이 잘못 개선한다(spec §6.4: 테스트가 manifest↔predicate 일치를 잠금 — 가능하면 inputs에 명시된 필드를 detector가 실제 읽는지 assert).
- LLM 루프의 "config 조정"은 Plan 1 `DetectorConfig`(TOML, 코드 fallback)를 통해. config 파일 로드 경로가 아직 `DetectorConfig::default()`만이면(Plan 1 pipeline) 파일 로드는 후속 — manifest의 config_keys는 그 키를 선언만.
- signal/metrics MCP 노출은 이 plan 범위(detector manifest)와 별개 — 필요 시 후속.
