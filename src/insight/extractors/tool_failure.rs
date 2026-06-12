//! `ToolFailure` detector (Plan 6 update: outcome-first).
//!
//! Rule (Plan 6):
//! For each `tool_result` event, resolve the command outcome via
//! `resolve_outcome` (OTLP-first chain). Emit a signal when
//! `resolve_outcome(...).status == Failed`. Unknown outcome does NOT fire.
//!
//! The old is_error-based trigger is replaced: `is_error` is kept in facts as a
//! **tool-execution indicator only** (did the tool executor accept the call?),
//! not as the pass/fail signal for the command.
//!
//! `outcome_provenance` ("measured" | "estimated") is added to facts so callers
//! can distinguish OTLP-confirmed failures from Tier-4 content estimates.
//!
//! Forward window (config `retry_window`, default 5): if a DISTINCT later
//! `tool_call` (a new `tool_use_id`) re-runs the same operation — same
//! `tool_name` AND same `/input` — and that re-run resolves to Passed, expose
//! `retried=true` as a FACT (not a suppression judgment). Re-using the same
//! `tool_use_id` is the SAME invocation, never a retry — and since we only fire
//! when an id resolves Failed, a same-id check could never resolve Passed.
//!
//! Facts: `is_error` (tool-execution), `outcome_provenance`, `retried`,
//! `tool_name`, `tool_use_id`, `error_excerpt`, event ids.
//!
//! Evidence refs: [tool_result_event_id, paired_tool_call_event_id] (if call found).

use serde_json::json;

use crate::insight::config::DetectorConfig;
use crate::insight::extractor::Detector;
use crate::insight::manifest::DetectorManifest;
use crate::insight::outcome::{resolve_outcome, OutcomeStatus};
use crate::insight::types::SignalCandidate;
use crate::insight::view::SessionInsightView;
use crate::model::observed::EventKind;

/// Default forward window (events) to check for a compensating successful retry.
const RETRY_WINDOW_DEFAULT: usize = 5;

pub struct ToolFailure;

