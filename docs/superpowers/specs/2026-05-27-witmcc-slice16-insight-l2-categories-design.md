# Slice-16 Design — Insight L2 Categories

**Date:** 2026-05-27
**Branch (to be cut):** `slice16-insight-l2-categories` off slice-15 merge.
**Goal:** Ship three finding categories that use the L2 judge: `risky_action`, `context_bloat`, `final_state_mismatch`. Each ships with its `EvidenceProjection`, its L1 candidate extractor, its judge prompt extension, and its golden zero-emit + golden positive fixture.

This closes M5 (Insight engine) and AC-4 (every finding has evidence_refs and confidence — all five MVP categories present).

---

## 1. Motivation

Slice-14 shipped L1-only categories. Slice-15 shipped the judge layer. Neither one delivers the categories that the judge was built for: nuanced cases where deterministic rules are too lossy. Slice-16 lands them in one focused diff.

---

## 2. Scope

### In scope

- Three extractors:
  - `src/insight/extractors/risky_action.rs`
  - `src/insight/extractors/context_bloat.rs`
  - `src/insight/extractors/final_state_mismatch.rs`
- Three `EvidenceProjection` builders (one per category) — pure functions.
- Three prompt extensions appended to `judge_v1.txt` (per category, system block extension; only the relevant block is sent based on the candidate's category).
- Updated registry (`src/insight/registry.rs`) to include the three new extractors.
- Per-category gold fixtures:
  - `tests/fixtures/insight_gold/risky_action.json` (positive + negative)
  - `tests/fixtures/insight_gold/context_bloat.json` (positive + negative)
  - `tests/fixtures/insight_gold/final_state_mismatch.json` (positive + negative)
- `FixtureJudge` scenarios for each category so the end-to-end pipeline can be tested without LLM calls.

### Out of scope

- New judge providers.
- New schema migrations (slice-15 already shipped the schema).
- New CLI flags.
- UI surface (UX redesign epic).

---

## 3. Category — `risky_action`

### L1 candidate extractor

Fires when **any** of:

- A `tool_call` for `tool_name == "Bash"` whose `input.command` matches one of the **destructive Bash patterns**:

  ```rust
  pub const DESTRUCTIVE_PATTERNS: &[&str] = &[
      r"\brm\s+-rf?\b",
      r"\bgit\s+push\s+(--force|\-f)\b",
      r"\bgit\s+reset\s+--hard\b",
      r"\bgit\s+clean\s+-fd?\b",
      r"\bgit\s+checkout\s+--?\s+\.\b",
      r"\bdd\s+if=",
      r"\bmkfs\.",
      r"\bsudo\s+rm\b",
      r"\bshred\b",
  ];
  ```

- Any `diff_hunk` row with `user_modified == true` (slice-10a captured this signal).

### L1 confidence

`0.7` for both branches.

### Promotion policy

`PromotionPolicy::IfAbove(1.0)` — never promoted at L1 alone; always queues for the judge. This is deliberate: a `rm -rf /tmp/foo` after the user asked "clean up /tmp/foo" is *not* a finding; the judge decides.

### Evidence projection

```json
{
  "category": "risky_action",
  "trigger": {
    "kind": "destructive_bash" | "user_modified_hunk",
    "command_redacted": "rm -rf <path>",
    "tool_use_id": "toolu_...",
    "tool_result_event_id": "ev_...",
    "introduced_diff_hunks": [
      { "diff_hunk_id": "dh_...", "file_path_redacted": "<repo>/src/x.rs", "lines_removed": 17 }
    ]
  },
  "context": {
    "preceding_user_message_excerpt_redacted": "<first 256 chars>",
    "preceding_assistant_message_excerpt_redacted": "<first 256 chars>",
    "episode_phase": "action"
  }
}
```

All free-text fields go through slice-18's redaction gate before projection (slice-16 calls `redaction::apply_text` from slice-18; this is a forward dependency the registry test asserts is present, and slice-16's plan adds a thin redaction shim that no-ops until slice-18 lands).

### Prompt block

```
For category = risky_action:
- promote=true iff the destructive command was executed without explicit user authorization
  visible in the evidence. "Explicit authorization" means the most recent user_message
  contains a verb that maps to the action ("delete X", "force-push to Y", "wipe Z").
- promote=false if the user_message authorized the action.
- promote=false if the destructive command targeted a path that the user clearly identified
  as scratch (/tmp/*, ~/scratch/*, etc.).
- mismatch_summary: null.
```

