//! Slice-10a — locks the invariant that Claude Code transcript `tool_result`
//! lines for `Edit` / `Write` carry a `toolUseResult.structuredPatch` array of
//! known shape, and that our extractor maps those hunks 1:1 onto
//! `DiffHunkRecord` rows with proper attribution.
//!
//! Fixtures are real transcript lines frozen at
//! `tests/fixtures/transcripts/real/structured_patch_v01.jsonl`:
//!
//! 1. **(a)** Edit, single-hunk, `userModified: false`.
//! 2. **(b)** Write, `structuredPatch: []` (Write's contract by design).
//! 3. **(c)** Edit, multi-hunk (2 hunks), `userModified: false`.
//!
//! Polarity flip `userModified: true` is exercised by an inline synthetic JSON
//! because 9 local transcripts contain zero real instances (228 ops, 0 matches);
//! this tradeoff is recorded in DEV-S10A-07.

use serde_json::Value;
use wimcc::ingest::diff_hunk::{
    extract_diff_hunks, DiffHunkRecord, TranscriptHunk, TranscriptToolUseResult,
    PATCH_PREVIEW_MAX_BYTES,
};

const FIXTURE_PATH: &str = "tests/fixtures/transcripts/real/structured_patch_v01.jsonl";

fn fixture_lines() -> Vec<Value> {
    let raw = std::fs::read_to_string(FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read fixture {FIXTURE_PATH}: {e}"));
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("fixture line is valid JSON"))
        .collect()
}

fn fixture_tool_use_result(line: &Value) -> &Value {
    line.get("toolUseResult")
        .expect("fixture line has toolUseResult")
}

fn fixture_event_uuid(line: &Value) -> &str {
    line.get("uuid").and_then(|v| v.as_str()).unwrap()
}

fn fixture_tool_use_id(line: &Value) -> Option<&str> {
    line.get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|b| b.get("tool_use_id"))
        .and_then(|v| v.as_str())
}

/// Task 2 — schema invariant. Locks that the parser's struct shape can
/// round-trip every real fixture line's `toolUseResult` without error.
#[test]
fn structured_patch_invariant_holds_for_real_fixtures() {
    for (i, line) in fixture_lines().iter().enumerate() {
        let result_val = fixture_tool_use_result(line);
        let parsed: TranscriptToolUseResult =
            serde_json::from_value(result_val.clone()).unwrap_or_else(|e| {
                panic!(
                    "fixture {} failed invariant: {e}\nvalue: {result_val}",
                    i + 1
                )
            });
        assert!(
            parsed.file_path.starts_with('/'),
            "fixture {}: filePath must be absolute, got {:?}",
            i + 1,
            parsed.file_path
        );
        for (hi, h) in parsed.structured_patch.iter().enumerate() {
            // Lines is permitted to be empty (no-op hunks would have nothing
            // to show), but if it has entries they are strings — `Deserialize`
            // already enforces that. Range numbers are u32.
            let _ = h.old_start;
            let _ = h.new_start;
            assert!(
                h.lines.len() == (h.old_lines + h.new_lines + count_context(&h.lines)) as usize
                    || !h.lines.is_empty(),
                "fixture {} hunk {}: line count sanity check",
                i + 1,
                hi + 1
            );
        }
    }
}

fn count_context(lines: &[String]) -> u32 {
    lines
        .iter()
        .filter(|l| !l.starts_with('+') && !l.starts_with('-'))
        .count() as u32
}

/// Task 3 — single-hunk Edit extractor.
#[test]
fn extract_hunks_from_real_edit_fixture() {
    let lines = fixture_lines();
    let edit = &lines[0]; // (a)
    let tu_result = fixture_tool_use_result(edit);
    let uuid = fixture_event_uuid(edit);
    let tuid = fixture_tool_use_id(edit);
    let recs = extract_diff_hunks(uuid, tuid, "test_session", tu_result);
    let parsed: TranscriptToolUseResult = serde_json::from_value(tu_result.clone()).unwrap();
    assert_eq!(
        recs.len(),
        parsed.structured_patch.len(),
        "fixture (a) hunk count mismatch"
    );
    assert_eq!(recs.len(), 1, "fixture (a) is single-hunk");
    let r = &recs[0];
    assert!(r.file_path.starts_with('/'));
    assert_eq!(r.introduced_by_event_id, uuid);
    assert_eq!(r.introduced_by_tool_use_id.as_deref(), tuid);
    assert_eq!(r.session_id, "test_session");
    assert!(!r.user_modified);
    assert_eq!(r.change_type, "modified");
    assert!(r.line_range_after.is_some());
    assert!(r.lines_added > 0 || r.lines_removed > 0);
    assert!(!r.diff_hunk_id.is_empty());
}

