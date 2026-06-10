//! Command outcome resolution (Plan 6): OTLP-first + fallback chain.
//!
//! `is_error` is unreliable for command pass/fail and is NOT used for outcome:
//! for a piped command like `cargo test 2>&1 | tail` it is false even when the
//! tests fail, because the shell-reported exit is the pipeline's last stage
//! (`tail`, which succeeds). A *non-piped* failing command instead surfaces via
//! Claude Code's "Exit code N" content prepend, parsed in Step 3.
//!
//! Fallback chain (first match wins):
//! 1. OTLP `log_record` (event_name=tool_result, same tool_use_id) → `attributes.success`
//!    — measured.
//! 2. Hook `post_tool_use` (same tool_use_id) → `tool_response.exit_code`
//!    — measured.
//! 3. Transcript `tool_result` content — a line-start "Exit code N" (Claude
//!    Code's prepend on non-zero exit) or "exit code: N" — measured.
//! 3b. Transcript `tool_result` content starting with `<tool_use_error>` —
//!     하니스의 기계 생성 에러 래퍼 → Failed, measured. exit code가 없는
//!     비-Bash 도구(Edit/Write)의 유일한 transcript 실패 신호.
//!     예외: `<tool_use_error>Cancelled:`(병렬 호출 취소)는 Unknown 유지.
//! 4. Nothing matched → Unknown (is_error is NOT used for outcome).

use crate::model::observed::{EventKind, ObservedEvent};

/// Resolved command outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeStatus {
    Passed,
    Failed,
    Unknown,
}

/// How the outcome was determined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeProvenance {
    /// Derived from a machine-generated signal that directly reflects the
    /// tool execution's result: OTLP success attribute, hook exit_code, a
    /// line-start "Exit code N", or the harness `<tool_use_error>` wrapper
    /// (Step 3b — 실패 사실은 측정이지만 exit code 값은 없을 수 있음).
    Measured,
    /// Derived from a tool-specific output pattern heuristic (e.g. "FAILED").
    Estimated,
    /// No measured signal available; outcome is genuinely unknown.
    Unknown,
}

/// Resolved outcome pair.
#[derive(Debug, Clone, Copy)]
pub struct Outcome {
    pub status: OutcomeStatus,
    pub provenance: OutcomeProvenance,
}

impl Outcome {
    pub const UNKNOWN: Outcome = Outcome {
        status: OutcomeStatus::Unknown,
        provenance: OutcomeProvenance::Unknown,
    };
}