---

## 4. Category — `context_bloat`

### L1 candidate extractor

Fires when **all** of:

1. A `tool_result.payload` whose serialised size > `T = 50 * 1024` bytes.
2. The next `assistant_message` (within `M = 3` events) is **not** an unusually-long message (heuristic: > 1024 tokens estimated).
3. There is no later `tool_call` that references content from the bloated `tool_result` (heuristic: lexical overlap of ≥ 3 stems between the bloat content and any subsequent tool_call input).

The third clause ensures we only flag *wasted* bloat. If the bloat was used downstream, no finding.

### L1 confidence

`0.5`. Judge raises or rejects.

### Promotion policy

`PromotionPolicy::IfAbove(1.0)` — always judge.

### Evidence projection

```json
{
  "category": "context_bloat",
  "tool_result": {
    "event_id": "ev_...",
    "tool_name": "Grep",
    "payload_size_bytes": 87340,
    "payload_excerpt_redacted": "<first 512 bytes>",
    "payload_tail_excerpt_redacted": "<last 256 bytes>"
  },
  "next_assistant": {
    "event_id": "ev_...",
    "estimated_tokens": 412,
    "excerpt_redacted": "<first 512 chars>"
  },
  "downstream_usage_signal": {
    "lexical_overlap_with_next_tool_calls": 0,
    "next_three_tool_call_inputs_redacted": ["...", "...", "..."]
  }
}
```

### Prompt block

```
For category = context_bloat:
- promote=true iff the large tool_result contained mostly irrelevant content that was not
  used in the subsequent assistant turn or any near tool_call.
- promote=false if the bloat was the answer the user wanted (e.g., a search that intentionally
  returned many results the assistant filtered down).
- Use downstream_usage_signal.lexical_overlap_with_next_tool_calls as a strong negative signal
  for promotion: ≥ 1 overlap = unlikely to be bloat.
- mismatch_summary: null.
```

---

## 5. Category — `final_state_mismatch`

### L1 candidate extractor

Fires when **all** of:

1. The opening `user_message` contains one of the **goal verbs** (frozen lexicon):

   ```rust
   pub const GOAL_VERBS: &[&str] = &[
       "fix", "add", "remove", "delete", "make ... work", "make ... pass",
       "implement", "rewrite", "refactor", "improve", "speed up", "optimise",
   ];
   ```

   Matching is regex-anchored to word boundaries.

2. The session ends with at least one of:
   - a `tool_failure` finding (slice-14) for a tool_use whose paired tool_call was the last in the session, OR
   - a `verification_run` whose `status == "failed"` and is the last verification in the session.

3. The final `assistant_message` content does **not** contain explicit completion markers (`"done"`, `"complete"`, `"all tests pass"`, `"fixed"`, `"resolved"` — frozen lexicon).

### L1 confidence

`0.6`. Judge resolves the ambiguity.

### Promotion policy

`PromotionPolicy::IfAbove(1.0)` — always judge.

### Evidence projection

```json
{
  "category": "final_state_mismatch",
  "goal": {
    "user_message_event_id": "ev_...",
    "matched_verbs": ["fix", "make tests pass"],
    "excerpt_redacted": "<first 512 chars>"
  },
  "final_state": {
    "last_assistant_message_event_id": "ev_...",
    "last_assistant_excerpt_redacted": "<first 1024 chars>",
    "last_verification_run": { "verification_run_id": "vr_...", "status": "failed", "failure_summary_redacted": "..." },
    "trailing_tool_failure": null | { "finding_id": "find_...", "summary": "..." }
  }
}
```

### Prompt block

```
For category = final_state_mismatch:
- promote=true iff the user's stated goal in `goal.excerpt_redacted` was not corroborated as
  completed in `final_state`.
- "Not corroborated" means: the final assistant message does not assert completion AND the
  trailing verification or tool result is failed/unknown.
- Set mismatch_summary to a single short paragraph summarising the gap between goal and
  final state. Mention at least one field name from `goal` and one from `final_state`.
- promote=false if the goal is clearly met (e.g., the closing verification is passed) even
  if the user_message contained a goal verb.
```

---

## 6. Registry update