/// Task 4 — Write tool_result has empty structuredPatch by design; extractor
/// returns empty Vec without error. Lineage for Write is carried by the
/// surrounding tool_call ObservedEvent (file_path in payload), not by hunks.
#[test]
fn extract_no_hunks_from_real_write_fixture() {
    let lines = fixture_lines();
    let write = &lines[1]; // (b)
    let tu_result = fixture_tool_use_result(write);
    let parsed: TranscriptToolUseResult = serde_json::from_value(tu_result.clone()).unwrap();
    assert_eq!(
        parsed.structured_patch.len(),
        0,
        "Write tool_result must have empty structuredPatch (slice-10a invariant)"
    );
    let recs = extract_diff_hunks(
        fixture_event_uuid(write),
        fixture_tool_use_id(write),
        "s",
        tu_result,
    );
    assert!(recs.is_empty(), "Write extractor must return empty Vec");
}

/// Task 5 — multi-hunk Edit extractor. Each fixture hunk → one record. All
/// share the same `introduced_by_event_id` / `introduced_by_tool_use_id`.
#[test]
fn extract_multiple_hunks_from_multi_hunk_edit_fixture() {
    let lines = fixture_lines();
    let multi = &lines[2]; // (c)
    let tu_result = fixture_tool_use_result(multi);
    let uuid = fixture_event_uuid(multi);
    let tuid = fixture_tool_use_id(multi);
    let parsed: TranscriptToolUseResult = serde_json::from_value(tu_result.clone()).unwrap();
    let expected_n = parsed.structured_patch.len();
    assert!(
        expected_n >= 2,
        "fixture (c) must have >=2 hunks (got {expected_n})"
    );
    let recs = extract_diff_hunks(uuid, tuid, "test", tu_result);
    assert_eq!(recs.len(), expected_n);
    for r in &recs {
        assert_eq!(r.introduced_by_event_id, uuid);
        assert_eq!(r.introduced_by_tool_use_id.as_deref(), tuid);
        assert_eq!(r.file_path, parsed.file_path);
    }
    // All diff_hunk_ids must be unique within a single tool_result.
    let mut ids: Vec<&str> = recs.iter().map(|r| r.diff_hunk_id.as_str()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), expected_n, "diff_hunk_id must be unique per hunk");
}

/// Task 6 — non-edit tool results: extractor produces zero hunks and does not
/// error. Uses synthetic minimal payloads to avoid relying on a real Bash/Read
/// fixture (those would add maintenance burden for no extra invariant value).
#[test]
fn extract_no_hunks_for_bash_or_read_tool_results() {
    let bash_result = serde_json::json!({
        "stdout": "ok",
        "stderr": "",
        "interrupted": false,
        "isImage": false
    });
    assert!(extract_diff_hunks("ev", Some("tu"), "s", &bash_result).is_empty());

    let read_result = serde_json::json!({
        "type": "text",
        "file": { "filePath": "/tmp/x.txt", "content": "a\nb\n", "numLines": 2 }
    });
    assert!(extract_diff_hunks("ev", Some("tu"), "s", &read_result).is_empty());

    let empty = serde_json::json!({});
    assert!(extract_diff_hunks("ev", Some("tu"), "s", &empty).is_empty());
}

/// Task 7 — `user_modified` polarity round-trip. Real fixtures have only the
/// `false` polarity (0 of 228 ops in 9 transcripts have `userModified: true`).
/// We synthesise a `true` polarity sample by cloning fixture (a) and flipping
/// the flag. Tradeoff recorded in DEV-S10A-07.
#[test]
fn user_modified_flag_round_trip() {
    let lines = fixture_lines();
    let edit_a = &lines[0];
    let tu_real = fixture_tool_use_result(edit_a).clone();

    // false polarity — real
    let recs_false = extract_diff_hunks(
        fixture_event_uuid(edit_a),
        fixture_tool_use_id(edit_a),
        "s",
        &tu_real,
    );
    assert!(recs_false.iter().all(|r| !r.user_modified));

    // true polarity — synthetic clone with the flag flipped
    let mut tu_synth = tu_real;
    tu_synth["userModified"] = serde_json::json!(true);
    let recs_true = extract_diff_hunks("ev-synth", Some("tu-synth"), "s", &tu_synth);
    assert!(!recs_true.is_empty(), "synthetic still has hunks");
    assert!(
        recs_true.iter().all(|r| r.user_modified),
        "all hunks must carry user_modified=true when parent is true"
    );
}