/// Resolve the command outcome for a given `tool_use_id` from the event slice.
///
/// Events must belong to the same session. Order is not assumed (every step
/// of the chain scans the full slice).
///
/// The `is_error` field of transcript `tool_result` events is intentionally
/// **not** used to determine pass/fail — it only indicates whether the tool
/// executor accepted the invocation.
pub fn resolve_outcome(events: &[ObservedEvent], tool_use_id: &str) -> Outcome {
    // ── Step 1: OTLP log_record (event_name=tool_result) ─────────────────────
    // Payload shape: LogFacet serialised to JSON.
    //   { "event_name": "tool_result",
    //     "attributes": { "tool_use_id": "...", "success": "true"|"false", ... },
    //     ... }
    // `event_name` is promoted out of OTLP attributes by the log ingest layer
    // (otel_logs.rs:72-75) into a top-level LogFacet field.
    // `attributes` is the flattened OTLP attribute map.
    for ev in events {
        if ev.kind != EventKind::LogRecord {
            continue;
        }
        if ev.tool_use_id.as_deref() != Some(tool_use_id) {
            continue;
        }
        let is_tool_result_log = ev
            .payload
            .pointer("/event_name")
            .and_then(|v| v.as_str())
            == Some("tool_result");
        if !is_tool_result_log {
            continue;
        }
        if let Some(success_str) = ev
            .payload
            .pointer("/attributes/success")
            .and_then(|v| v.as_str())
        {
            let status = if success_str == "true" {
                OutcomeStatus::Passed
            } else {
                OutcomeStatus::Failed
            };
            return Outcome {
                status,
                provenance: OutcomeProvenance::Measured,
            };
        }
    }

    // ── Step 2: Hook post_tool_use exit_code ──────────────────────────────────
    // Payload shape: {"hook": <original Claude Code hook JSON>}
    // The hook JSON has `tool_use_id` at the top level and `tool_response` with
    // `exit_code`. (Verified against tests/fixtures/hook/post_tool_use.json.)
    for ev in events {
        if ev.kind != EventKind::HookEvent {
            continue;
        }
        if ev.subkind.as_deref() != Some("post_tool_use") {
            continue;
        }
        // The tool_use_id is indexed in ObservedEvent.tool_use_id by hook ingest.
        if ev.tool_use_id.as_deref() != Some(tool_use_id) {
            continue;
        }
        if let Some(code) = ev
            .payload
            .pointer("/hook/tool_response/exit_code")
            .and_then(|v| v.as_i64())
        {
            let status = if code == 0 {
                OutcomeStatus::Passed
            } else {
                OutcomeStatus::Failed
            };
            return Outcome {
                status,
                provenance: OutcomeProvenance::Measured,
            };
        }
    }

    // ── Step 3: Transcript tool_result content exit-code line ─────────────────
    // Payload shape: {"tool_result": {"content": "...", ...}}
    // Matches a line-start "Exit code N" (Claude Code prepends this on a non-zero
    // Bash exit) or "exit code: N" (structural, not heuristic — does not match
    // "non-zero exit" or similar prose).
    for ev in events {
        if ev.kind != EventKind::ToolResult {
            continue;
        }
        if ev.tool_use_id.as_deref() != Some(tool_use_id) {
            continue;
        }
        if let Some(content) = ev
            .payload
            .pointer("/tool_result/content")
            .and_then(|v| v.as_str())
        {
            if let Some(code) = parse_exit_code(content) {
                let status = if code == 0 {
                    OutcomeStatus::Passed
                } else {
                    OutcomeStatus::Failed
                };
                return Outcome {
                    status,
                    provenance: OutcomeProvenance::Measured,
                };
            }
            // ── Step 3b: harness 구조화 에러 래퍼 ─────────────────────────────
            // "<tool_use_error>…</tool_use_error>"는 Claude Code 하니스가 도구
            // 실행 실패 시 기계 생성하는 래퍼(프로즈 파싱 아님) → Failed, Measured.
            // Bash 외 도구(Edit/Write 등)는 exit code가 없어 이 채널이 유일한
            // transcript 실패 신호다 (코퍼스 ~786건 = 실패 4종 762 + 취소 ~24; real fixture:
            // disposition_v01.jsonl, invariant: tests/transcript_disposition.rs).
            // 예외: "Cancelled:"(병렬 호출 취소)는 실행 실패가 아니라
            // disposition(cancelled) → Unknown 유지.
            if let Some(inner) = content.strip_prefix("<tool_use_error>") {
                if !inner.starts_with("Cancelled:") {
                    return Outcome {
                        status: OutcomeStatus::Failed,
                        provenance: OutcomeProvenance::Measured,
                    };
                }
            }
        }
    }

    // ── No signal found ───────────────────────────────────────────────────────
    Outcome::UNKNOWN
}

