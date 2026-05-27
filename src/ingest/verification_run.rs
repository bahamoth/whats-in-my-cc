//! VerificationRun extractor (slice-11).
//!
//! Walks `ObservedEvent` slices and emits `VerificationRunRecord` rows for
//! events that represent verification activity:
//!   - **Bash branch**: `ToolCall` with `tool_name == "Bash"` and a command
//!     on the `verification_allowlist`, plus its paired `ToolResult`.
//!     Status is derived from `tool_result["is_error"]`.
//!   - **Hook branch**: `HookEvent` with `subkind == "PostToolUse"` and
//!     `hook_input.tool_name == "Bash"` + matched command. Deduped by
//!     `trigger_event_id` — if a Bash row already exists for the same
//!     `trigger_event_id`, the hook row is dropped.
//!   - **OTel branch**: `OtelSpan` with `attributes["verification.kind"]` —
//!     see DEV-S11-01; emits no rows in production until a real fixture lands.
//!
//! All output IDs are deterministic: `verification_run_id` is derived from
//! `sha256(session_id || trigger_event_id || started_at)` with a `vr_` prefix.

use sha2::{Digest, Sha256};

use crate::insight::verification_allowlist::classify;
use crate::model::observed::{EventKind, ObservedEvent};

pub const PARSER_VERSION: &str = "verification_run@v1";
pub const SCHEMA_VERSION: &str = "verification_run.v1";

/// A single verification run row ready for DB insertion.
#[derive(Debug, Clone)]
pub struct VerificationRunRecord {
    pub verification_run_id: String,
    pub schema_version: &'static str,
    pub session_id: String,
    pub source: String,          // "bash" | "hook" | "otel"
    pub command: String,
    pub command_kind: String,
    pub trigger_event_id: String,
    pub trigger_tool_use_id: Option<String>,
    pub status: String,          // "passed" | "failed" | "unknown"
    pub started_at: String,      // ISO 8601 UTC
    pub ended_at: Option<String>,
    pub exit_code: Option<i32>,
    pub failure_summary: Option<String>,
    pub raw_event_id: String,
    pub parser_version: &'static str,
}

