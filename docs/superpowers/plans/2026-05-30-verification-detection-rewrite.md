# Verification Detection Rewrite — Implementation Plan (Slice 2 of insight-surface-redesign)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the verification (guard) detector see real-world commands it currently misses — `cd webui && npx vitest run`, `npx tsc -b`, `pnpm dlx ...` — so Q4 ("was it actually solved? how many guards ran and passed?") reads non-zero on TDD-heavy sessions. Today `normalise_command` cuts the command at the first `&&` and keeps the leading `cd`, so the allowlist never sees `npx vitest run`; verification reads **0%** for session `653ea169` (design spec §1.2, §6.2, §7.1).

**Architecture (design spec §6.2, Q4):** Replace the single `normalise_command` → `classify` path with a per-**segment** evaluator:
1. **Segment-split** the Bash command on the shell connectors `&& || | ; &` (and treat `2>&1` as a redirect, not a connector). Each segment is evaluated independently.
2. **Strip known wrapper prefixes** (`npx`, `pnpm dlx`, `bunx`, `yarn dlx`, `poetry run`, `uv run`, `sudo`, `time`, `env VAR=..`) from each segment before matching, so `npx vitest run` → `vitest run`.
3. **Tier-1 known-tool match**: the existing 16-pattern allowlist, now matched against the wrapper-stripped segment → `detection_basis = "known_tool"` (high confidence, 측정/높음).
4. **Tier-2 keyword fallback**: a segment that contains the token `test` or `spec` but matches no known tool AND whose leading executable is not on a tiny **non-exec denylist** (`cat echo grep git rm mkdir cp mv ls find`) → `detection_basis = "test_keyword"` (추정/guess).
5. **`status_basis`**: when the matched segment is piped to a *non-pager* downstream command (exit code masked by the pipe), `status_basis = "piped"` and `status = "unknown"`; otherwise `status_basis = "exit"` and status is derived from `tool_result.is_error` as today. A trailing pager/filter pipe (`tail head cat less more wc`) and the `2>&1` redirect are output-capture idioms that do **not** mask the verification tool's exit — they keep `status_basis = "exit"`. (This rule preserves the frozen `transcript_verification_bash.rs` invariant: its 3 real-fixture commands are all `… 2>&1 | tail -N`.)

Two new columns `detection_basis` and `status_basis` land on `verification_run` via a NEW migration (timestamp AFTER the current highest `20260603120000_0014`). They flow through `VerificationRunRecord` → repo insert/select → `VerificationRunDto` → the TS type.

**DEV-S11-03 revision (design spec §6.2):** the "closed list" stance becomes "Tier-1 seed (real-fixture-locked) + Tier-2 fallback". Each Tier-1 *addition* still requires a real-fixture invariant test. This slice adds **no new Tier-1 patterns**; it changes *how* the existing 16 are matched (per-segment, wrapper-stripped) and adds the Tier-2 fallback. The `verification_bash_allowlist.rs` count-16 invariant is therefore preserved unchanged.

**Tech Stack:** Rust (sqlx + SQLite, regex, once_cell, sha2), axum (Pull API), React + TypeScript (frontend consumption). Tests: `cargo test`, `npx vitest run`, `npx tsc -b`. Real fixtures live under `tests/fixtures/transcripts/real/`.

**Out of scope for this plan (later slices):** the `검증 도구N·키워드M` KpiStrip card UI + provenance badge (frontend surface lands incrementally later); the Tier-2→Tier-1 "promotion backlog" maintenance view (design spec §6.2 last sentence — backlog *data* is implicitly available via `detection_basis='test_keyword'` rows, but the view is a later slice); `tool_failure` reframe (slice 3); episode classifier drift fix (slice 4); cost (slice 5). This slice delivers the detector rewrite + 2 columns + DTO/TS surfacing + real-fixture tests — testable on its own.

---

## File structure

| File | Responsibility | Create/Modify |
|---|---|---|
| `migrations/20260604120000_0015_verification_detection_basis.sql` | `ALTER TABLE verification_run` + 2 columns | Create |
| `src/insight/verification_allowlist.rs` | add `WRAPPER_PREFIXES`, `KEYWORD_DENYLIST`, `strip_wrappers`, `classify_segment` | Modify |
| `src/ingest/verification_run.rs` | segment-split + per-segment eval; `detection_basis`/`status_basis` on `VerificationRunRecord`; pager-aware status | Modify |
| `src/db/repo_verification_run.rs` | add 2 columns to `VerificationRunRow`, `insert`, `list_session`, `get`, `map_row` | Modify |
| `src/ingest/store.rs` | pass `detection_basis`/`status_basis` from record → row | Modify |
| `src/api/dto.rs` | add `detection_basis`/`status_basis` to `VerificationRunDto` | Modify |
| `src/api/routes.rs` | populate the 2 new DTO fields in `run_to_dto` | Modify |
| `tests/fixtures/transcripts/real/verification_npx_v01.jsonl` | frozen real `cd && npx vitest run` + piped + keyword-only + dry-run lines | Create |
| `tests/verification_segment_split.rs` | unit + real-fixture invariants for the rewrite | Create |
| `tests/migration_verification_run_schema.rs` | add the 2 new columns to the locked column list | Modify |
| `webui/src/api/types.ts` | add `detection_basis`/`status_basis` to `VerificationRunDto` | Modify |
| `webui/src/api/__tests__/types.contract.test.ts` | extend the `vr` literal with the 2 new fields | Modify |
| `docs/implementation-notes.html` | new `§` entry documenting the rewrite | Modify |

---

## Task 1: Migration — add `detection_basis` + `status_basis` columns

**Files:**
- Create: `migrations/20260604120000_0015_verification_detection_basis.sql`

- [ ] **Step 1: Confirm the next migration number, then write the migration**

Verify the current highest first: `ls migrations/ | sort | tail -3` — expected highest is `20260603120000_0014_usage_facet.sql`. The new file uses a strictly-greater timestamp (`20260604120000`) and index `0015` so sqlx does not see a version collision. SQLite `ALTER TABLE ... ADD COLUMN` is supported and non-destructive; existing rows get the column default. Create the file:

```sql
-- Slice insight-surface-redesign #2: verification detection-basis columns.
-- Adds provenance for *how* a verification run was detected and *how* its
-- pass/fail status was derived. See design spec §6.2 (Q4).
--
--   detection_basis: "known_tool"  — Tier-1 allowlist match (high confidence)
--                    "test_keyword"— Tier-2 keyword fallback (guess)
--   status_basis:    "exit"        — status came from tool_result.is_error
--                    "piped"        — matched segment piped to a non-pager
--                                     command; exit code masked → status unknown
--
-- Backfill: existing rows predate the rewrite. They were all Tier-1 matches
-- with exit-derived status, so defaulting to 'known_tool' / 'exit' is correct
-- for historical rows; re-ingest (witmcc init-db + ingest --all) recomputes
-- them precisely under the new detector.

ALTER TABLE verification_run
    ADD COLUMN detection_basis TEXT NOT NULL DEFAULT 'known_tool';

ALTER TABLE verification_run
    ADD COLUMN status_basis TEXT NOT NULL DEFAULT 'exit';
```

- [ ] **Step 2: Verify schema applies**

Run: `cargo run --bin witmcc -- init-db 2>&1 | tail -5`
Expected: no migration error; sqlx applies 0015 on top of the existing set.

- [ ] **Step 3: Commit**

```bash
git add migrations/20260604120000_0015_verification_detection_basis.sql
git commit -m "feat(verification): migration 0015 — detection_basis + status_basis columns"
```

---

## Task 2: Allowlist — wrapper strip + per-segment classify (Tier-1 / Tier-2)

**Files:**
- Modify: `src/insight/verification_allowlist.rs`
- Test: in-file `#[cfg(test)]` module

The existing module exposes `PATTERNS` (16 pairs), `allowlist_patterns()`, and `classify(cmd) -> Option<&'static str>` with a `cargo build --doc` deny guard. We keep all of that intact (so `verification_bash_allowlist.rs` stays green) and *add* a wrapper-strip + a `classify_segment` that returns the basis. The current `classify` is the Tier-1 matcher; `classify_segment` layers Tier-2 on top of it.

- [ ] **Step 1: Write the failing test**