/// Parse a command exit code from tool output content.
///
/// Recognises the line-start form `exit code[:] <N>` case-insensitively — both
/// Claude Code's `Exit code <N>` prepend (capital E, **no colon**) on a non-zero
/// Bash exit and an explicit `exit code: <N>` line. The CC prepend form was
/// confirmed across 215 local sessions / 82 occurrences (CC 2.1.153–2.1.168);
/// the prior colon-only matcher silently dropped every one of them.
///
/// The match is anchored at a **line start** (after optional leading whitespace)
/// so prose mentioning the phrase mid-line is not misread as the command's own
/// outcome. Still a structural parse, not a heuristic — it does NOT match prose
/// like "returned non-zero exit status" or "exit status: 1".
pub fn parse_exit_code(content: &str) -> Option<i64> {
    const PREFIX: &str = "exit code";
    for line in content.lines() {
        let trimmed = line.trim_start();
        // `get(..)` returns None when PREFIX.len() is not a char boundary or the
        // line is too short — both mean "no match here", so just keep scanning.
        let Some(head) = trimmed.get(..PREFIX.len()) else {
            continue;
        };
        if !head.eq_ignore_ascii_case(PREFIX) {
            continue;
        }
        // After "exit code": skip an optional colon and surrounding spaces, then
        // take leading ASCII digits. "exit code:" with no number falls through.
        let rest = trimmed[PREFIX.len()..].trim_start_matches([':', ' ', '\t']);
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() {
            return digits.parse().ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_exit_code_basic() {
        assert_eq!(parse_exit_code("exit code: 2"), Some(2));
        assert_eq!(parse_exit_code("Exit Code: 0"), Some(0));
        assert_eq!(parse_exit_code("EXIT CODE:127"), Some(127));
        assert_eq!(parse_exit_code("no exit here"), None);
        assert_eq!(parse_exit_code("exit code: abc"), None);
        assert_eq!(parse_exit_code("exit code:"), None);
        // Does not match prose
        assert_eq!(parse_exit_code("returned non-zero exit status 2"), None);
    }

    #[test]
    fn parse_exit_code_in_multiline() {
        let s = "FAILED\n\nexit code: 1\nsome more output";
        assert_eq!(parse_exit_code(s), Some(1));
    }

    #[test]
    fn parse_exit_code_with_leading_spaces() {
        assert_eq!(parse_exit_code("exit code:  42"), Some(42));
    }

    #[test]
    fn parse_exit_code_claude_code_prepend_format() {
        // Claude Code prepends "Exit code <N>\n" (capital E, NO colon) to a Bash
        // tool_result's content when the command exits non-zero. Confirmed across
        // 215 local sessions / 82 occurrences spanning CC 2.1.153–2.1.168 — exit
        // values seen: 1, 2, 5, 7, 101, 127, 128, 143. The colon-only parser
        // silently dropped every one of these (measured failures lost). The parser
        // MUST recognise the real CC format.
        assert_eq!(parse_exit_code("Exit code 101\nthread 'main' panicked"), Some(101));
        assert_eq!(parse_exit_code("Exit code 7\nintentional non-zero exit"), Some(7));
        assert_eq!(parse_exit_code("Exit code 1"), Some(1));
    }

    fn tool_result_ev(tid: &str, content: &str) -> ObservedEvent {
        ObservedEvent {
            event_id: format!("ev_{tid}"),
            raw_event_id: format!("raw_{tid}"),
            schema_version: "observed_event.v1".into(),
            session_id: "sess_outcome".into(),
            kind: EventKind::ToolResult,
            tool_use_id: Some(tid.into()),
            parser_version: "test".into(),
            payload: serde_json::json!({
                "tool_result": {"tool_use_id": tid, "is_error": true, "content": content}
            }),
            ..Default::default()
        }
    }

    #[test]
    fn tool_use_error_resolves_failed_measured() {
        // <tool_use_error>는 하니스가 기계 생성하는 구조화 에러 채널(프로즈 아님) —
        // 실 payload: disposition_v01.jsonl session 5864d6c7 (stale-read Edit 실패).
        // 코퍼스 ~786건(실패 4종 762 + 취소 ~24)이 이 래퍼를 갖지만 기존 체인은 전부 Unknown으로 흘려보냈다.
        let evs = vec![tool_result_ev(
            "toolu_stale",
            "<tool_use_error>File has been modified since read, either by the user or by a linter. Read it again before attempting to write it.</tool_use_error>",
        )];
        let o = resolve_outcome(&evs, "toolu_stale");
        assert_eq!(o.status, OutcomeStatus::Failed);
        assert_eq!(o.provenance, OutcomeProvenance::Measured);
    }

    #[test]
    fn tool_use_error_cancelled_stays_unknown() {
        // 병렬 호출 취소는 실행 실패가 아니다 — disposition(cancelled)의 영역.
        // 실 payload: disposition_v01.jsonl session ed82aee9.
        let evs = vec![tool_result_ev(
            "toolu_cxl",
            "<tool_use_error>Cancelled: parallel tool call Bash(...)</tool_use_error>",
        )];
        let o = resolve_outcome(&evs, "toolu_cxl");
        assert_eq!(o.status, OutcomeStatus::Unknown);
    }

    #[test]
    fn parse_exit_code_anchors_at_line_start() {
        // Only the structural CC prepend (line start) or an explicit "exit code:"
        // line counts. Prose mentioning "exit code" mid-line must NOT match, so a
        // command whose OUTPUT happens to contain the phrase isn't misread as the
        // command's own outcome (4 such mid-line cases observed in real data).
        assert_eq!(parse_exit_code("see the returned exit code 5 in the log"), None);
        assert_eq!(parse_exit_code("the script printed exit code: 9 to stdout"), None);
    }
}
