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

use crate::insight::outcome::{resolve_outcome, OutcomeProvenance, OutcomeStatus};
use crate::insight::verification_allowlist::classify_segment;
use crate::model::observed::{EventKind, ObservedEvent};

pub const PARSER_VERSION: &str = "verification_run@v1";
pub const SCHEMA_VERSION: &str = "verification_run.v1";

/// A single verification run row ready for DB insertion.
#[derive(Debug, Clone)]
pub struct VerificationRunRecord {
    pub verification_run_id: String,
    pub schema_version: &'static str,
    pub session_id: String,
    pub source: String,              // "bash" | "hook" | "otel"
    pub command: String,
    pub command_kind: String,
    pub trigger_event_id: String,
    pub trigger_tool_use_id: Option<String>,
    pub status: String,              // "passed" | "failed" | "unknown"
    pub status_provenance: Option<String>, // "measured" | "estimated" | "unknown"
    pub detection_basis: String,     // "known_tool" | "test_keyword"
    pub status_basis: String,        // "exit" | "piped"
    pub started_at: String,          // ISO 8601 UTC
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
/// - Derives status via `resolve_outcome` (Plan 6 OTLP-first chain).
///   `is_error` is NOT used for pass/fail — it only signals tool-execution.
///   If the chain returns Unknown and the command_kind is test/build/lint,
///   a content failure rule (Tier-4, estimated) may upgrade to Failed.
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

        // Segment-split + per-segment classify: picks the first segment that
        // matches a verification tool (Tier-1) or test/spec keyword (Tier-2),
        // so `cd webui && npx vitest run` (or `cd webui\nnpx vitest run`) is
        // detected on its `npx vitest run` segment, not the leading `cd`.
        let Some(m) = matched_segment(cmd) else {
            continue;
        };
        let command_kind = m.command_kind;
        let effective_cmd = m.command.as_str();

        let Some(tid) = ev.tool_use_id.as_deref() else {
            continue;
        };

        // Find paired tool_result
        let result_ev = result_by_tid.get(tid).copied();

        // ── Plan 6: resolve_outcome (OTLP-first chain) ───────────────────────
        // Pass ALL session events so OTLP log_record / hook events are also
        // considered, not just the paired tool_result.
        // is_error is intentionally ignored for status — it reflects only
        // whether the tool executor accepted the call, not the command exit.
        let resolved = resolve_outcome(evs, tid);

        // Tier-4: for verification kinds (test/build/lint/format), when the
        // chain returns Unknown, apply a content failure rule (estimated).
        // This catches "error[" / "FAILED" in cargo output without OTLP.
        let (resolved_status, resolved_prov) = if resolved.status == OutcomeStatus::Unknown
            && is_verification_kind(command_kind)
        {
            if let Some(r) = result_ev {
                let content = r
                    .payload
                    .pointer("/tool_result/content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if looks_like_failure(content) {
                    (OutcomeStatus::Failed, OutcomeProvenance::Estimated)
                } else {
                    (resolved.status, resolved.provenance)
                }
            } else {
                (resolved.status, resolved.provenance)
            }
        } else {
            (resolved.status, resolved.provenance)
        };

        let failure_summary = if resolved_status == OutcomeStatus::Failed {
            result_ev
                .and_then(|r| r.payload.pointer("/tool_result/content"))
                .and_then(|v| v.as_str())
                .map(|s| truncate(s, 512))
        } else {
            None
        };

        let status = match resolved_status {
            OutcomeStatus::Passed => "passed",
            OutcomeStatus::Failed => "failed",
            OutcomeStatus::Unknown => "unknown",
        };
        let status_provenance_str: &str = match resolved_prov {
            OutcomeProvenance::Measured => "measured",
            OutcomeProvenance::Estimated => "estimated",
            OutcomeProvenance::Unknown => "unknown",
        };