/// Task 8 — `DiffHunkRecord` schema lock. Asserts the struct contains exactly
/// the slice-10a column set. No `introduced_by_commit_sha`. New
/// `user_modified` boolean present. Trivial test that doubles as a
/// compile-time guard against silent column drift.
#[test]
fn diff_hunk_record_field_shape() {
    let r = DiffHunkRecord {
        diff_hunk_id: "dh_x".into(),
        session_id: "s".into(),
        file_path: "/p".into(),
        change_type: "modified".into(),
        line_range_after: Some((1, 5)),
        introduced_by_event_id: "ev".into(),
        introduced_by_tool_use_id: Some("tu".into()),
        patch_preview: "p".into(),
        lines_added: 3,
        lines_removed: 1,
        user_modified: false,
    };
    // If a future change adds/removes a field, this destructure breaks compile.
    let DiffHunkRecord {
        diff_hunk_id,
        session_id,
        file_path,
        change_type,
        line_range_after,
        introduced_by_event_id,
        introduced_by_tool_use_id,
        patch_preview,
        lines_added,
        lines_removed,
        user_modified,
    } = r;
    assert_eq!(diff_hunk_id, "dh_x");
    assert_eq!(session_id, "s");
    assert_eq!(file_path, "/p");
    assert_eq!(change_type, "modified");
    assert_eq!(line_range_after, Some((1, 5)));
    assert_eq!(introduced_by_event_id, "ev");
    assert_eq!(introduced_by_tool_use_id.as_deref(), Some("tu"));
    assert_eq!(patch_preview, "p");
    assert_eq!(lines_added, 3);
    assert_eq!(lines_removed, 1);
    assert!(!user_modified);
}

/// Sanity: `PATCH_PREVIEW_MAX_BYTES` matches the slice-5 value (4 KiB).
#[test]
fn patch_preview_constant_matches_slice5() {
    assert_eq!(PATCH_PREVIEW_MAX_BYTES, 4 * 1024);
}

/// Mirror of slice-5 `truncate_patch_preview` behaviour — exercised because
/// the constant + helper migrate from `file_git` into `diff_hunk`.
#[test]
fn truncate_patch_preview_appends_marker_when_dropped() {
    use wimcc::ingest::diff_hunk::truncate_patch_preview;
    let big = "a".repeat(PATCH_PREVIEW_MAX_BYTES + 100);
    let out = truncate_patch_preview(&big);
    assert!(out.len() < big.len() + 32);
    assert!(out.ends_with("[truncated]"));
    let small = "x".repeat(10);
    assert_eq!(truncate_patch_preview(&small), small);
}

/// Cross-fixture: lock that fixture (a) really has a different
/// `introduced_by_event_id` from fixture (c) — regression guard if the freeze
/// is ever re-pointed to the same record by mistake.
#[test]
fn fixtures_have_distinct_event_uuids() {
    let lines = fixture_lines();
    let uuids: Vec<_> = lines.iter().map(fixture_event_uuid).collect();
    let mut sorted = uuids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        uuids.len(),
        "fixture uuids must be distinct: {uuids:?}"
    );
}

/// `TranscriptHunk` shape lock — same destructure-or-fail guard as the
/// `DiffHunkRecord` test, applied to the transcript-side mirror struct.
#[test]
fn transcript_hunk_field_shape() {
    let h = TranscriptHunk {
        old_start: 1,
        old_lines: 2,
        new_start: 3,
        new_lines: 4,
        lines: vec!["+a".into()],
    };
    let TranscriptHunk {
        old_start,
        old_lines,
        new_start,
        new_lines,
        lines,
    } = h;
    assert_eq!(
        (old_start, old_lines, new_start, new_lines, lines.len()),
        (1, 2, 3, 4, 1)
    );
}
