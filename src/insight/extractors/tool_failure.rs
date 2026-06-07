//! `ToolFailure` detector (Plan 1: finding → signal).
//!
//! Rule (spec §5):
//! For each `tool_result` event with `is_error == true`, emit one signal.
//! Forward window (config `retry_window`, default 5): if a later `tool_result`
//! for the same `tool_use_id` has `is_error == false`, expose `retried=true`
//! as a FACT (not a suppression judgment).
//!
//! Facts only: `is_error`, `retried`, `tool_name`, `tool_use_id`,
//! `error_excerpt`, plus event ids. NO severity / failure-class / benign /
//! internal judgment — those are interpretation, left to LLM/human (spec §6.3).
//!
//! Evidence refs: [tool_result_event_id, paired_tool_call_event_id] (if call found).

use serde_json::json;

use crate::insight::config::DetectorConfig;
use crate::insight::extractor::Detector;
use crate::insight::manifest::DetectorManifest;
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
            intent: "도구 실행이 is_error=true로 끝나고 retry_window 내에 같은 tool_use_id로 성공한 결과가 없는 경우를 탐지한다.",
            // Verified against detect(): reads /tool_result/is_error (bool),
            // ev.tool_use_id (correlation), /tool_result/content (error_excerpt),
            // paired tool_call for /tool_use/... fields (tool_name).
            inputs: vec![
                "tool_result.is_error",
                "tool_result.tool_use_id",
                "tool_result.content",
                "tool_call.tool_use_id",
                "tool_call.tool_name",
            ],
            rule: "tool_result.is_error==true이고, 이후 retry_window개 이벤트 안에 동일 tool_use_id를 가진 is_error==false tool_result가 없으면 발화. 성공 retry가 존재하면 retried=true FACT로 표면화(억제하지 않음).",
            output: "{is_error, retried, tool_name, tool_use_id, error_excerpt, tool_result_event_id, paired_call_event_id}",
            // Verified: cfg.usize_param("tool_failure", "retry_window", RETRY_WINDOW_DEFAULT)
            config_keys: vec!["retry_window"],
            rationale: "tests/fixtures/transcripts/real/tool_failure_v01.jsonl + spec §6.3",
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

            // Check is_error flag (spec §5 edge case: absent = false, no fire).
            // Payload structure: {"tool_result": {"is_error": bool, ...}}
            // (verified against real fixture: aac68973-729e-4014-a02b-28a556f5ff29).
            let is_error = ev
                .payload
                .pointer("/tool_result/is_error")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !is_error {
                continue;
            }

            let tid = ev.tool_use_id.clone();
            if let Some(ref t) = tid {
                if emitted.contains(t) {
                    continue;
                }
            }

            // Forward window: same tool_use_id success → retried (a FACT, not a
            // suppression). We surface it; "is this benign?" is a judgment.
            let retried = tid
                .as_ref()
                .map(|t| {
                    let end = (i + 1 + window).min(events.len());
                    events[i + 1..end].iter().any(|e2| {
                        e2.kind == EventKind::ToolResult
                            && e2.tool_use_id.as_deref() == Some(t.as_str())
                            && !e2
                                .payload
                                .pointer("/tool_result/is_error")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false)
                    })
                })
                .unwrap_or(false);

            // Find the paired tool_call event; tool_name lives on the call event.
            let call = events[..i]
                .iter()
                .rev()
                .find(|e2| e2.kind == EventKind::ToolCall && e2.tool_use_id == tid);
            let tool_name = call
                .and_then(|e| e.tool_name.as_deref())
                .or(ev.tool_name.as_deref())
                .unwrap_or("unknown");

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

            out.push(SignalCandidate {
                detector: "tool_failure",
                subkind: None,
                summary,
                evidence_refs: refs,
                facts,
            });

            if let Some(t) = tid {
                emitted.insert(t);
            }
        }

        out
    }
}