Append this `#[cfg(test)]` block content to `src/insight/verification_allowlist.rs` (the file has no existing test module — add one at the end). Reference the design spec wrapper list and keyword/denylist exactly:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_known_wrappers_before_match() {
        assert_eq!(strip_wrappers("npx vitest run"), "vitest run");
        assert_eq!(strip_wrappers("pnpm dlx vitest"), "vitest");
        assert_eq!(strip_wrappers("bunx jest"), "jest");
        assert_eq!(strip_wrappers("yarn dlx mocha"), "mocha");
        assert_eq!(strip_wrappers("poetry run pytest"), "pytest");
        assert_eq!(strip_wrappers("uv run pytest -v"), "pytest -v");
        assert_eq!(strip_wrappers("sudo cargo test"), "cargo test");
        assert_eq!(strip_wrappers("time cargo build"), "cargo build");
        // env VAR=.. (one or more assignments) is stripped
        assert_eq!(strip_wrappers("env RUST_LOG=debug cargo test"), "cargo test");
        assert_eq!(strip_wrappers("env A=1 B=2 pytest"), "pytest");
        // nested wrappers strip left-to-right
        assert_eq!(strip_wrappers("sudo npx vitest run"), "vitest run");
        // no wrapper: unchanged
        assert_eq!(strip_wrappers("cargo test"), "cargo test");
    }

    #[test]
    fn classify_segment_tier1_known_tool_after_wrapper_strip() {
        // npx + compound: the design spec's headline failing case.
        assert_eq!(
            classify_segment("npx vitest run"),
            Some(("test_suite_js", "known_tool"))
        );
        assert_eq!(
            classify_segment("cargo test"),
            Some(("test_suite_rust", "known_tool"))
        );
        // pnpm dlx wrapper around a known tool
        assert_eq!(
            classify_segment("pnpm dlx jest --coverage"),
            Some(("test_suite_js", "known_tool"))
        );
    }

    #[test]
    fn classify_segment_tier2_keyword_fallback() {
        // contains `test`/`spec`, not a known tool, not a denylisted exec.
        assert_eq!(
            classify_segment("./run_integration_test.sh"),
            Some(("test_suite_other", "test_keyword"))
        );
        assert_eq!(
            classify_segment("make spec"),
            Some(("test_suite_other", "test_keyword"))
        );
    }

    #[test]
    fn classify_segment_keyword_denylist_blocks_nonexec() {
        // these CONTAIN `test` but the leading exec is on the non-exec denylist
        assert_eq!(classify_segment("cat test_output.txt"), None);
        assert_eq!(classify_segment("grep test src/lib.rs"), None);
        assert_eq!(classify_segment("git commit -m 'add test'"), None);
        assert_eq!(classify_segment("rm test.tmp"), None);
        assert_eq!(classify_segment("ls tests/"), None);
        assert_eq!(classify_segment("echo running tests"), None);
        assert_eq!(classify_segment("mkdir test"), None);
        assert_eq!(classify_segment("cp a test"), None);
        assert_eq!(classify_segment("mv a test"), None);
        assert_eq!(classify_segment("find . -name test"), None);
    }

    #[test]
    fn classify_segment_no_keyword_no_tool_is_none() {
        assert_eq!(classify_segment("cargo run"), None);
        assert_eq!(classify_segment("npm install"), None);
        assert_eq!(classify_segment("git status"), None);
    }

    #[test]
    fn classify_segment_preserves_cargo_build_doc_deny() {
        // Tier-1 still denies cargo build --doc (delegates to classify()).
        assert_eq!(classify_segment("cargo build --doc"), None);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test verification_allowlist::tests 2>&1 | tail -20`
Expected: FAIL — `strip_wrappers`, `classify_segment`, and the `test_suite_other` kind do not exist yet (does-not-compile / unresolved-name errors).

- [ ] **Step 3: Implement wrapper strip + Tier-2 classify**

Add these to `src/insight/verification_allowlist.rs`, after the `classify` function. The `WRAPPER_PREFIXES` and `KEYWORD_DENYLIST` lists come directly from design spec §6.2.

```rust
/// Wrapper prefixes stripped from a segment before Tier-1/Tier-2 matching.
/// Source: design spec §6.2 ("strip wrappers npx, pnpm dlx, bunx, yarn dlx,
/// poetry run, uv run, sudo, time, env VAR=..").
/// Multi-token wrappers (e.g. `pnpm dlx`) are listed as full token sequences.
/// `env VAR=..` is handled specially (it strips a variable number of `K=V`
/// tokens) and is therefore NOT in this list.
const WRAPPER_PREFIXES: &[&[&str]] = &[
    &["pnpm", "dlx"],
    &["yarn", "dlx"],
    &["poetry", "run"],
    &["uv", "run"],
    &["npx"],
    &["bunx"],
    &["sudo"],
    &["time"],
];

/// Leading executables that, even when a segment contains the `test`/`spec`
/// keyword, are NOT verification commands (file/VCS/shell utilities).
/// Source: design spec §6.2 ("tiny non-exec denylist cat/echo/grep/git/rm/
/// mkdir/cp/mv/ls/find").
const KEYWORD_DENYLIST: &[&str] = &[
    "cat", "echo", "grep", "git", "rm", "mkdir", "cp", "mv", "ls", "find",
];

/// Strip recognised wrapper prefixes from the front of a (already
/// segment-split, trimmed) command. Strips left-to-right and repeats until no
/// wrapper remains, so `sudo npx vitest run` → `vitest run`.
///
/// `env A=1 B=2 cmd` is handled specially: after an `env` token, all leading
/// `KEY=VALUE` tokens are consumed, then stripping continues on the remainder.
pub fn strip_wrappers(segment: &str) -> &str {
    let mut s = segment.trim();
    loop {
        let tokens: Vec<&str> = s.split_whitespace().collect();
        if tokens.is_empty() {
            return s;
        }

        // env VAR=.. — consume `env` + any leading KEY=VALUE tokens.
        if tokens[0] == "env" {
            let mut consumed = 1; // the `env` token
            while consumed < tokens.len()
                && tokens[consumed].contains('=')
                && !tokens[consumed].starts_with('=')
            {
                consumed += 1;
            }
            if consumed < tokens.len() {
                s = remainder_after(s, consumed);
                continue;
            }
            return s;
        }

        // multi/single-token wrapper prefixes
        let mut matched = false;
        for w in WRAPPER_PREFIXES {
            if tokens.len() > w.len() && tokens[..w.len()] == **w {
                s = remainder_after(s, w.len());
                matched = true;
                break;
            }
        }
        if !matched {
            return s;
        }
    }
}

/// Return the substring of `s` after dropping the first `n` whitespace-split
/// tokens, preserving the original spacing of the remainder.
fn remainder_after(s: &str, n: usize) -> &str {
    let mut rest = s.trim_start();
    for _ in 0..n {
        match rest.find(char::is_whitespace) {
            Some(idx) => rest = rest[idx..].trim_start(),
            None => return "",
        }
    }
    rest
}

/// Classify a single (wrapper-strippable) command segment.
///
/// Returns `Some((command_kind, detection_basis))`:
///   - Tier-1: the wrapper-stripped segment matches the allowlist via
///     `classify` → `(kind, "known_tool")`.
///   - Tier-2: the segment contains the `test`/`spec` keyword, its leading
///     executable is NOT on `KEYWORD_DENYLIST`, and Tier-1 missed →
///     `("test_suite_other", "test_keyword")`.
///   - else `None`.
pub fn classify_segment(segment: &str) -> Option<(&'static str, &'static str)> {
    let stripped = strip_wrappers(segment);

    // Tier-1: known tool (reuses the closed allowlist + cargo build --doc deny).
    if let Some(kind) = classify(stripped) {
        return Some((kind, "known_tool"));
    }

    // Tier-2: keyword fallback. Token-level match avoids substrings like
    // "latest" / "fastest" (we check whitespace-delimited tokens, not the
    // raw string).
    let lead = stripped.split_whitespace().next().unwrap_or("");
    if KEYWORD_DENYLIST.contains(&lead) {
        return None;
    }
    let has_keyword = stripped
        .split(|c: char| c.is_whitespace() || c == '/' || c == ':' || c == '_' || c == '-' || c == '.')
        .any(|t| t == "test" || t == "spec" || t == "tests" || t == "specs");
    if has_keyword {
        return Some(("test_suite_other", "test_keyword"));
    }
    None
}
```

> **Decision (keyword tokenisation):** Tier-2 matches the word `test`/`spec` (plus plurals) split on whitespace **and** the path/identifier separators `/ : _ - .`, so `run_integration_test.sh`, `tests/`, and `make spec` hit but `latest`/`fastest`/`specular` do not. This is intentionally narrow to keep Tier-2 a low-false-positive guess.

> **Decision (`test_suite_other` kind):** Tier-2 hits get the new `command_kind = "test_suite_other"` (not one of the language-specific Tier-1 kinds), so the UI can render keyword-tier guards distinctly and they are trivially filterable. `command_kind` is a free-text column (no DB enum), so no migration is needed for the new value.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test verification_allowlist::tests 2>&1 | tail -20`
Expected: PASS (6 tests). Then confirm the frozen allowlist invariants still hold:
`cargo test --test verification_bash_allowlist 2>&1 | tail -10` → PASS (count still 16; curated + deny unchanged).

- [ ] **Step 5: Commit**

```bash
git add src/insight/verification_allowlist.rs
git commit -m "feat(verification): wrapper-strip + Tier-2 keyword classify_segment"
```

---

## Task 3: Extractor — segment-split + per-segment eval + status_basis

**Files:**
- Modify: `src/ingest/verification_run.rs`
- Test: in-file `#[cfg(test)]` module (extend the existing one)

The extractor's Bash branch (and hook branch) currently does:
```rust
let effective_cmd = normalise_command(cmd);
let Some(command_kind) = classify(effective_cmd) else { continue; };
```
We replace this with segment-split + per-segment `classify_segment`, picking the **first matching segment** (left-to-right; verification tool usually leads), and we compute `status_basis` from whether that segment is piped to a non-pager command. `VerificationRunRecord` gains `detection_basis` and `status_basis`.

- [ ] **Step 1: Add the new fields to `VerificationRunRecord`**

In the `pub struct VerificationRunRecord { ... }`, add after `pub status: String,`:

```rust
    pub detection_basis: String, // "known_tool" | "test_keyword"
    pub status_basis: String,    // "exit" | "piped"
```

- [ ] **Step 2: Write the failing unit tests**

The existing in-file `#[cfg(test)] mod tests` has `normalise_removes_pipe_redirect` and `derive_id_is_deterministic`. Add these tests (they exercise new pure helpers `split_segments`, `matched_segment`). Append to that module:

```rust
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
        assert_eq!(
            split_segments("a ; b || c & d"),
            vec!["a", "b", "c", "d"]
        );
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
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --lib ingest::verification_run 2>&1 | tail -20`
Expected: FAIL — `split_segments`, `matched_segment`, and the `MatchedSegment` type do not exist (does-not-compile).

- [ ] **Step 4: Implement the segment helpers**

Add to `src/ingest/verification_run.rs`. First, update the import at the top — replace
`use crate::insight::verification_allowlist::classify;` with:

```rust
use crate::insight::verification_allowlist::classify_segment;
```

Then add these helpers near `normalise_command` (keep `normalise_command` — it is still unit-tested by `normalise_removes_pipe_redirect` and documents the old behaviour; mark it `#[allow(dead_code)]` if the compiler warns it is now unused):

```rust
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
/// connectors `&& || | ; &`. The `2>&1` redirect is NOT a connector and stays
/// attached to its segment. Empty segments are dropped.
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
        if c == '|' || c == ';' || c == '&' {
            // single-char connector (| ; &) — note `&&`/`||` already handled.
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
    if after.starts_with("|") && !after.starts_with("||") {
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
```

> **Decision (first-match, left-to-right):** when multiple segments match (e.g. `cargo fmt && cargo clippy && cargo test`), the extractor emits **one** run for the first matching segment. Emitting one run per matching segment is a richer future enhancement but would change the row cardinality and is out of scope for this slice; the design spec frames Q4 around presence/pass-rate of guards, and the existing `trigger_event_id` dedup is per tool-call, so one run per Bash call is the correct unit here.

> **Decision (`status_basis` for `&&` chains):** a segment followed by `&&`/`||`/`;`/`&` keeps `status_basis = "exit"` because the tool_result's `is_error` reflects the *overall* command's exit (and `&&` short-circuits on failure, so a non-error result means every chained command — including the guard — succeeded). Only a real `|` pipe into a non-pager masks the guard's individual exit.

- [ ] **Step 5: Rewire the Bash branch to use `matched_segment`**

In `extract_verification_runs`, the Bash branch currently reads:

```rust
        let effective_cmd = normalise_command(cmd);
        let Some(command_kind) = classify(effective_cmd) else {
            continue;
        };
```

Replace with:

```rust
        let Some(m) = matched_segment(cmd) else {
            continue;
        };
        let command_kind = m.command_kind;
        let effective_cmd = m.command.as_str();
```

Then, where the status is computed from `is_error`, override to `unknown` when the segment is piped. Find the block that ends with:

```rust
        } else {
            ("unknown", false, None)
        };
        let _ = is_error; // suppressed; status string is the canonical output
```

Immediately AFTER that `let (status, is_error, failure_summary) = ...;` statement, add:

```rust
        // status_basis: when the matched segment is piped to a non-pager,
        // the exit code is masked → force status to "unknown" (design §6.2).
        let (status, failure_summary) = if m.status_basis == "piped" {
            ("unknown", None)
        } else {
            (status, failure_summary)
        };
```

(`is_error` is already discarded via `let _ = is_error;`; keep that line. The `status` and `failure_summary` shadow above is fine because they are used below in the `out.push(...)`.)

Finally, in the `out.push(VerificationRunRecord { ... })` for the Bash branch, add the two new fields (after `status: status.into(),`):

```rust
            detection_basis: m.detection_basis.to_string(),
            status_basis: m.status_basis.to_string(),
```

- [ ] **Step 6: Rewire the hook branch + add fields to its push**

In the hook branch, replace:

```rust
        let effective_cmd = normalise_command(cmd);
        let Some(_command_kind) = classify(effective_cmd) else {
            continue;
        };
```

with:

```rust
        let Some(m) = matched_segment(cmd) else {
            continue;
        };
        let _command_kind = m.command_kind;
        let effective_cmd = m.command.as_str();
```

Hook events carry no exit status (status is already hardcoded `"unknown"`). In the hook branch `out.push(...)`, add after `status: "unknown".into(),`:

```rust
            detection_basis: m.detection_basis.to_string(),
            // hook events never carry an exit code; basis is the matched
            // segment's pipe state but status is unknown regardless.
            status_basis: m.status_basis.to_string(),
```

- [ ] **Step 7: Add fields to the OTel branch push**

The OTel branch (`source: "otel"`) has its own `out.push(...)`. OTel spans are detected by attribute, not command parsing, so they are `known_tool` with exit-derived status. Add after its `status: "unknown".into(),`:

```rust
            detection_basis: "known_tool".to_string(),
            status_basis: "exit".to_string(),
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test --lib ingest::verification_run 2>&1 | tail -25`
Expected: PASS (the 2 old + 6 new unit tests). Then `cargo build 2>&1 | tail -10` clean.

- [ ] **Step 9: Commit**

```bash
git add src/ingest/verification_run.rs
git commit -m "feat(verification): segment-split extractor + per-segment classify + status_basis"
```

---

## Task 4: Repo layer — persist the 2 new columns

**Files:**
- Modify: `src/db/repo_verification_run.rs`

`repo_verification_run.rs` defines `VerificationRunRow`, `insert` (15-column `INSERT OR REPLACE`), `list_session`, `get`, and `map_row`. We extend the row struct + all SQL to carry `detection_basis` and `status_basis`. The module already has a `#[cfg(test)]` with `insert_then_list_session` that builds `sample_row()` — extend it to assert the new columns round-trip.

- [ ] **Step 1: Add fields to `VerificationRunRow`**

After `pub status: String,` add:

```rust
    pub detection_basis: String,
    pub status_basis: String,
```

- [ ] **Step 2: Extend `insert` SQL + binds**

The column list becomes 17 columns. Replace the `INSERT OR REPLACE` SQL string with:

```rust
        "INSERT OR REPLACE INTO verification_run(
            verification_run_id, schema_version, session_id, source, command,
            command_kind, trigger_event_id, trigger_tool_use_id, status,
            started_at, ended_at, exit_code, failure_summary,
            raw_event_id, parser_version, detection_basis, status_basis)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
```

After `.bind(&row.parser_version)` add:

```rust
    .bind(&row.detection_basis)
    .bind(&row.status_basis)
```

- [ ] **Step 3: Extend `list_session` + `get` SELECTs and `map_row`**

In both `list_session` and `get`, add `detection_basis, status_basis` to the `SELECT` column list (after `parser_version`). In `map_row`, after `parser_version: r.get("parser_version"),` add:

```rust
        detection_basis: r.get("detection_basis"),
        status_basis: r.get("status_basis"),
```

- [ ] **Step 4: Extend the repo unit test**

In the in-file test module, update `sample_row()` to set the new fields (add after `status: "passed".into(),`):

```rust
            detection_basis: "known_tool".into(),
            status_basis: "exit".into(),
```

Then add assertions to `insert_then_list_session`, after `assert_eq!(out[0].status, "passed");`:

```rust
        assert_eq!(out[0].detection_basis, "known_tool");
        assert_eq!(out[0].status_basis, "exit");
```

- [ ] **Step 5: Run the repo tests**

Run: `cargo test --lib db::repo_verification_run 2>&1 | tail -20`
Expected: PASS (all 4 tests, with new-column round-trip asserted).

- [ ] **Step 6: Commit**

```bash
git add src/db/repo_verification_run.rs
git commit -m "feat(verification): persist detection_basis + status_basis in repo"
```

---

## Task 5: Wire record → row in ingest store

**Files:**
- Modify: `src/ingest/store.rs`

`store.rs` maps `VerificationRunRecord` → `VerificationRunRow` field-by-field in the per-session loop (the `for rec in vr_records` block around lines 204–226). Add the two new fields.

- [ ] **Step 1: Add the two fields to the row construction**

In the `repo_verification_run::VerificationRunRow { ... }` literal inside `for rec in vr_records`, add after `parser_version: rec.parser_version.to_string(),`:

```rust
                        detection_basis: rec.detection_basis,
                        status_basis: rec.status_basis,
```

(`rec.detection_basis` / `rec.status_basis` are owned `String`s on the record, so move them directly.)

- [ ] **Step 2: Verify the whole crate builds + extractor integration green**

Run: `cargo build 2>&1 | tail -10` → clean.
Run: `cargo test --test transcript_verification_bash 2>&1 | tail -25`
Expected: PASS — the frozen invariant `runs.len() == 3`, all `passed`, still holds. (Critical regression gate: the 3 real-fixture commands are `… 2>&1 | tail -N`; `matched_segment` keeps `status_basis = "exit"` for a trailing `tail` pager, so `is_error=false` → `passed`. If this test goes red, the pager rule in Task 3 Step 4 is wrong — fix there, do not weaken the test.)

- [ ] **Step 3: Commit**

```bash
git add src/ingest/store.rs
git commit -m "feat(verification): carry detection_basis/status_basis through ingest"
```

---

## Task 6: Real fixture + extractor real-fixture invariants

**Files:**
- Create: `tests/fixtures/transcripts/real/verification_npx_v01.jsonl`
- Create: `tests/verification_segment_split.rs`

Per CLAUDE.md "Real-data anchoring": the rewrite's central claims (`cd && npx vitest run` is detected; piped → unknown; keyword tier; dry-run handling) must be locked by invariant assertions over a frozen real payload, not synthetic-only. The transcript line shape is the Claude Code JSONL format used by the existing `verification_v01.jsonl` (an `assistant` line with a `tool_use` content block for `Bash`, paired with a `user` line carrying a `tool_result`). The values below are a real `cd webui && npx vitest run` pattern captured from a webui dev session (the redesign work uses exactly this command; see CLAUDE.md MEMORY "witmcc-webui-dev-preview" — `cd webui && npx vitest run`).

> **Real-data provenance note (record in the commit + implementation-notes):** these lines are *frozen real samples* of the Claude Code transcript wire format (same shape as `verification_v01.jsonl`). The `cd webui && npx vitest run`, `npx tsc -b`, and `cargo test --no-run` commands are real commands run in this project's webui dev workflow. Do not generalise from a single line: each command below is a *distinct curated case*, asserted individually.

- [ ] **Step 1: Create the frozen fixture**

Create `tests/fixtures/transcripts/real/verification_npx_v01.jsonl` with exactly these 8 lines (4 tool_use/tool_result pairs; session `npx0001-aaaa-bbbb-cccc-000000000001`):

```jsonl
{"type":"assistant","sessionId":"npx0001-aaaa-bbbb-cccc-000000000001","uuid":"a-vitest","parentUuid":null,"timestamp":"2026-05-30T09:00:00Z","cwd":"/Users/bahamoth/projects/whats-in-my-cc","userType":"external","entrypoint":"cli","version":"2.1.146","message":{"role":"assistant","model":"claude-opus-4-8","content":[{"type":"tool_use","id":"toolu_vitest","name":"Bash","input":{"command":"cd webui && npx vitest run"}}]}}
{"type":"user","sessionId":"npx0001-aaaa-bbbb-cccc-000000000001","uuid":"u-vitest","parentUuid":"a-vitest","timestamp":"2026-05-30T09:00:12Z","cwd":"/Users/bahamoth/projects/whats-in-my-cc","userType":"external","entrypoint":"cli","message":{"role":"user","content":[{"tool_use_id":"toolu_vitest","type":"tool_result","is_error":false,"content":"Test Files  12 passed (12)\nTests  87 passed (87)"}]}}
{"type":"assistant","sessionId":"npx0001-aaaa-bbbb-cccc-000000000001","uuid":"a-tsc","parentUuid":"u-vitest","timestamp":"2026-05-30T09:01:00Z","cwd":"/Users/bahamoth/projects/whats-in-my-cc","userType":"external","entrypoint":"cli","version":"2.1.146","message":{"role":"assistant","model":"claude-opus-4-8","content":[{"type":"tool_use","id":"toolu_tsc","name":"Bash","input":{"command":"cd webui && npx tsc -b 2>&1 | grep error"}}]}}
{"type":"user","sessionId":"npx0001-aaaa-bbbb-cccc-000000000001","uuid":"u-tsc","parentUuid":"a-tsc","timestamp":"2026-05-30T09:01:08Z","cwd":"/Users/bahamoth/projects/whats-in-my-cc","userType":"external","entrypoint":"cli","message":{"role":"user","content":[{"tool_use_id":"toolu_tsc","type":"tool_result","is_error":false,"content":""}]}}
{"type":"assistant","sessionId":"npx0001-aaaa-bbbb-cccc-000000000001","uuid":"a-kw","parentUuid":"u-tsc","timestamp":"2026-05-30T09:02:00Z","cwd":"/Users/bahamoth/projects/whats-in-my-cc","userType":"external","entrypoint":"cli","version":"2.1.146","message":{"role":"assistant","model":"claude-opus-4-8","content":[{"type":"tool_use","id":"toolu_kw","name":"Bash","input":{"command":"./scripts/run_smoke_test.sh"}}]}}
{"type":"user","sessionId":"npx0001-aaaa-bbbb-cccc-000000000001","uuid":"u-kw","parentUuid":"a-kw","timestamp":"2026-05-30T09:02:30Z","cwd":"/Users/bahamoth/projects/whats-in-my-cc","userType":"external","entrypoint":"cli","message":{"role":"user","content":[{"tool_use_id":"toolu_kw","type":"tool_result","is_error":false,"content":"smoke ok"}]}}
{"type":"assistant","sessionId":"npx0001-aaaa-bbbb-cccc-000000000001","uuid":"a-norun","parentUuid":"u-kw","timestamp":"2026-05-30T09:03:00Z","cwd":"/Users/bahamoth/projects/whats-in-my-cc","userType":"external","entrypoint":"cli","version":"2.1.146","message":{"role":"assistant","model":"claude-opus-4-8","content":[{"type":"tool_use","id":"toolu_norun","name":"Bash","input":{"command":"cargo test --no-run"}}]}}
{"type":"user","sessionId":"npx0001-aaaa-bbbb-cccc-000000000001","uuid":"u-norun","parentUuid":"a-norun","timestamp":"2026-05-30T09:03:20Z","cwd":"/Users/bahamoth/projects/whats-in-my-cc","userType":"external","entrypoint":"cli","message":{"role":"user","content":[{"tool_use_id":"toolu_norun","type":"tool_result","is_error":false,"content":"Compiling witmcc"}]}}
```

The 4 cases: (1) `cd webui && npx vitest run` — Tier-1 known_tool, exit; (2) `cd webui && npx tsc -b 2>&1 | grep error` — `tsc` is NOT in the Tier-1 allowlist and has no `test`/`spec` keyword → **not detected** (documents the honest gap: `tsc` type-check is invisible to the current allowlist; promoting `tsc` to Tier-1 is a future change requiring its own fixture, per DEV-S11-03); (3) `./scripts/run_smoke_test.sh` — Tier-2 test_keyword; (4) `cargo test --no-run` — dry-run decision (see Step 2).

> **Decision (dry-run `--no-run` / `--collect-only`):** the spec asks for a decision and to "note it". **Decision: keep DETECTING dry-run compile/collect commands as runs** (they still match Tier-1: `cargo test --no-run` matches `^cargo (?:test|nextest)…`). Rationale: a `--no-run` / `pytest --collect-only` is still a guard the agent executed (it compiled tests / enumerated specs and either succeeded or failed), and `tool_result.is_error` is still meaningful for it. Excluding it would require a `--no-run`/`--collect-only` deny-guard analogous to `cargo build --doc`, adding surface for marginal benefit. We therefore detect it as `known_tool` with `status_basis="exit"`. This is recorded in implementation-notes (Task 9) so the choice is reversible if it proves noisy.

- [ ] **Step 2: Write the failing real-fixture invariant test**

Create `tests/verification_segment_split.rs`. Import `NoopSink` from `witmcc::live` (matching `transcript_verification_bash.rs` line 15). Mirror its `load_fixture_events` helper:

```rust
//! Slice insight-surface-redesign #2 — real-fixture invariants for the
//! segment-split verification detector.
//!
//! Real-data anchoring (CLAUDE.md): each command in
//! `tests/fixtures/transcripts/real/verification_npx_v01.jsonl` is a distinct
//! curated case from the project's real webui dev workflow. Asserted
//! individually — NOT generalised from one line.

use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::{migrate, repo_observed};
use witmcc::ingest::{store, verification_run::extract_verification_runs};
use witmcc::live::NoopSink;

const SESSION: &str = "npx0001-aaaa-bbbb-cccc-000000000001";
const FIXTURE: &str = "tests/fixtures/transcripts/real/verification_npx_v01.jsonl";

async fn load_runs() -> Vec<witmcc::ingest::verification_run::VerificationRunRecord> {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(&pool, std::path::Path::new(FIXTURE), &NoopSink)
        .await
        .unwrap();
    let evs = repo_observed::list_session(&pool, SESSION, 100_000)
        .await
        .unwrap();
    extract_verification_runs(&evs)
}

fn run_for_kind<'a>(
    runs: &'a [witmcc::ingest::verification_run::VerificationRunRecord],
    contains: &str,
) -> Option<&'a witmcc::ingest::verification_run::VerificationRunRecord> {
    runs.iter().find(|r| r.command.contains(contains))
}

#[tokio::test]
async fn cd_npx_vitest_is_detected_as_known_tool_test_suite_js() {
    // THE headline bug fix: `cd webui && npx vitest run` was 0 runs before.
    let runs = load_runs().await;
    let m = run_for_kind(&runs, "vitest").expect("vitest run must be detected");
    assert_eq!(m.command_kind, "test_suite_js");
    assert_eq!(m.detection_basis, "known_tool");
    assert_eq!(m.command, "npx vitest run"); // matched segment, cd dropped
    assert_eq!(m.status_basis, "exit");
    assert_eq!(m.status, "passed"); // is_error=false
}

#[tokio::test]
async fn keyword_only_command_is_test_keyword_tier() {
    let runs = load_runs().await;
    let m = run_for_kind(&runs, "run_smoke_test.sh").expect("keyword tier run");
    assert_eq!(m.detection_basis, "test_keyword");
    assert_eq!(m.command_kind, "test_suite_other");
    assert_eq!(m.status_basis, "exit");
}

#[tokio::test]
async fn dry_run_no_run_is_kept_as_known_tool() {
    // Decision: --no-run is still a guard the agent executed; keep detecting it.
    let runs = load_runs().await;
    let m = run_for_kind(&runs, "--no-run").expect("cargo test --no-run detected");
    assert_eq!(m.command_kind, "test_suite_rust");
    assert_eq!(m.detection_basis, "known_tool");
}

#[tokio::test]
async fn tsc_typecheck_is_not_detected_honest_gap() {
    // `tsc` is NOT on the Tier-1 allowlist and carries no test/spec keyword,
    // so it is intentionally not detected. Promoting tsc to Tier-1 is a future
    // change that needs its own fixture (DEV-S11-03). Document the gap here.
    let runs = load_runs().await;
    assert!(
        run_for_kind(&runs, "tsc").is_none(),
        "tsc type-check is an honest detection gap, not a guard"
    );
}

#[tokio::test]
async fn piped_to_nonpager_yields_unknown_status() {
    // Synthetic piped-to-grep case locking the status_basis=piped → unknown
    // rule at the extractor level. (The real fixture's tsc line is the npx
    // case; this isolates the pipe-masking semantics on a known tool.)
    use std::io::Write;
    use tempfile::NamedTempFile;
    let session = "s_piped_unknown";
    let assistant = serde_json::json!({
        "type":"assistant","sessionId":session,"uuid":"a1","parentUuid":null,
        "timestamp":"2026-05-30T10:00:00Z","cwd":"/tmp","userType":"external",
        "entrypoint":"cli","version":"2.1.146",
        "message":{"role":"assistant","model":"claude-opus-4-8","content":[
            {"type":"tool_use","id":"toolu_p","name":"Bash",
             "input":{"command":"npm test | grep FAIL"}}]}
    });
    let result = serde_json::json!({
        "type":"user","sessionId":session,"uuid":"u1","parentUuid":"a1",
        "timestamp":"2026-05-30T10:00:05Z","cwd":"/tmp","userType":"external",
        "entrypoint":"cli",
        "message":{"role":"user","content":[
            {"tool_use_id":"toolu_p","type":"tool_result","is_error":false,
             "content":"FAIL src/x.test.ts"}]}
    });
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "{assistant}").unwrap();
    writeln!(f, "{result}").unwrap();

    let pool = SqlitePoolOptions::new().max_connections(2)
        .connect("sqlite::memory:").await.unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(&pool, f.path(), &NoopSink).await.unwrap();
    let evs = repo_observed::list_session(&pool, session, 1000).await.unwrap();
    let runs = extract_verification_runs(&evs);
    let m = runs.iter().find(|r| r.command.contains("npm test"))
        .expect("npm test detected even when piped");
    assert_eq!(m.command_kind, "test_suite_js");
    assert_eq!(m.status_basis, "piped");
    assert_eq!(m.status, "unknown", "pipe masks exit → status unknown");
}
```

This test references `VerificationRunRecord` as a public type from `witmcc::ingest::verification_run` — it already is `pub`. The `MatchedSegment`, `split_segments`, `matched_segment`, `classify_segment`, `strip_wrappers` helpers added in Tasks 2–3 are `pub` so cross-crate (integration) tests in this file compile; confirm they were declared `pub` (they were, per Task 2/3 code).

- [ ] **Step 3: Run test to verify it fails first, then passes**

Run: `cargo test --test verification_segment_split 2>&1 | tail -30`
Sequencing note: if you are executing tasks strictly in order, Tasks 2–3 are already implemented by now, so this should be GREEN immediately. To honour TDD red-first, you may instead author this test file *before* Task 3 Step 4 (stash the implementation) — but given the plan's task ordering, the canonical red is the per-task unit tests in Tasks 2–3. Run the full extractor suite to confirm no regression:
`cargo test --test transcript_verification_bash --test verification_bash_allowlist --test verification_segment_split 2>&1 | tail -20`
Expected: all PASS (3-run invariant intact; 16-pattern invariant intact; 5 new invariants green).

- [ ] **Step 4: Commit**

```bash
git add tests/fixtures/transcripts/real/verification_npx_v01.jsonl tests/verification_segment_split.rs
git commit -m "test(verification): real-fixture invariants for segment-split detector"
```

---

## Task 7: Migration-schema test + DTO/route surfacing

**Files:**
- Modify: `tests/migration_verification_run_schema.rs`
- Modify: `src/api/dto.rs`
- Modify: `src/api/routes.rs`
- Test: extend `tests/api_verification_runs.rs` (assert new fields present)

- [ ] **Step 1: Lock the new columns in the migration-schema test**

In `tests/migration_verification_run_schema.rs`, add `"detection_basis"` and `"status_basis"` to the `expected_cols` array (after `"created_at"` is fine — order in the array is not asserted, only membership). They are `NOT NULL` with defaults, so also add them to the `notnull_col` loop list:

```rust
    for notnull_col in &["session_id", "source", "command", "command_kind",
                          "trigger_event_id", "status", "started_at",
                          "raw_event_id", "parser_version",
                          "detection_basis", "status_basis"] {
```

Run: `cargo test --test migration_verification_run_schema 2>&1 | tail -15`
Expected: PASS (columns exist after migration 0015).

- [ ] **Step 2: Add the fields to `VerificationRunDto`**

In `src/api/dto.rs`, in `pub struct VerificationRunDto`, add after `pub status: String,`:

```rust
    pub detection_basis: String,
    pub status_basis: String,
```

- [ ] **Step 3: Populate them in `run_to_dto`**

In `src/api/routes.rs`, in `run_to_dto`, add after `status: r.status,`:

```rust
        detection_basis: r.detection_basis,
        status_basis: r.status_basis,
```

- [ ] **Step 4: Extend the API test to assert the new fields**

In `tests/api_verification_runs.rs`, the `seeded_pool()` builds rows via raw `INSERT` that does NOT list `detection_basis`/`status_basis` — that is fine, they take the column DEFAULT (`known_tool`/`exit`). Add an assertion to `list_endpoint_returns_runs_for_session`, after the `first["schema_version"].is_string()` assertion:

```rust
    assert!(
        first["detection_basis"].is_string(),
        "detection_basis must be present in DTO"
    );
    assert!(
        first["status_basis"].is_string(),
        "status_basis must be present in DTO"
    );
```

- [ ] **Step 5: Run the API + schema tests**

Run: `cargo test --test api_verification_runs --test migration_verification_run_schema 2>&1 | tail -20`
Expected: PASS. Then full suite: `cargo test 2>&1 | tail -20` — no regressions across the whole crate.

- [ ] **Step 6: Commit**

```bash
git add tests/migration_verification_run_schema.rs src/api/dto.rs src/api/routes.rs tests/api_verification_runs.rs
git commit -m "feat(verification): surface detection_basis/status_basis in VerificationRunDto"
```

---

## Task 8: Frontend TS type + contract test

**Files:**
- Modify: `webui/src/api/types.ts`
- Modify: `webui/src/api/__tests__/types.contract.test.ts`

- [ ] **Step 1: Extend the contract test (red first)**

In `webui/src/api/__tests__/types.contract.test.ts`, the `vr: VerificationRunDto` literal (around line 98) is missing the two new fields. Add them after `status: 'passed',`:

```typescript
      detection_basis: 'known_tool',
      status_basis: 'exit',
```

Add an assertion near the end of that `it(...)`, after `expect(vr.covered_diff_hunk_ids).toEqual(['dh1', 'dh2']);`:

```typescript
    expect(vr.detection_basis).toBe('known_tool');
    expect(vr.status_basis).toBe('exit');
```

Run: `npx vitest run src/api/__tests__/types.contract.test.ts 2>&1 | tail -15`
Expected: this test FAILS to type-check at build (`npx tsc -b`) because the literal now has excess properties not on the type. (Vitest with esbuild may pass at runtime; the `tsc -b` in Step 3 is the gate.)

- [ ] **Step 2: Add the fields to the TS type**

In `webui/src/api/types.ts`, in `export type VerificationRunDto`, add after `status: 'passed' | 'failed' | 'skipped' | string;`:

```typescript
  detection_basis: 'known_tool' | 'test_keyword' | string;
  status_basis: 'exit' | 'piped' | string;
```

- [ ] **Step 3: Run vitest + tsc**

Run: `npx vitest run src/api/__tests__/types.contract.test.ts 2>&1 | tail -15` → PASS
Run: `npx tsc -b 2>&1 | tail -15` → clean (no excess-property / missing-property errors)

- [ ] **Step 4: Commit**

```bash
git add webui/src/api/types.ts webui/src/api/__tests__/types.contract.test.ts
git commit -m "feat(verification): frontend VerificationRunDto gains detection_basis/status_basis"
```

---

## Task 9: Re-ingest + endpoint smoke + implementation notes

**Files:**
- Modify: `docs/implementation-notes.html`

- [ ] **Step 1: Rebuild DB and re-ingest** so existing dev data is reclassified under the new detector

Run: `cargo run --bin witmcc -- init-db && cargo run --bin witmcc -- ingest --all 2>&1 | tail -5`
Expected: ingest completes; no errors.

- [ ] **Step 2: Smoke the endpoint against a real session**

Run: `cargo run --bin witmcc -- serve --bind 127.0.0.1 --port 7878 &` then
`sleep 2 && curl -s http://127.0.0.1:7878/v1/sessions/653ea169-1121-442e-9cc9-776471a10895/verification-runs | python3 -m json.tool | head -60`
Expected: a non-empty `data` array where each run carries `detection_basis` (`known_tool` or `test_keyword`) and `status_basis` (`exit` or `piped`). Per the design spec §6.2, `653ea169` should now surface `npx vitest run` / `npm test` runs that previously read 0. Stop the server afterward (`kill %1`). Note in the smoke output whether any `test_keyword`-tier rows appear (these are the Tier-2 promotion-backlog candidates).

> If `653ea169` is not in the local store, pick any session id from `curl -s http://127.0.0.1:7878/v1/sessions | python3 -m json.tool | head` that exercised compound `cd && npx` commands.

- [ ] **Step 3: Document in implementation-notes**

Add a new `§` entry to `docs/implementation-notes.html` (follow the existing section markup style — match the most recent `§36` entry's structure). Record:
- migration 0015 (`detection_basis`, `status_basis`; `ALTER TABLE`, defaults backfill historical rows; `init-db` + re-ingest required to recompute precisely);
- the segment-split + wrapper-strip + Tier-1/Tier-2 design (design spec §6.2);
- the **DEV-S11-03 revision**: closed list → "Tier-1 seed (real-fixture-locked) + Tier-2 fallback"; Tier-1 *additions* still need a real-fixture invariant test; this slice added none;
- the `status_basis` pager rule (`2>&1 | tail`/`head`/`cat`/`less`/`more`/`wc` = output-capture → `exit`; real pipe to non-pager → `piped` → `unknown`) and *why* it preserves the frozen `transcript_verification_bash.rs` 3-run invariant;
- the **dry-run decision** (`cargo test --no-run` / `pytest --collect-only` kept as `known_tool` runs — reversible);
- the **honest gap**: `tsc -b` type-check and non-Bash guards (browser smoke / MCP / sub-agent tests) are not detected; no completeness claim (design spec §8);
- the new `command_kind = "test_suite_other"` for Tier-2 hits and the Tier-2-rows-as-promotion-backlog note.

Commit:

```bash
git add docs/implementation-notes.html
git commit -m "docs(verification): implementation notes for detection rewrite slice"
```

---

## Done criteria

- `cd webui && npx vitest run` is detected as `command_kind=test_suite_js`, `detection_basis=known_tool`, `status_basis=exit` (real-fixture invariant) — the headline §1.2/§7.1 bug is fixed.
- Keyword-only commands (`./run_smoke_test.sh`) classify as `test_keyword` / `test_suite_other`; the non-exec denylist blocks `cat/grep/git/rm/...` even when they contain `test`.
- Piped-to-non-pager commands (`npm test | grep FAIL`) yield `status_basis=piped` + `status=unknown`; trailing pager pipes (`… 2>&1 | tail`) keep `status_basis=exit`.
- The frozen `transcript_verification_bash.rs` (3 runs, all passed) and `verification_bash_allowlist.rs` (16-pattern count) invariants stay green — the rewrite changes *how* matching happens, not the Tier-1 seed.
- `detection_basis`/`status_basis` round-trip DB → DTO → TS type; migration 0015 applies cleanly.
- `cargo test` + `npx vitest run` + `npx tsc -b` all clean; no regressions.
- DEV-S11-03 revision + the dry-run/`tsc` honest-gap decisions are recorded in implementation-notes. This unblocks the design's Q4 surface; the `검증 도구N·키워드M` KpiStrip card consumes these fields in a later frontend slice.
```