/// Extract verification run records from an ordered slice of observed events.
/// Events must belong to the same session.
///
/// Behaviour:
/// - Walks `ToolCall` events with `tool_name == "Bash"`. If the command
///   (at `payload["input"]["command"]`) matches the allowlist, looks for the
///   paired `ToolResult` (matched by `tool_use_id`).
/// - Derives status from `tool_result["is_error"]`: `false` → "passed",
///   `true` → "failed". If no result found → "unknown".
/// - IDs are deterministic — calling this function twice over the same events
///   produces identical rows.
pub fn extract_verification_runs(evs: &[ObservedEvent]) -> Vec<VerificationRunRecord> {
    // Build tool_use_id → tool_result event index for O(1) lookup.
    let mut result_by_tid: std::collections::HashMap<&str, &ObservedEvent> =
        std::collections::HashMap::new();
    for ev in evs {
        if ev.kind == EventKind::ToolResult {
            if let Some(tid) = ev.tool_use_id.as_deref() {
                result_by_tid.entry(tid).or_insert(ev);
            }
        }
    }

    // Track produced trigger_event_ids to deduplicate hook vs bash rows.
    let mut seen_trigger_ids: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    let mut out: Vec<VerificationRunRecord> = Vec::new();

    // --- Bash branch ---
    for ev in evs {
        if ev.kind != EventKind::ToolCall {
            continue;
        }
        if ev.tool_name.as_deref() != Some("Bash") {
            continue;
        }

        let cmd = ev
            .payload
            .pointer("/input/command")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Normalise: strip trailing shell pipe / redirect sections so that
        // `cargo test 2>&1 | tail -5` is classified as `cargo test`.
        let effective_cmd = normalise_command(cmd);
        let Some(command_kind) = classify(effective_cmd) else {
            continue;
        };

        let Some(tid) = ev.tool_use_id.as_deref() else {
            continue;
        };

        // Find paired tool_result
        let result_ev = result_by_tid.get(tid).copied();

        let (status, is_error, failure_summary) = if let Some(r) = result_ev {
            let tr_payload = r.payload.pointer("/tool_result");
            let is_err = tr_payload
                .and_then(|p| p.get("is_error"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let summary = if is_err {
                tr_payload
                    .and_then(|p| p.get("content"))
                    .and_then(|v| v.as_str())
                    .map(|s| truncate(s, 512))
            } else {
                None
            };
            if is_err {
                ("failed", true, summary)
            } else {
                ("passed", false, None)
            }
        } else {
            ("unknown", false, None)
        };
        let _ = is_error; // suppressed; status string is the canonical output

        // trigger_event_id = the tool_result event (or the tool_call if no result)
        let trigger_event_id = result_ev
            .map(|r| r.event_id.as_str())
            .unwrap_or(ev.event_id.as_str());

        let started_at = ev.observed_at.to_rfc3339();
        let ended_at = result_ev.map(|r| r.observed_at.to_rfc3339());

        let vr_id = derive_id(&ev.session_id, trigger_event_id, &started_at);

        if seen_trigger_ids.contains(trigger_event_id) {
            continue;
        }
        seen_trigger_ids.insert(trigger_event_id.to_string());

        out.push(VerificationRunRecord {
            verification_run_id: vr_id,
            schema_version: SCHEMA_VERSION,
            session_id: ev.session_id.clone(),
            source: "bash".into(),
            command: effective_cmd.to_string(),
            command_kind: command_kind.to_string(),
            trigger_event_id: trigger_event_id.to_string(),
            trigger_tool_use_id: Some(tid.to_string()),
            status: status.into(),
            started_at,
            ended_at,
            exit_code: None, // not available in transcript; OTel branch may set this
            failure_summary,
            raw_event_id: result_ev
                .map(|r| r.raw_event_id.as_str())
                .unwrap_or(ev.raw_event_id.as_str())
                .to_string(),
            parser_version: PARSER_VERSION,
        });
    }

    // --- Hook branch ---
    // `HookEvent` with `subkind == "PostToolUse"` and the matched Bash
    // command on the allowlist. Deduped by trigger_event_id.
    for ev in evs {
        if ev.kind != EventKind::HookEvent {
            continue;
        }
        if ev.subkind.as_deref() != Some("post_tool_use") {
            continue;
        }
        let tool_name = ev
            .payload
            .pointer("/hook/hook_input/tool_name")
            .and_then(|v| v.as_str());
        if tool_name != Some("Bash") {
            continue;
        }
        let cmd = ev
            .payload
            .pointer("/hook/hook_input/tool_input/command")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let effective_cmd = normalise_command(cmd);
        let Some(_command_kind) = classify(effective_cmd) else {
            continue;
        };
        let trigger_event_id = ev.event_id.as_str();
        if seen_trigger_ids.contains(trigger_event_id) {
            continue;
        }
        // Hook rows are lower priority than Bash rows — if already produced
        // via the Bash branch (same trigger_event_id via cross-reference),
        // skip. We do not attempt cross-branch dedup here; same trigger_event_id
        // is the only dedup key we have.
        // (Hook rows are rare — most verification runs come from Bash branch.)
        seen_trigger_ids.insert(trigger_event_id.to_string());
        let started_at = ev.observed_at.to_rfc3339();
        let vr_id = derive_id(&ev.session_id, trigger_event_id, &started_at);
        out.push(VerificationRunRecord {
            verification_run_id: vr_id,
            schema_version: SCHEMA_VERSION,
            session_id: ev.session_id.clone(),
            source: "hook".into(),
            command: effective_cmd.to_string(),
            command_kind: _command_kind.to_string(),
            trigger_event_id: trigger_event_id.to_string(),
            trigger_tool_use_id: ev.tool_use_id.clone(),
            status: "unknown".into(), // hook events don't carry exit status directly
            started_at,
            ended_at: None,
            exit_code: None,
            failure_summary: None,
            raw_event_id: ev.raw_event_id.clone(),
            parser_version: PARSER_VERSION,
        });
    }

    // --- OTel branch (DEV-S11-01) ---
    // `OtelSpan` with `attributes["verification.kind"]`. No real data yet;
    // this branch is here so the spec surface is locked and future spans are
    // not silently dropped.
    for ev in evs {
        if ev.kind != EventKind::OtelSpan {
            continue;
        }
        // DEV-S11-01: no real fixture; branch never fires in production.
        // Only reachable via tests/verification_otel_synth.rs which uses
        // a synthetic OtelSpan with this attribute.
        let vk = ev
            .payload
            .pointer("/telemetry/attributes/verification.kind")
            .or_else(|| ev.payload.get("verification.kind"))
            .and_then(|v| v.as_str());
        let Some(vk_str) = vk else { continue };

        let trigger_event_id = ev.event_id.as_str();
        if seen_trigger_ids.contains(trigger_event_id) {
            continue;
        }
        if ev.trace_id.is_none() && ev.span_id.is_none() {
            // DEV-S11-01: drop + warn per spec §7
            tracing::warn!(
                "verification_run extractor: OtelSpan with verification.kind but no trace_id/span_id; dropping"
            );
            continue;
        }
        seen_trigger_ids.insert(trigger_event_id.to_string());
        let started_at = ev.observed_at.to_rfc3339();
        let vr_id = derive_id(&ev.session_id, trigger_event_id, &started_at);
        out.push(VerificationRunRecord {
            verification_run_id: vr_id,
            schema_version: SCHEMA_VERSION,
            session_id: ev.session_id.clone(),
            source: "otel".into(),
            command: vk_str.to_string(),
            command_kind: vk_str.to_string(),
            trigger_event_id: trigger_event_id.to_string(),
            trigger_tool_use_id: None,
            status: "unknown".into(),
            started_at,
            ended_at: None,
            exit_code: None,
            failure_summary: None,
            raw_event_id: ev.raw_event_id.clone(),
            parser_version: PARSER_VERSION,
        });
    }

    out
}

/// Derive a deterministic `verification_run_id` from session + trigger + time.
/// Format: `vr_` + first 16 hex chars of sha256(session_id||trigger_event_id||started_at).
fn derive_id(session_id: &str, trigger_event_id: &str, started_at: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(session_id.as_bytes());
    hasher.update(b"||");
    hasher.update(trigger_event_id.as_bytes());
    hasher.update(b"||");
    hasher.update(started_at.as_bytes());
    let hash = hasher.finalize();
    format!("vr_{}", hex::encode(&hash[..8]))
}

/// Normalise a shell command string for allowlist matching.
///
/// Strategy: take everything up to the first `2>&1`, `|`, `;`, or `&&`
/// token. This handles the common pattern `cargo test 2>&1 | tail -5`
/// where the useful prefix is `cargo test`.
fn normalise_command(cmd: &str) -> &str {
    // Find the first occurrence of shell metacharacter sequences.
    // We scan for: " 2>&1", " |", " ;", " &&", " &"
    let seps = [" 2>&1", " |", " ;", " &&", " &"];
    let cut = seps
        .iter()
        .filter_map(|sep| cmd.find(sep))
        .min();
    if let Some(pos) = cut {
        cmd[..pos].trim_end()
    } else {
        cmd.trim_end()
    }
}

/// Truncate a string to at most `max_bytes` bytes (UTF-8 safe).
fn truncate(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        s.to_string()
    } else {
        // Find a UTF-8 boundary
        let mut end = max_bytes;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s[..end].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalise_removes_pipe_redirect() {
        assert_eq!(normalise_command("cargo test 2>&1 | tail -5"), "cargo test");
        assert_eq!(normalise_command("cargo test | tail -5"), "cargo test");
        assert_eq!(normalise_command("cargo test --lib"), "cargo test --lib");
        assert_eq!(normalise_command("cargo test && echo done"), "cargo test");
    }

    #[test]
    fn derive_id_is_deterministic() {
        let a = derive_id("sess", "ev1", "2026-05-27T10:00:00Z");
        let b = derive_id("sess", "ev1", "2026-05-27T10:00:00Z");
        assert_eq!(a, b);
        assert!(a.starts_with("vr_"));
    }
}
