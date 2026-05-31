//! `RiskyAction` L1+L2 extractor (slice-16).
//!
//! Rule (spec §3):
//! Fires when ANY of:
//! (a) A `tool_call` for `tool_name == "Bash"` whose `input.command` matches a
//!     destructive pattern allowlist.
//! (b) Any `diff_hunk` row with `user_modified == true`.
//!
//! L1 confidence: 0.7 for both branches.
//! Promotion policy: IfAbove(1.0) — always routes through L2 judge.
//! Severity: high.
//!
//! Evidence projection goes through the redaction shim (DEV-S16-05).

use regex::Regex;
use serde_json::json;

use crate::insight::extractor::InsightExtractor;
use crate::insight::redaction_shim;
use crate::insight::types::{FindingCandidate, PromotionPolicy};
use crate::insight::view::SessionInsightView;
use crate::model::observed::EventKind;

/// Destructive Bash command patterns (spec §3).
/// Locked by synthetic fixture; no real `rm -rf` observed in 9-transcript survey
/// (DEV-S16-01).
pub const DESTRUCTIVE_PATTERNS: &[&str] = &[
    r"\brm\s+-[rRfF]*[rR][fF]?\b",  // rm -rf, rm -fr, rm -r, rm -f variants
    r"\bsudo\s+rm\b",                // sudo rm (any form)
    r"\bgit\s+push\s+(--force|-f)\b", // git push --force or git push -f
    r"\bgit\s+reset\s+--hard\b",     // git reset --hard
    r"\bgit\s+clean\s+-[fdFD]+\b",   // git clean -fd, -fD, etc.
    r"\bgit\s+checkout\s+(-{1,2}\s+)?\.\b", // git checkout -- . or git checkout .
    r"\bdd\s+if=",                   // dd if=... (disk copy/wipe)
    r"\bmkfs\.",                     // mkfs.ext4, mkfs.vfat, etc.
    r"\bshred\b",                    // shred (secure delete)
];

pub struct RiskyAction;

