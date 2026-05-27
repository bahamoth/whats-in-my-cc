# Slice-12 Design — Episode Segmentation

**Date:** 2026-05-27
**Branch (to be cut):** `slice12-episode-segmentation` off slice-11 merge.
**Goal:** Assign a `phase` label to every contiguous run of events within a session and persist one `Episode` row per phase span. Expose via Pull API. The label set follows `docs/03_data_model_spec.html` §6: `intake | exploration | diagnosis | action | verification | repair | drift`.

This slice closes the second half of AC-3 (episode link in the lineage chain) and is a hard precondition for slice-14 (`missing_verification` rule reasons over "did an action episode have a following verification episode").

---

## 1. Motivation

Findings like `missing_verification`, `final_state_mismatch`, and `risky_action` all need to scope their decision to a contiguous unit of work, not the whole session. Without an `Episode` primitive, every finding rule would re-invent its own windowing — leading to inconsistent definitions of "the window in which verification should have happened".

The data model spec already names episode `phase` values and a `classification_basis` array (the observed signals that justify the label). This slice's task is to make that classification deterministic, regression-locked against real transcripts, and queryable.

---

## 2. Scope

### In scope

- New table `episode` (migration `0006_episode.sql`).
- New extractor `src/insight/episode.rs` implementing a state machine that converts the ordered `ObservedEvent` stream of a session into a `Vec<EpisodeRecord>`.
- Phase classifier as a pure function `classify(window: &EventWindow) -> Phase` where `EventWindow` is a fixed-size lookahead/lookback slice over `(actor, kind, subkind, tool_name, is_error)` tuples.
- Frozen golden output: `tests/fixtures/episode_gold/<transcript_id>.json` containing the expected episode list for each real transcript. Updating the golden requires a deviation note in the commit body.
- Pull API endpoint `GET /v1/sessions/:id/episodes`.
- No graph wiring in this slice. Episodes are a **side-table**, not graph nodes. Adding them as graph nodes would force a node-shape decision UX redesign should make.

### Out of scope