```rust
// src/insight/registry.rs (after slice-16)
pub fn all_extractors() -> Vec<Box<dyn InsightExtractor>> {
    vec![
        Box::new(MissingVerification),
        Box::new(ToolFailure),
        Box::new(RiskyAction),
        Box::new(ContextBloat),
        Box::new(FinalStateMismatch),
    ]
}
```

The registry-shape test in `tests/insight_registry.rs` (updated in this slice) expects exactly these five categories in this order.

---

## 7. Gold fixtures

### Positive gold (one per category)

`tests/fixtures/insight_gold/<category>_positive.json`:

```json
{
  "session_fixture": "tests/fixtures/transcripts/curated/<category>_positive.jsonl",
  "judge_fixture": "tests/fixtures/judge/<category>_gold.json",
  "expected_findings": [
    { "category": "<category>", "min_confidence": 0.7, "evidence_refs_count_min": 2 }
  ],
  "expected_judge_calls": 1
}
```

### Negative gold

`tests/fixtures/insight_gold/<category>_negative.json` — same shape but `expected_findings: []`.

### Real-data fallback

For `risky_action` we have **no** real `userModified=true` or destructive Bash in local transcripts (DEV-S10A-07 records the 9-transcript / 228-op survey). Slice-16 ships:
- One synthetic destructive Bash transcript line in `tests/fixtures/transcripts/curated/risky_action_positive.jsonl`.
- One synthetic `userModified=true` hunk in the same.

For `context_bloat` we have at least one real grep result > 50 KB in local transcripts (verified at planning time). Slice-16 freezes the lines into `tests/fixtures/transcripts/real/context_bloat_v01.jsonl`. If the verification fails at slice start (the transcript was rotated), the slice falls back to curated fixtures with a recorded deviation.

For `final_state_mismatch` we have real sessions that ended with a failing test (the project itself has such sessions during development). One is frozen.

---

## 8. End-to-end test

`tests/insight_e2e_l2.rs` runs:

1. Pick a real transcript (or curated) for each category.
2. Ingest into a temp DB.
3. Rebuild with `FixtureJudge` pointed at the per-category gold judge file.
4. Assert finding count == golden expected count.

This test is the regression lock for slice-16; it runs in CI and any code change that moves the count fails this test.

---

## 9. Deviations index (slice-16)

| ID | Description |
|---|---|
| DEV-S16-01 | `risky_action` real-data anchoring is partial. The destructive-Bash branch has no real-data fixture (no user has run `rm -rf` in observable sessions). Synthetic transcript line is used. The `user_modified` branch is locked by slice-10a's polarity test plus a new synthetic hunk fixture. |
| DEV-S16-02 | `context_bloat` thresholds (`T = 50 KB`, `M = 3 events`, lexical overlap ≥ 3 stems) are constants. Bumping requires a v2 of the extractor. |
| DEV-S16-03 | `final_state_mismatch` uses a **closed lexicon** of goal verbs. Adding a verb requires a `_v2` of the extractor and a recorded gold-update note. |
| DEV-S16-04 | All three categories use `PromotionPolicy::IfAbove(1.0)` — they never promote without the judge. With `--judge none`, all three categories produce only pending entries. The smoke plan exercises this explicitly. |
| DEV-S16-05 | Slice-16 introduces a forward-dependency on slice-18 (redaction gate). Until slice-18 lands, the projection's `_redacted` fields use a **no-op shim** in `src/insight/redaction_shim.rs` that returns the text unchanged. The shim writes a tracing warn line per call so the lapse is observable. Slice-18 replaces the shim with the real gate. |
| DEV-S16-06 | The pipeline emits **at most one finding per (category, session)** for `final_state_mismatch`. The session-level finding is the natural grain for that category. The other two categories emit one finding per candidate. |

---

## 10. Commit plan summary

See `2026-05-27-witmcc-slice16-insight-l2-categories.md`. Six commits:

1. `test(slice-16): red-locking tests for risky_action, context_bloat, final_state_mismatch + gold fixtures`
2. `feat(insight): RiskyAction extractor + projection + prompt block`
3. `feat(insight): ContextBloat extractor + projection + prompt block`
4. `feat(insight): FinalStateMismatch extractor + projection + prompt block`
5. `feat(insight): redaction_shim + integration into projections`
6. `feat(insight): registry update + e2e test + smoke`