impl InsightExtractor for RiskyAction {
    fn category(&self) -> &'static str {
        "risky_action"
    }

    fn floor(&self) -> f32 {
        0.7
    }

    fn promotion_policy(&self) -> PromotionPolicy {
        // Always queue for judge; never promote at L1 alone (spec §3).
        PromotionPolicy::IfAbove(1.0)
    }

    fn extract(&self, view: &SessionInsightView<'_>) -> Vec<FindingCandidate> {
        let patterns: Vec<Regex> = DESTRUCTIVE_PATTERNS
            .iter()
            .map(|p| Regex::new(p).expect("destructive pattern must compile"))
            .collect();

        let mut candidates: Vec<FindingCandidate> = Vec::new();
        let mut emitted_events: std::collections::HashSet<String> = std::collections::HashSet::new();

        // --- Branch (a): destructive Bash tool_call ---
        for ev in view.events {
            if ev.kind != EventKind::ToolCall {
                continue;
            }
            if ev.tool_name.as_deref() != Some("Bash") {
                continue;
            }

            let command = ev
                .payload
                .pointer("/tool_use/input/command")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let is_destructive = patterns.iter().any(|re| re.is_match(command));
            if !is_destructive {
                continue;
            }

            if emitted_events.contains(&ev.event_id) {
                continue;
            }
            emitted_events.insert(ev.event_id.clone());

            let command_redacted = redaction_shim::apply_text(command);

            // Find the preceding user message for context
            let preceding_user_excerpt = find_preceding_user_excerpt(view, &ev.event_id);
            let preceding_assistant_excerpt = find_preceding_assistant_excerpt(view, &ev.event_id);

            let projection = json!({
                "category": "risky_action",
                "session_id": view.session_id,
                "trigger": {
                    "kind": "destructive_bash",
                    "command_redacted": command_redacted,
                    "tool_use_id": ev.tool_use_id,
                    "tool_result_event_id": null,
                    "introduced_diff_hunks": []
                },
                "context": {
                    "preceding_user_message_excerpt_redacted": preceding_user_excerpt,
                    "preceding_assistant_message_excerpt_redacted": preceding_assistant_excerpt
                }
            });

            let summary = format!(
                "Destructive Bash command detected (tool_use_id={:?}): {}",
                ev.tool_use_id,
                &command[..command.len().min(80)]
            );

            candidates.push(FindingCandidate {
                category: "risky_action",
                subkind: None,
                confidence_l1: 0.7,
                severity: "high",
                summary,
                evidence_refs: vec![ev.event_id.clone()],
                evidence_projection: projection,
            });
        }

        // --- Branch (b): user_modified diff_hunk ---
        for hunk in view.diff_hunks {
            if !hunk.user_modified {
                continue;
            }

            let key = format!("hunk:{}", hunk.diff_hunk_id);
            if emitted_events.contains(&key) {
                continue;
            }
            emitted_events.insert(key);

            let file_path_redacted = redaction_shim::apply_text(&hunk.file_path);

            let projection = json!({
                "category": "risky_action",
                "session_id": view.session_id,
                "trigger": {
                    "kind": "user_modified_hunk",
                    "command_redacted": null,
                    "tool_use_id": null,
                    "tool_result_event_id": hunk.introduced_by_event_id,
                    "introduced_diff_hunks": [
                        {
                            "diff_hunk_id": hunk.diff_hunk_id,
                            "file_path_redacted": file_path_redacted,
                            "lines_removed": hunk.lines_removed
                        }
                    ]
                },
                "context": {
                    "preceding_user_message_excerpt_redacted": "",
                    "preceding_assistant_message_excerpt_redacted": ""
                }
            });

            let summary = format!(
                "User-modified diff hunk detected: {} ({} lines removed)",
                hunk.file_path, hunk.lines_removed
            );

            candidates.push(FindingCandidate {
                category: "risky_action",
                subkind: None,
                confidence_l1: 0.7,
                severity: "high",
                summary,
                evidence_refs: vec![hunk.introduced_by_event_id.clone()],
                evidence_projection: projection,
            });
        }

        candidates
    }
}

/// Extract up to 256 chars from the most recent user_message before `event_id`.
fn find_preceding_user_excerpt(view: &SessionInsightView<'_>, event_id: &str) -> String {
    use crate::model::observed::{Actor, EventKind};
    let pos = view.events.iter().position(|e| e.event_id == event_id).unwrap_or(0);
    for ev in view.events[..pos].iter().rev() {
        if ev.actor == Actor::User && ev.kind == EventKind::UserMessage {
            let text = extract_message_text(&ev.payload);
            return redaction_shim::apply_text_truncated(&text, 256);
        }
    }
    String::new()
}

/// Extract up to 256 chars from the most recent assistant_message before `event_id`.
fn find_preceding_assistant_excerpt(view: &SessionInsightView<'_>, event_id: &str) -> String {
    use crate::model::observed::{Actor, EventKind};
    let pos = view.events.iter().position(|e| e.event_id == event_id).unwrap_or(0);
    for ev in view.events[..pos].iter().rev() {
        if ev.actor == Actor::Assistant && ev.kind == EventKind::AssistantMessage {
            let text = extract_message_text(&ev.payload);
            return redaction_shim::apply_text_truncated(&text, 256);
        }
    }
    String::new()
}

/// Extract plain text from the message payload's content array.
fn extract_message_text(payload: &serde_json::Value) -> String {
    if let Some(content) = payload.pointer("/message/content") {
        if let Some(arr) = content.as_array() {
            return arr
                .iter()
                .filter_map(|item| {
                    item.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
                })
                .collect::<Vec<_>>()
                .join(" ");
        }
        if let Some(s) = content.as_str() {
            return s.to_string();
        }
    }
    String::new()
}