impl Detector for ToolFailure {
    fn id(&self) -> &'static str {
        "tool_failure"
    }

    fn manifest(&self) -> DetectorManifest {
        DetectorManifest {
            id: "tool_failure",
            intent: "도구 실행의 명령 결과가 resolve_outcome 체인(OTLP→hook→transcript, 측정 우선)에서 Failed이고 retry_window 내에 같은 tool_use_id로 Passed가 없는 경우를 탐지한다. is_error는 도구 실행 여부만 나타내며 pass/fail 판정에 미사용.",
            // Verified against detect(): reads ev.tool_use_id (correlation),
            // resolve_outcome chain (OTLP/hook/exit-code), /tool_result/content (error_excerpt),
            // paired tool_call for tool_name. is_error kept as FACT, not trigger.
            inputs: vec![
                "otlp_log_record.attributes.success",
                "hook_event.tool_response.exit_code",
                "tool_result.content (exit code: N)",
                "tool_result.tool_use_id",
                "tool_result.is_error (tool-execution fact only)",
                "tool_call.tool_use_id",
                "tool_call.tool_name",
            ],
            rule: "resolve_outcome(events, tool_use_id).status==Failed이면 발화(Unknown은 미발화). retry_window 이내에 동일 (tool_name, input)을 가진 '다른' tool_use_id의 후속 tool_call이 Passed로 resolve되면 retried=true FACT로 표면화(같은 tool_use_id는 같은 호출이라 retry 아님; 억제 없음).",
            output: "{is_error, outcome_provenance, retried, tool_name, tool_use_id, error_excerpt, tool_result_event_id, paired_call_event_id}",
            // Verified: cfg.usize_param("tool_failure", "retry_window", RETRY_WINDOW_DEFAULT)
            config_keys: vec!["retry_window"],
            rationale: "tests/fixtures/transcripts/real/tool_failure_v01.jsonl + outcome resolution 체인(src/insight/outcome.rs) + spec §6.3",
            metric_class: "process",
        }
    }

    fn detect(&self, view: &SessionInsightView<'_>, cfg: &DetectorConfig) -> Vec<SignalCandidate> {
        let window = cfg.usize_param("tool_failure", "retry_window", RETRY_WINDOW_DEFAULT);
        let events = view.events;
        let mut out = Vec::new();
        // Track which tool_use_ids we've already emitted a signal for to avoid
        // duplicates when there are multiple error results for the same id.
        let mut emitted: std::collections::HashSet<String> = std::collections::HashSet::new();

        for (i, ev) in events.iter().enumerate() {
            if ev.kind != EventKind::ToolResult {
                continue;
            }

            let tid = ev.tool_use_id.clone();

            if let Some(ref t) = tid {
                if emitted.contains(t) {
                    continue;
                }
            }

            // Plan 6: resolve_outcome (OTLP-first chain).
            // is_error is NOT the trigger — it is only a tool-execution indicator.
            // Unknown outcome → do NOT fire (no confirmed failure signal).
            let tid_str = tid.as_deref().unwrap_or("");
            let outcome = resolve_outcome(events, tid_str);
            if outcome.status != OutcomeStatus::Failed {
                continue;
            }

            let outcome_provenance = match outcome.provenance {
                crate::insight::outcome::OutcomeProvenance::Measured => "measured",
                crate::insight::outcome::OutcomeProvenance::Estimated => "estimated",
                crate::insight::outcome::OutcomeProvenance::Unknown => "unknown",
            };

            // is_error: keep as a FACT labeled as tool-execution indicator only.
            // (Payload structure: {"tool_result": {"is_error": bool, ...}}
            // verified against real fixture: aac68973-729e-4014-a02b-28a556f5ff29.)
            let is_error = ev
                .payload
                .pointer("/tool_result/is_error")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            // Find the paired tool_call event; tool_name + input live on the call.
            let call = events[..i]
                .iter()
                .rev()
                .find(|e2| e2.kind == EventKind::ToolCall && e2.tool_use_id == tid);
            let tool_name = call
                .and_then(|e| e.tool_name.as_deref())
                .or(ev.tool_name.as_deref())
                .unwrap_or("unknown");

            // Forward window: did the agent RE-RUN this operation and succeed?
            // A retry is a DISTINCT later tool_call (new tool_use_id) with the same
            // (tool_name, /input) whose own outcome resolves to Passed. Re-using the
            // SAME tool_use_id is the same invocation — never a retry — and since we
            // only reach here when this id resolves Failed, a same-id check could
            // never resolve Passed. retried is a FACT (no suppression).
            let failing_input = call.and_then(|c| c.payload.pointer("/input"));
            let retried = match (call.and_then(|c| c.tool_name.as_deref()), tid.as_deref()) {
                (Some(failing_tool), Some(failing_tid)) => {
                    let end = (i + 1 + window).min(events.len());
                    events[i + 1..end].iter().any(|c2| {
                        if c2.kind != EventKind::ToolCall {
                            return false;
                        }
                        let Some(c2_tid) = c2.tool_use_id.as_deref() else {
                            return false;
                        };
                        if c2_tid == failing_tid {
                            return false; // same invocation, not a retry
                        }
                        if c2.tool_name.as_deref() != Some(failing_tool) {
                            return false;
                        }
                        if c2.payload.pointer("/input") != failing_input {
                            return false;
                        }
                        resolve_outcome(events, c2_tid).status == OutcomeStatus::Passed
                    })
                }
                _ => false,
            };

            let error_excerpt: String = ev
                .payload
                .pointer("/tool_result/content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .chars()
                .take(512)
                .collect();

            let mut refs = vec![ev.event_id.clone()];
            if let Some(c) = call {
                refs.push(c.event_id.clone());
            }

            // FACTS ONLY — no exit/benign/internal judgment. Raw signals exposed.
            // is_error is preserved as a tool-execution fact (not the trigger).
            // outcome_provenance indicates how the failure was determined.
            let facts = json!({
                "is_error": is_error,
                "outcome_provenance": outcome_provenance,
                "retried": retried,
                "tool_name": tool_name,
                "tool_use_id": tid,
                "error_excerpt": error_excerpt,
                "tool_result_event_id": ev.event_id,
                "paired_call_event_id": call.map(|c| c.event_id.clone()),
            });
            let summary = format!(
                "Tool {tool_name} command failed (outcome_provenance={outcome_provenance}, retried={retried})."
            );

            out.push(SignalCandidate {
                detector: "tool_failure",
                subkind: None,
                summary,
                evidence_refs: refs,
                facts,
                dedup_key: None,
            });

            if let Some(t) = tid {
                emitted.insert(t);
            }
        }

        out
    }
}