        // status_basis: when the matched segment is piped to a non-pager,
        // the exit code is masked → force status to "unknown" (design §6.2).
        let (status, status_provenance) = if m.status_basis == "piped" {
            ("unknown", "unknown".to_string())
        } else {
            (status, status_provenance_str.to_string())
        };
        let status_provenance = Some(status_provenance);
        let failure_summary = if status == "failed" { failure_summary } else { None };

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
            status_provenance,
            detection_basis: m.detection_basis.to_string(),
            status_basis: m.status_basis.to_string(),
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
        let Some(m) = matched_segment(cmd) else {
            continue;
        };
        let _command_kind = m.command_kind;
        let effective_cmd = m.command.as_str();
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
        // Hook branch: apply resolve_outcome with all session events.
        // Hook post_tool_use carries exit_code → measured if present.
        let hook_resolved = resolve_outcome(evs, ev.tool_use_id.as_deref().unwrap_or(""));
        let hook_status = match hook_resolved.status {
            OutcomeStatus::Passed => "passed",
            OutcomeStatus::Failed => "failed",
            OutcomeStatus::Unknown => "unknown",
        };
        let hook_status_provenance = Some(match hook_resolved.provenance {
            OutcomeProvenance::Measured => "measured".to_string(),
            OutcomeProvenance::Estimated => "estimated".to_string(),
            OutcomeProvenance::Unknown => "unknown".to_string(),
        });
        out.push(VerificationRunRecord {
            verification_run_id: vr_id,
            schema_version: SCHEMA_VERSION,
            session_id: ev.session_id.clone(),
            source: "hook".into(),
            command: effective_cmd.to_string(),
            command_kind: _command_kind.to_string(),
            trigger_event_id: trigger_event_id.to_string(),
            trigger_tool_use_id: ev.tool_use_id.clone(),
            status: hook_status.into(),
            status_provenance: hook_status_provenance,
            detection_basis: m.detection_basis.to_string(),
            status_basis: m.status_basis.to_string(),
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
            status_provenance: Some("unknown".to_string()),
            // OTel spans are detected by attribute, not command parsing →
            // known_tool with exit-derived status semantics.
            detection_basis: "known_tool".to_string(),
            status_basis: "exit".to_string(),
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

/// One matched command segment within a (possibly compound) Bash command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedSegment {
    /// The matched segment text (wrapper still present, redirects retained).
    pub command: String,
    pub command_kind: &'static str,
    pub detection_basis: &'static str, // "known_tool" | "test_keyword"
    pub status_basis: &'static str,    // "exit" | "piped"
}

/// Pager / output-filter commands. When the matched segment is piped INTO one
/// of these, the pipe is an output-capture idiom and the verification tool's
/// exit is still considered observable (status_basis = "exit").
const PAGER_COMMANDS: &[&str] = &["tail", "head", "cat", "less", "more", "wc"];

/// Split a compound shell command into simple-command segments on the
/// connectors `&& || | ; &` and the NEWLINE separator. The `2>&1` redirect is
/// NOT a connector and stays attached to its segment. Empty segments dropped.
///
/// Real-data anchoring: in this project's transcripts (session 653ea169)
/// `cd .../webui` and `npx vitest run …` are joined by a literal newline, so
/// newline MUST be treated as a connector (see
/// `tests/fixtures/transcripts/real/verification_npx_v01.jsonl`).
pub fn split_segments(cmd: &str) -> Vec<String> {
    let bytes = cmd.as_bytes();
    let mut segs: Vec<String> = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    let push = |segs: &mut Vec<String>, s: &str| {
        let t = s.trim();
        if !t.is_empty() {
            segs.push(t.to_string());
        }
    };
    while i < bytes.len() {
        let two = cmd.get(i..i + 2);
        if two == Some("&&") || two == Some("||") {
            push(&mut segs, &cmd[start..i]);
            i += 2;
            start = i;
            continue;
        }
        let c = bytes[i] as char;
        // A `&` that immediately follows `>` is part of a redirect (`2>&1`,
        // `>&2`), NOT a background connector — leave it attached to the segment.
        if c == '&' && i > 0 && bytes[i - 1] == b'>' {
            i += 1;
            continue;
        }
        // single-char connectors: pipe, semicolon, background, and newline.
        // (`&&`/`||` already handled above.)
        if c == '|' || c == ';' || c == '&' || c == '\n' {
            push(&mut segs, &cmd[start..i]);
            i += 1;
            start = i;
            continue;
        }
        i += 1;
    }
    push(&mut segs, &cmd[start..]);
    segs
}

/// Evaluate a compound Bash command and return the FIRST segment that matches
/// a verification tool (Tier-1) or test/spec keyword (Tier-2), with its
/// `status_basis` (whether a downstream non-pager pipe masks the exit code).
pub fn matched_segment(cmd: &str) -> Option<MatchedSegment> {
    let segs = split_segments(cmd);
    for (idx, seg) in segs.iter().enumerate() {
        if let Some((kind, basis)) = classify_segment(seg) {
            // status_basis: examine the connector that follows the matched
            // segment in the ORIGINAL command. If it is a pipe `|` into a
            // non-pager command, the exit code is masked → "piped". A trailing
            // pager pipe (tail/head/…) or any non-pipe connector keeps "exit".
            let status_basis = downstream_status_basis(cmd, seg, &segs, idx);
            return Some(MatchedSegment {
                command: seg.clone(),
                command_kind: kind,
                detection_basis: basis,
                status_basis,
            });
        }
    }
    None
}

/// Decide "exit" vs "piped" for the matched segment at `idx`.
fn downstream_status_basis(cmd: &str, seg: &str, segs: &[String], idx: usize) -> &'static str {
    // Find the byte position right after the matched segment text in `cmd`.
    let Some(seg_pos) = cmd.find(seg) else {
        return "exit";
    };
    let after = cmd[seg_pos + seg.len()..].trim_start();
    // A pipe connector masks the exit code only if it is `|` (single) and the
    // next segment's leading command is NOT a pager.
    if after.starts_with('|') && !after.starts_with("||") {
        if let Some(next) = segs.get(idx + 1) {
            let next_lead = next.split_whitespace().next().unwrap_or("");
            if PAGER_COMMANDS.contains(&next_lead) {
                return "exit"; // output-capture idiom
            }
            return "piped";
        }
        return "piped";
    }
    "exit"
}

/// Normalise a shell command string for allowlist matching.
///
/// Strategy: take everything up to the first `2>&1`, `|`, `;`, or `&&`
/// token. This handles the common pattern `cargo test 2>&1 | tail -5`
/// where the useful prefix is `cargo test`.
///
/// Retained (and unit-tested by `normalise_removes_pipe_redirect`) as the
/// documented pre-rewrite behaviour; the extractor now uses `matched_segment`.
#[allow(dead_code)]
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

/// Returns true when the `command_kind` is a verification kind for which
/// Tier-4 (estimated) failure content rules are meaningful.
fn is_verification_kind(kind: &str) -> bool {
    matches!(
        kind,
        "test_suite_rust"
            | "test_suite_js"
            | "test_suite_py"
            | "test_suite_go"
            | "test_suite_java"
            | "test_suite_other"
            | "build"
            | "build_check"
            | "lint"
            | "format_check"
    )
}

/// Tier-4 estimated failure heuristic: checks for common failure patterns in
/// tool output when no OTLP/hook/exit-code signal is available.
///
/// NOT used when a measured signal (OTLP/hook/exit-code) exists. Only
/// fires when the chain returned Unknown and command_kind is a known
/// verification kind.
fn looks_like_failure(content: &str) -> bool {
    // cargo test / cargo build common failure indicators
    content.contains("error[")
        || content.contains("\nerror:")
        || content.starts_with("error:")
        || content.contains("FAILED")
        // npm/vitest/jest
        || content.contains("Tests failed")
        || content.contains("test failures")
        // pytest
        || content.contains("FAILURES")
        || content.contains("failed,")
        // cargo clippy
        || content.contains("aborting due to")
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

    #[test]
    fn split_segments_breaks_on_connectors() {
        assert_eq!(
            split_segments("cd webui && npx vitest run"),
            vec!["cd webui", "npx vitest run"]
        );
        assert_eq!(
            split_segments("cargo fmt && cargo clippy && cargo test"),
            vec!["cargo fmt", "cargo clippy", "cargo test"]
        );
        assert_eq!(split_segments("a ; b || c & d"), vec!["a", "b", "c", "d"]);
        // pipe is a connector too (for SEGMENT identification)
        assert_eq!(
            split_segments("cargo test | tail -5"),
            vec!["cargo test", "tail -5"]
        );
        // 2>&1 is a redirect, NOT a connector — stays attached to its segment
        assert_eq!(
            split_segments("cargo test 2>&1 | tail -5"),
            vec!["cargo test 2>&1", "tail -5"]
        );
    }

    #[test]
    fn split_segments_breaks_on_newline_real_data() {
        // Real-data anchoring: in this project's transcripts (session 653ea169)
        // `cd .../webui` and `npx vitest run …` are joined by a literal NEWLINE,
        // not `&&`. The detector MUST split on newline or the headline bug
        // (verification 0%) is unfixed. Frozen sample:
        //   tests/fixtures/transcripts/real/verification_npx_v01.jsonl
        assert_eq!(
            split_segments("cd /tmp/webui\nnpx vitest run 2>&1 | tail -12"),
            vec!["cd /tmp/webui", "npx vitest run 2>&1", "tail -12"]
        );
    }

    #[test]
    fn matched_segment_picks_first_known_tool_after_cd() {
        // The design spec's headline bug: cd is segment 0, the tool is segment 1.
        let m = matched_segment("cd webui && npx vitest run").expect("match");
        assert_eq!(m.command, "npx vitest run");
        assert_eq!(m.command_kind, "test_suite_js");
        assert_eq!(m.detection_basis, "known_tool");
        // tool segment is the LAST segment → exit code visible.
        assert_eq!(m.status_basis, "exit");
    }

    #[test]
    fn matched_segment_picks_known_tool_after_cd_newline_real_data() {
        // Real-data shape (newline connector). Pager pipe (tail) keeps exit basis.
        let m = matched_segment("cd /tmp/webui\nnpx vitest run 2>&1 | tail -12")
            .expect("match");
        assert_eq!(m.command, "npx vitest run 2>&1");
        assert_eq!(m.command_kind, "test_suite_js");
        assert_eq!(m.detection_basis, "known_tool");
        assert_eq!(m.status_basis, "exit");
    }

    #[test]
    fn matched_segment_pager_pipe_is_exit_basis() {
        // `… 2>&1 | tail` is an output-capture idiom: tail is a pager, so the
        // verification tool's exit is treated as observable (status_basis=exit).
        let m = matched_segment("cargo test 2>&1 | tail -40").expect("match");
        assert_eq!(m.command, "cargo test 2>&1");
        assert_eq!(m.command_kind, "test_suite_rust");
        assert_eq!(m.status_basis, "exit");
    }

    #[test]
    fn matched_segment_real_pipe_is_piped_basis() {
        // Piped to a NON-pager downstream command → exit code masked → piped.
        let m = matched_segment("npm test | grep FAIL").expect("match");
        assert_eq!(m.command_kind, "test_suite_js");
        assert_eq!(m.detection_basis, "known_tool");
        assert_eq!(m.status_basis, "piped");
    }

    #[test]
    fn matched_segment_keyword_tier() {
        let m = matched_segment("cd repo && ./run_e2e_test.sh").expect("match");
        assert_eq!(m.command_kind, "test_suite_other");
        assert_eq!(m.detection_basis, "test_keyword");
        assert_eq!(m.status_basis, "exit");
    }

    #[test]
    fn matched_segment_none_when_no_segment_matches() {
        assert!(matched_segment("cd webui && npm install").is_none());
        assert!(matched_segment("git status").is_none());
    }

    #[test]
    fn matched_segment_excludes_dry_run() {
        // dry-run / collect-only are not runs (slice directive #6).
        assert!(matched_segment("cargo test --no-run").is_none());
        assert!(matched_segment("cd webui && npx vitest run --no-run").is_none());
        assert!(matched_segment("pytest --collect-only").is_none());
    }
}