- Cross-session pattern detection over episodes.
- Episode summarisation via LLM (the `summary` field stays empty in this slice; the eventual UX redesign or slice-16's L2 judge may fill it).
- Drift / stall detection beyond the basic "many consecutive exploration events with no progress" rule.

---

## 3. Phase taxonomy + classification rules

| Phase | Observed signal (deterministic) | Boundary trigger |
|---|---|---|
| `intake` | `user_message` from the user actor whose content is a fresh request (not a follow-up to a prior assistant response). | Session start, or any `user_message` whose content is not a continuation. |
| `exploration` | Reads / searches / lists: `tool_call` with `tool_name ∈ {Read, Grep, Glob, LS, WebFetch, WebSearch}` AND no mutation in the look-ahead window. | First such tool_call after intake / repair. |
| `diagnosis` | Read or browse triggered by an error: a previous `tool_result.is_error == true` exists in the same intake window. | First exploration after observed error. |
| `action` | Mutation: `tool_call` with `tool_name ∈ {Edit, Write, MultiEdit, Bash}` (Bash only when not on the verification allowlist). | First such call. |
| `verification` | A `VerificationRun` row exists in the same window. | Onset of the verification_run. |
| `repair` | Action episode immediately after a failed verification or failed tool_result. | First action after observed failure. |
| `drift` | ≥ N consecutive exploration events (default N = 8) with no action and no intent-change. Configurable in `episode_classifier.rs`. | Window of repetition. |

Rules are encoded as a state machine in `src/insight/episode.rs`. The state machine reads ordered events left-to-right; on each event it consults the current state + a small lookahead (next 3 events) to decide whether to emit a boundary.

### Why a state machine (not a classifier-per-event)

Classifying each event independently would mis-label transitions. A `Read` on its own is exploration; the same `Read` immediately following a failed `Bash` is diagnosis. A state machine carries that history explicitly. The state itself is a struct:

```rust
struct ClassifierState {
    current_phase: Phase,
    last_error_at: Option<DateTime<Utc>>,
    last_verification_at: Option<DateTime<Utc>>,
    exploration_streak: usize,    // for drift detection
    intake_window_start: DateTime<Utc>,
}
```

### Lookahead

Three events ahead. This buys the classifier the ability to recognise patterns like "Read → Read → Edit" as a single `action` episode rather than a 2-step exploration plus a 1-step action. The lookahead size is a constant; bumping it requires a `_v2` bump on the classifier.

---

## 4. Schema

```sql
-- migrations/20260528120000_0006_episode.sql
CREATE TABLE IF NOT EXISTS episode (
    episode_id              TEXT PRIMARY KEY,           -- "ep_" + sha256(session_id||phase||start_event_id||end_event_id)
    schema_version          TEXT NOT NULL DEFAULT 'episode.v1',
    session_id              TEXT NOT NULL,
    phase                   TEXT NOT NULL,              -- intake | exploration | diagnosis | action | verification | repair | drift
    start_event_id          TEXT NOT NULL,
    end_event_id            TEXT NOT NULL,
    started_at              TEXT NOT NULL,              -- observed_at of start_event
    ended_at                TEXT NOT NULL,              -- observed_at of end_event
    evidence_node_ids       TEXT NOT NULL,              -- JSON array of node_ids that justify the phase
    classification_basis    TEXT NOT NULL,              -- JSON array of "{rule_id}@{version}" strings
    confidence              REAL NOT NULL,              -- 0.0 .. 1.0
    summary                 TEXT,                       -- nullable; LLM may fill later
    classifier_version      TEXT NOT NULL,              -- "episode_classifier@v1"
    created_at              TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_episode_session_started
    ON episode(session_id, started_at);

CREATE INDEX IF NOT EXISTS idx_episode_session_phase
    ON episode(session_id, phase);
```

`classification_basis` is the spec's `["read/search actions", "error output referenced", "no mutation"]`-style array. Each entry is a versioned rule_id like `"phase_diagnosis_after_error@v1"`. The list is canonical:

```rust
pub const RULE_IDS: &[&str] = &[
    "phase_intake_fresh_user_message@v1",
    "phase_exploration_read_only_window@v1",
    "phase_diagnosis_after_error@v1",
    "phase_action_first_mutation@v1",
    "phase_verification_run_window@v1",
    "phase_repair_after_failed_verification@v1",
    "phase_drift_long_exploration@v1",
];
```

A test asserts the list contents (count + values).

---

## 5. Real-data invariant — golden output

For each real transcript in `~/.claude/projects/`, we freeze the expected episode list:

```
tests/fixtures/episode_gold/
  ├── aac68973.json
  ├── <transcript_id_2>.json
  └── ... (one per transcript)
```

Schema:

```json
{
  "session_id": "aac68973",
  "expected_episodes": [
    {
      "phase": "intake",
      "start_event_offset_in_session": 0,
      "end_event_offset_in_session": 0,
      "classification_basis": ["phase_intake_fresh_user_message@v1"]
    },
    {
      "phase": "exploration",
      "start_event_offset_in_session": 1,
      "end_event_offset_in_session": 7,
      "classification_basis": ["phase_exploration_read_only_window@v1"]
    },
    ...
  ]
}
```

We use **event offsets within the session** rather than `event_id`s so the golden file is stable across re-ingests that may produce different `event_id`s. The classifier is offset-stable by design (it does not consult event IDs).

The test `tests/episode_gold.rs` runs the classifier against each transcript fixture and asserts exact match. Updating the golden requires:

1. Justification in the commit body (e.g., `(episode gold update: aac68973 — boundary at offset 42 moved to 41 because lookahead now sees a deferred Edit)`).
2. A new section in implementation-notes if the rule version changed.

### Initial golden bootstrapping

Slice-12's red-locking commit ships an **empty** golden file with `expected_episodes: []`. The first green commit runs the classifier against the real transcript, captures the output, and stores it as the golden. This is the single time a golden is "auto-populated" — every subsequent change is explicit.

---

## 6. Pull API surface

### `GET /v1/sessions/:id/episodes`

```json
{
  "data": [
    {
      "episode_id": "ep_…",
      "schema_version": "episode.v1",
      "session_id": "sess_…",
      "phase": "exploration",
      "start_event_id": "ev_…",
      "end_event_id": "ev_…",
      "started_at": "…",
      "ended_at": "…",
      "evidence_node_ids": ["node_…", "node_…"],
      "classification_basis": ["phase_exploration_read_only_window@v1"],
      "confidence": 0.85,
      "summary": null
    }
  ]
}
```

### `GET /v1/episodes/:id`

Single-episode detail (same shape, single object in `data`).

---

## 7. Confidence policy

| Phase | Confidence floor | Notes |
|---|---|---|
| `intake` | 1.0 | Trivial — user actor + message. |
| `exploration` | 0.85 | Risk: a tool we classified as read-only may have mutated state. |
| `diagnosis` | 0.8 | Same risk + reliance on "error in window" detection. |
| `action` | 0.95 | Edit/Write/Bash detection is precise. |
| `verification` | 0.95 | Anchored on slice-11's `VerificationRun`. |
| `repair` | 0.7 | Higher uncertainty because we're inferring intent. |
| `drift` | 0.6 | Most heuristic phase. |

Confidence is **per-episode**, not per-event. Stored in the `confidence` column.

---

## 8. Failure modes

| Failure | Behaviour |
|---|---|
| Session has zero events | Emit zero episodes. Endpoint returns empty array. |
| Session has only one event of an indeterminate kind (e.g., `thinking`) | Emit a single `exploration` episode with confidence 0.5 + a `classification_basis: ["fallback_single_event@v1"]`. Tested separately. |
| Two consecutive events in different epochs (clock skew) | Episode boundary derives from event order, not timestamps. Reverse-clock events stay in their original order. |
| Re-running classifier produces different boundaries | Test `tests/episode_determinism.rs` asserts identical output from two runs over the same input. |
| Classifier panics | Wrapped in `catch_unwind`; failure logged + emit zero episodes + write a single audit row `episode_classifier_error`. Audit row visible via `/v1/health`. |

---

## 9. Deviations index (slice-12)

| ID | Description |
|---|---|
| DEV-S12-01 | Episodes are a **side-table**, not graph nodes. Adding them as graph nodes is deferred to the UX redesign epic. |
| DEV-S12-02 | The classifier is a **state machine with 3-event lookahead**. Larger lookahead would be more accurate but pushes complexity into the rule set; bigger lookaheads are deferred. |
| DEV-S12-03 | Drift detection threshold `N=8 consecutive explorations` is a guess. Real-data tuning happens after slice-16 ships findings that surface drift candidates. |
| DEV-S12-04 | Empty golden file is committed first, then auto-populated on the first green run. This is the only "auto-populate" pattern in the project; subsequent changes are explicit. |
| DEV-S12-05 | `summary` field stays NULL throughout slice-12. Filling it via LLM is post-MVP. |
| DEV-S12-06 | Phase `drift` is computed but never used as a finding category in MVP. The label exists for the eventual UX. |

---

## 10. Commit plan

See `2026-05-27-witmcc-slice12-episode-segmentation.md`. Five commits:

1. `test(slice-12): red-locking tests for episode classifier + schema + API`
2. `feat(db): 0006_episode migration + repo_episode`
3. `feat(insight): episode classifier state machine + rule registry`
4. `feat(insight): populate episode_gold from real transcript run`  (golden bootstrap commit)
5. `feat(api): /v1/sessions/:id/episodes + /v1/episodes/:id`
