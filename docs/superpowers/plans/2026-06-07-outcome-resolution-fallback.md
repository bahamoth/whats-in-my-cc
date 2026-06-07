# Outcome Resolution with Fallback (Plan 6)

> REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use `- [ ]`.

**Goal:** 명령 결과(passed/failed) 판정을 **OTLP `success` 최우선 + 폴백 체인**으로. transcript `is_error`(도구 레벨) 의존 제거, **provenance(measured/estimated/unknown)** 노출. verification_run·tool_failure 공유.

**근본 원인:** transcript `tool_result.is_error`는 도구 실행 여부일 뿐 명령 exit≠0을 반영 안 함 → 빌드/테스트 실패도 is_error=false → passed/ok 오판.

**폴백 체인 (OTLP optional):**
1. OTLP `log_record`(event_name=tool_result, 같은 tool_use_id) `attributes.success` → measured
2. hook(post_tool_use) `tool_response.exit_code` → measured
3. transcript content의 명시적 `exit code: N` → measured
4. (verification 한정) 도구별 출력 실패 규칙 → estimated
5. 없음 → unknown (is_error는 "도구 실행"으로만)

`SessionInsightView.events`에 log_record/hook 포함 → extract 시 correlation. 라이브 시점차면 unknown, idempotent re-run에서 갱신.

---

## Task 1: resolve_outcome 헬퍼
**Files:** `src/insight/outcome.rs`(+mod.rs), `tests/outcome_resolve.rs`

- [x] **Step 1: 실패 테스트** — (a) OTLP success=false → Failed/Measured (b) is_error=false + OTLP 없음 → Unknown/Unknown (NOT passed) (c) content "exit code: 2" → Failed/Measured.
- [x] **Step 2: 실패 확인** `cargo test --test outcome_resolve`.
- [x] **Step 3: 구현** `src/insight/outcome.rs`:
```rust
//! Command outcome (Plan 6): OTLP success 최우선 + fallback. is_error(도구 레벨) 미사용.
use crate::model::observed::{EventKind, ObservedEvent};

#[derive(Debug,Clone,Copy,PartialEq,Eq,serde::Serialize)]
#[serde(rename_all="snake_case")] pub enum OutcomeStatus { Passed, Failed, Unknown }
#[derive(Debug,Clone,Copy,PartialEq,Eq,serde::Serialize)]
#[serde(rename_all="snake_case")] pub enum OutcomeProvenance { Measured, Estimated, Unknown }
#[derive(Debug,Clone,Copy)] pub struct Outcome { pub status: OutcomeStatus, pub provenance: OutcomeProvenance }

pub fn resolve_outcome(events: &[ObservedEvent], tool_use_id: &str) -> Outcome {
    // 1) OTLP log_record(tool_result).success
    for ev in events {
        if ev.kind==EventKind::LogRecord && ev.tool_use_id.as_deref()==Some(tool_use_id)
           && ev.payload.pointer("/event_name").and_then(|v|v.as_str())==Some("tool_result") {
            if let Some(s)=ev.payload.pointer("/attributes/success").and_then(|v|v.as_str()) {
                let st=if s=="true"{OutcomeStatus::Passed}else{OutcomeStatus::Failed};
                return Outcome{status:st,provenance:OutcomeProvenance::Measured};
            }
        }
    }
    // 2) hook post_tool_use exit_code
    for ev in events {
        if ev.kind==EventKind::HookEvent && ev.subkind.as_deref()==Some("post_tool_use")
           && ev.payload.pointer("/hook/hook_input/tool_use_id").and_then(|v|v.as_str())==Some(tool_use_id) {
            if let Some(code)=ev.payload.pointer("/hook/tool_response/exit_code").and_then(|v|v.as_i64()) {
                let st=if code==0{OutcomeStatus::Passed}else{OutcomeStatus::Failed};
                return Outcome{status:st,provenance:OutcomeProvenance::Measured};
            }
        }
    }
    // 3) content explicit "exit code: N"
    for ev in events {
        if ev.kind==EventKind::ToolResult && ev.tool_use_id.as_deref()==Some(tool_use_id) {
            if let Some(c)=ev.payload.pointer("/tool_result/content").and_then(|v|v.as_str()) {
                if let Some(code)=parse_exit_code(c) {
                    let st=if code==0{OutcomeStatus::Passed}else{OutcomeStatus::Failed};
                    return Outcome{status:st,provenance:OutcomeProvenance::Measured};
                }
            }
        }
    }
    Outcome{status:OutcomeStatus::Unknown,provenance:OutcomeProvenance::Unknown}
}

fn parse_exit_code(content:&str)->Option<i64>{
    let lc=content.to_ascii_lowercase();
    let i=lc.find("exit code:")?;
    content[i+"exit code:".len()..].trim_start()
        .split(|c:char|!c.is_ascii_digit()).next().filter(|s|!s.is_empty())?.parse().ok()
}
```
실제 hook payload 포인터·tool_result content 포인터는 mapping.rs/hook.rs 구조로 확인·정정.
- [x] **Step 4** 통과 + mod 등록. **Commit** `feat(insight): resolve_outcome OTLP-first with fallback`. (commit 80dcdf1)

## Task 2: verification_run → resolve_outcome
**Files:** `verification_run.rs`, `repo_verification_run.rs`(+migration status_provenance), dto, 테스트
- [x] Bash branch status(108-138행)를 `resolve_outcome`로 교체. is_error 기반 제거. `status_provenance` 추가. command_kind가 test/build/lint면 OTLP/exit 없을 때 content 실패 패턴으로 estimated 보강.
- [x] migration `verification_run.status_provenance TEXT`. repo/DTO 반영.
- [x] 테스트: 빌드실패 content(is_error=false)가 더 이상 passed 아님. `cargo test` 그린. **Commit.** (commit b6f0260)

## Task 3: tool_failure → resolve_outcome
**Files:** `tool_failure.rs`, 테스트
- [x] 발화 기준 `is_error==true` → `resolve_outcome(...).status==Failed`. facts에 `outcome_provenance`. unknown은 발화 안 함. manifest 갱신.
- [x] 테스트 갱신. `cargo test` 그린. **Commit.** (commit d48d02e)
  - 회귀 발견·수정: `insight_pipeline.rs`·`insight_provenance.rs` 통합 fixture가 is_error=true 단독으로 발화를 기대했음 → content에 "exit code: 1" 추가(Tier-3 Measured)로 새 규칙에 정합.

## Task 4: 검증
- [x] `cargo test` 0 fail (97 그룹 그린), clippy 새 경고 0 (변경 파일 기준).
- [x] DB 삭제 → `init-db` → `ingest --all` 재ingest 후 검증 완료:
  - **b4196731 tool_failure: 20+건(old, is_error 노이즈, prov NULL) → 1건(measured)**. 살아남은 1건은 `is_error=false`인 `cargo test` 실패(exit≠0)를 measured로 포착 — Plan 6 근본 동기 그대로.
  - 전역 tool_failure: measured 3건만(Unknown 미발화). verification_run: unknown 990 / failed-estimated 73 / failed-measured 1, **passed 0**(is_error=false→passed 오판 제거).
  - **트레이드오프 확인**: transcript-only 재ingest(OTLP collector 미연동)면 OTLP `success`·명시적 exit code가 드물어 대부분 unknown. measured 발화는 보수적으로 적다(false positive↓ ↔ false negative↑, self-review대로). OTLP 연동 세션 검증은 별도.

## Self-Review
- 라이브 시점차→그 시점 unknown, re-run 갱신. Tier-4는 verification만 estimated. parse_exit_code는 명시 텍스트만(구조적). hook/OTLP 포인터 실제 구조로 확인.
