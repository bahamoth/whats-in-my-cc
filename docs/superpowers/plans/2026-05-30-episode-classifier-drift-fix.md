# Episode Classifier Drift Bug Fix — Implementation Plan (Slice 4 of insight-surface-redesign)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the episode classifier so the `Drift` transition (`src/insight/episode/classifier.rs:213-233`) stops double-classifying events. Today, when `exploration_streak >= DRIFT_THRESHOLD` (8) the classifier emits an `Exploration` episode that **ends at `events[i]`** and then starts a `Drift` episode whose `phase_start_idx = i` — so `events[i]` lands in **both** episodes. Across a long real session (`653ea169`) this produced 513 shared `event_id`s, 39 zero-duration episodes, negative gaps, empty `evidence_node_ids`, and a corrupted session duration (the "289h" mis-derivation in spec §3 Q3). This slice makes the drift boundary obey the same off-by-one convention as the normal boundary (prev episode ends at `i-1`, new starts at `i`), and populates `evidence_node_ids` with the spanned `event_id`s so every episode (drift included) carries non-empty evidence.

**Architecture:** `classify_session` is a pure, deterministic left-to-right state machine (no I/O, no globals — locked by `tests/episode_determinism.rs`). `episode_id` is `"ep_" + hex(sha256(session_id||phase||start_event_id||end_event_id))[..24]` (`make_episode_id`, classifier.rs:78). The graph builder (`src/graph/build.rs:50-82`) wraps `classify_session` in `catch_unwind`, converts each `EpisodeRecord` → `repo_episode::EpisodeRow` (serializing `evidence_node_ids` via `serde_json::to_string`, build.rs:65), and `INSERT OR REPLACE`s into the `episode` table (migration `0006`). **No schema change** — `evidence_node_ids` is already a `TEXT NOT NULL` JSON-array column. The fix is entirely inside `classifier.rs`; the `EpisodeRecord` shape, `Phase` enum, `RULE_IDS`, and `episode_id` derivation stay byte-for-byte stable so the existing golden (`tests/fixtures/episode_gold/{aac68973,ed82aee9}.json`) and determinism invariants keep passing.

**Key invariants to lock (TDD red-first):**
1. **No shared `event_id` across episodes** — events partition into episodes; each `event_id` index range `[start..end]` is disjoint from every other episode's range.
2. **No zero-duration / negative-gap rows** — `ended_at >= started_at` for every episode, and consecutive episodes are time-monotonic (`ep[n].started_at >= ep[n-1].ended_at` is not required because spans are inclusive ranges that touch at boundaries; instead assert `ep[n].started_at >= ep[n-1].started_at`).
3. **Drift episode has non-empty `evidence_node_ids`** — and so does every other episode.
4. **Determinism + `episode_id` stability** — two runs produce identical `episode_id`s; the two frozen goldens still match their recorded phase sequences and counts.

**Tech Stack:** Rust (chrono, sha2/hex, serde_json). Tests: `cargo test`, with a synthetic 8+-read-only-op stream (deterministic, in-test `ev()` helper mirroring the existing classifier test module) plus the real fixtures `tests/fixtures/transcripts/real/{verification_v01,structured_patch_v01}.jsonl`.

**Out of scope for this plan (later/other slices):** the `drift*` phase-bar UI badge "보정 후 신뢰" (spec §5) — frontend lands alongside its data later; verification-detection rewrite (§6.2, slice 2); cost (§6.5); cross-session baseline (proposal A). This slice delivers ONLY the classifier correctness fix + invariant tests + evidence population — testable on its own.

---

## File structure

| File | Responsibility | Create/Modify |
|---|---|---|
| `src/insight/episode/classifier.rs` | fix drift boundary off-by-one; thread `evidence_node_ids` into `emit`; final-episode evidence | Modify |
| `tests/episode_drift_no_overlap.rs` | synthetic 8+-read-only-op stream → non-overlapping, non-zero-duration, evidenced drift | Create |
| `tests/episode_no_overlap_real.rs` | real-fixture invariant: no shared event_ids / monotonic / evidenced across both frozen transcripts | Create |
| `docs/implementation-notes.html` | record the drift-boundary fix + evidence_node_ids semantics | Modify |

No migration, no DTO, no frontend change. The `episode` table column `evidence_node_ids` already exists (`migrations/20260528120000_0006_episode.sql:17`).

---

## Task 1: Lock the bug with a failing synthetic test (RED)

**Files:**
- Create: `tests/episode_drift_no_overlap.rs`

The classifier test module (`src/insight/episode/classifier.rs:331-344`) and `tests/episode_classifier_basic.rs:6-20` both use an `ev(i, actor, kind, tool)` helper that stamps `observed_at = Utc.timestamp_opt(1_700_000_000 + i, 0)` and `event_id = format!("ev_{i:03}")`. We reuse that exact pattern so timestamps are strictly increasing and event_ids are unique. A run-only stream of 9 `Read` tool_calls (after one `UserMessage`) drives `exploration_streak` past `DRIFT_THRESHOLD = 8`, triggering the buggy drift branch.

- [ ] **Step 1: Write the failing test**

Create `tests/episode_drift_no_overlap.rs`:

```rust
//! insight-redesign #4 — episode classifier drift bug fix (spec §6.4).
//!
//! A stream of >= DRIFT_THRESHOLD (8) consecutive read-only tool_calls must
//! trigger a Drift episode WITHOUT double-classifying the boundary event.
//! Pre-fix, the Exploration episode ended at events[i] and the Drift episode
//! started at events[i] — the same event landed in two episodes (513 shared
//! event_ids in real session 653ea169), producing zero-duration / negative-gap
//! rows and empty evidence.

use chrono::{TimeZone, Utc};
use std::collections::HashSet;
use witmcc::insight::episode::classifier::classify_session;
use witmcc::insight::episode::types::Phase;
use witmcc::model::observed::{Actor, EventKind, ObservedEvent};

/// Mirror of the helper in classifier.rs tests + episode_classifier_basic.rs:
/// strictly-increasing observed_at, unique event_id "ev_{i:03}".
fn ev(i: usize, actor: Actor, kind: EventKind, tool: Option<&str>) -> ObservedEvent {
    ObservedEvent {
        event_id: format!("ev_{i:03}"),
        raw_event_id: format!("raw_{i:03}"),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_drift".into(),
        observed_at: Utc.timestamp_opt(1_700_000_000 + i as i64, 0).unwrap(),
        actor,
        kind,
        tool_name: tool.map(String::from),
        parser_version: "test".into(),
        ..Default::default()
    }
}

/// Build: 1 user message + N consecutive read-only Read tool_calls.
/// With N >= DRIFT_THRESHOLD the classifier must emit a Drift episode.
fn read_only_stream(n_reads: usize) -> Vec<ObservedEvent> {
    let mut evs = vec![ev(0, Actor::User, EventKind::UserMessage, None)];
    for k in 1..=n_reads {
        evs.push(ev(k, Actor::Assistant, EventKind::ToolCall, Some("Read")));
    }
    evs
}

#[test]
fn drift_triggers_after_threshold() {
    // 9 reads (> DRIFT_THRESHOLD=8) must produce at least one Drift episode.
    let evs = read_only_stream(9);
    let eps = classify_session("sess_drift", &evs, &[]);
    assert!(
        eps.iter().any(|e| e.phase == Phase::Drift),
        "expected a Drift episode; got {:?}",
        eps.iter().map(|e| e.phase).collect::<Vec<_>>()
    );
}

#[test]
fn episodes_do_not_share_event_ids() {
    // Each event_id must belong to exactly one episode. We reconstruct each
    // episode's covered index range from start_event_id..=end_event_id and
    // assert the ranges are pairwise disjoint.
    let evs = read_only_stream(9);
    let eps = classify_session("sess_drift", &evs, &[]);

    // Map event_id -> stream index.
    let idx: std::collections::HashMap<&str, usize> = evs
        .iter()
        .enumerate()
        .map(|(i, e)| (e.event_id.as_str(), i))
        .collect();

    let mut seen: HashSet<usize> = HashSet::new();
    for e in &eps {
        let s = idx[e.start_event_id.as_str()];
        let t = idx[e.end_event_id.as_str()];
        assert!(s <= t, "episode start index {s} must be <= end index {t}");
        for i in s..=t {
            assert!(
                seen.insert(i),
                "event index {i} ({}) appears in more than one episode; \
                 episodes={:?}",
                evs[i].event_id,
                eps.iter()
                    .map(|x| (x.phase, x.start_event_id.clone(), x.end_event_id.clone()))
                    .collect::<Vec<_>>()
            );
        }
    }
    // Every event must be covered exactly once.
    assert_eq!(seen.len(), evs.len(), "every event must belong to one episode");
}

#[test]
fn no_zero_or_negative_duration_and_monotonic() {
    let evs = read_only_stream(9);
    let eps = classify_session("sess_drift", &evs, &[]);
    let mut prev_start: Option<chrono::DateTime<Utc>> = None;
    for e in &eps {
        assert!(
            e.ended_at >= e.started_at,
            "episode {:?} has ended_at {} < started_at {}",
            e.phase,
            e.ended_at,
            e.started_at
        );
        if let Some(p) = prev_start {
            assert!(
                e.started_at >= p,
                "episodes must be time-monotonic by started_at"
            );
        }
        prev_start = Some(e.started_at);
    }
}

#[test]
fn every_episode_has_non_empty_evidence() {
    let evs = read_only_stream(9);
    let eps = classify_session("sess_drift", &evs, &[]);
    for e in &eps {
        assert!(
            !e.evidence_node_ids.is_empty(),
            "episode {:?} ({}..{}) has empty evidence_node_ids",
            e.phase,
            e.start_event_id,
            e.end_event_id
        );
    }
    // The drift episode specifically must carry evidence (spec §6.4 ask).
    let drift = eps
        .iter()
        .find(|e| e.phase == Phase::Drift)
        .expect("a drift episode");
    assert!(
        !drift.evidence_node_ids.is_empty(),
        "drift episode must have non-empty evidence"
    );
}
```

- [ ] **Step 2: Run the test to confirm RED**

Run: `cargo test --test episode_drift_no_overlap 2>&1 | tail -30`
Expected failures (pre-fix behaviour):
- `episodes_do_not_share_event_ids` FAILS — the boundary event index appears in both the Exploration and Drift episode (the core bug).
- `every_episode_has_non_empty_evidence` FAILS — `evidence_node_ids` is hard-coded `vec![]` in `emit` (classifier.rs:133).
- `drift_triggers_after_threshold` likely PASSES (drift is produced today). `no_zero_or_negative_duration_and_monotonic` may PASS for this short synthetic stream but is locked for the long-session regression.

Confirm at least the two failing assertions fire before proceeding. Do NOT commit a red test on its own — the GREEN fix lands in Task 2 and they commit together (this repo's hook forbids test-trailing-implementation; here test+fix are one logical change committed in Task 2).

---

## Task 2: Fix the drift boundary + populate evidence (GREEN)

**Files:**
- Modify: `src/insight/episode/classifier.rs`

Two coupled changes:

**(A) Fix the off-by-one in the drift branch.** The normal boundary (classifier.rs:187-202) emits `[phase_start_idx .. i-1]` and starts the new phase at `i` — events never overlap there. The drift branch (classifier.rs:216-230) instead emits the Exploration span ending at `ev` (= `events[i]`) **and** sets `phase_start_idx = i`, so `events[i]` is in both. Fix: end the Exploration span at `i-1` and start `Drift` at `i`, matching the normal-boundary convention.

**(B) Populate `evidence_node_ids`.** `emit` (classifier.rs:115-139) hard-codes `evidence_node_ids: vec![]`. Thread the spanned `event_id`s through. The classifier owns `event_id`s directly (it has no access to derived graph `node_id`s, and reaching into the graph would break determinism + the pure-function contract). Per CLAUDE.md "Evidence-linked" + the `EpisodeRecord` doc, the episode's evidence is the events it spans; we store `event_id`s. This is consistent with `start_event_id`/`end_event_id` already being event_ids, and the graph builder serializes the vec verbatim (build.rs:65) — no downstream change needed.

- [ ] **Step 1: Add an evidence param to `emit`**

Replace the whole `emit` function (classifier.rs:114-139):

```rust
/// Emit a completed episode span covering events `[start_idx ..= end_idx]`.
/// `evidence_node_ids` is populated with the spanned event_ids — the episode's
/// evidence is the events it covers (the classifier owns event_ids; it has no
/// access to derived graph node_ids, and reaching into the graph would break
/// the pure-function determinism contract). The graph builder serializes this
/// vec verbatim into the `episode.evidence_node_ids` JSON column (build.rs).
fn emit(
    session_id: &str,
    phase: Phase,
    events: &[ObservedEvent],
    start_idx: usize,
    end_idx: usize,
    basis: Vec<&'static str>,
    confidence: f32,
) -> EpisodeRecord {
    let start = &events[start_idx];
    let end = &events[end_idx];
    let episode_id = make_episode_id(session_id, phase, &start.event_id, &end.event_id);
    let evidence_node_ids: Vec<String> = events[start_idx..=end_idx]
        .iter()
        .map(|e| e.event_id.clone())
        .collect();
    EpisodeRecord {
        episode_id,
        schema_version: "episode.v1".into(),
        session_id: session_id.to_string(),
        phase,
        start_event_id: start.event_id.clone(),
        end_event_id: end.event_id.clone(),
        started_at: start.observed_at,
        ended_at: end.observed_at,
        evidence_node_ids,
        classification_basis: basis,
        confidence,
        summary: None,
        classifier_version: CLASSIFIER_VERSION.into(),
    }
}
```

Note: `episode_id` is still `sha256(session_id||phase||start_event_id||end_event_id)` — unchanged inputs ⇒ **identical** ids ⇒ determinism + goldens hold. The signature changes from `(start: &ObservedEvent, end: &ObservedEvent)` to index-based `(events, start_idx, end_idx)` so `emit` can build the evidence slice; update all three call sites below.

- [ ] **Step 2: Update the normal-boundary call site**

In `classify_session`, replace the normal-boundary emit (classifier.rs:189-197) so it passes indices instead of event refs:

```rust
        if new_phase != st.current_phase || should_force_boundary(ev, st.current_phase) {
            // Emit the current episode [phase_start_idx ..= i-1]. The new phase
            // starts at i — ranges never overlap (prev ends at i-1).
            let (basis, confidence) = phase_basis_confidence(st.current_phase);
            out.push(emit(
                session_id,
                st.current_phase,
                events,
                st.phase_start_idx,
                i - 1,
                basis,
                confidence,
            ));

            // Start new episode at i.
            st.current_phase = new_phase;
            st.phase_start_idx = i;
            st.reset_streak();
        }
```

(Drop the dead `if st.current_phase == Phase::Verification { … }` no-op comment block at classifier.rs:206-210 in the same edit — it is an empty branch.)

- [ ] **Step 3: Fix the drift branch (the core bug)**

Replace the exploration-streak / drift block (classifier.rs:213-233):

```rust
        // Exploration streak tracking for drift.
        if st.current_phase == Phase::Exploration {
            st.exploration_streak += 1;
            if st.exploration_streak >= DRIFT_THRESHOLD {
                // Close the Exploration episode at i-1 (NOT i) and begin Drift
                // at i — same off-by-one as the normal boundary, so events[i]
                // belongs to exactly one episode. Pre-fix this ended at i and
                // started Drift at i, double-classifying events[i] (spec §6.4:
                // 513 shared event_ids / zero-duration rows in 653ea169).
                let (basis, confidence) = phase_basis_confidence(Phase::Exploration);
                out.push(emit(
                    session_id,
                    Phase::Exploration,
                    events,
                    st.phase_start_idx,
                    i - 1,
                    basis,
                    confidence,
                ));
                st.current_phase = Phase::Drift;
                st.phase_start_idx = i;
                st.reset_streak();
            }
        } else {
            st.reset_streak();
        }
```

The only change vs. pre-fix is `end_idx = i - 1` (was `ev`/`i`). `phase_start_idx = i` is unchanged, so the boundary event `i` is now the first event of the Drift episode and is no longer also the last event of the Exploration episode.

Edge note: `phase_start_idx` reaching `i-1` requires `i >= 1`, which always holds here — the loop starts at `i = 1` (classifier.rs:169) and the drift branch only runs inside the loop body, so `i - 1 >= 0` and `phase_start_idx <= i - 1` (the exploration episode started no later than the current event). The `[phase_start_idx ..= i-1]` slice is therefore always non-empty and valid.

- [ ] **Step 4: Update the final-episode call site**

Replace the trailing final emit (classifier.rs:238-247):

```rust
    // Emit the final episode [phase_start_idx ..= last].
    let (basis, confidence) = phase_basis_confidence(st.current_phase);
    out.push(emit(
        session_id,
        st.current_phase,
        events,
        st.phase_start_idx,
        events.len() - 1,
        basis,
        confidence,
    ));
```

This guarantees the Drift episode (whose `phase_start_idx = i`) is closed here at the last event, giving it a non-empty span and evidence.

- [ ] **Step 5: Run the synthetic test → GREEN**

Run: `cargo test --test episode_drift_no_overlap 2>&1 | tail -20`
Expected: all 4 tests PASS — no shared event_ids, monotonic non-negative durations, every episode (incl. drift) has non-empty evidence.

- [ ] **Step 6: Run the existing classifier suite → no regressions**

Run: `cargo test episode 2>&1 | tail -40`
This covers the in-crate `classifier::tests`, `episode_classifier_basic`, `episode_determinism`, `episode_gold`, `episode_gold_count_invariant`, `episode_rebuild_writes_rows`, `episode_rule_registry`, `migration_episode_schema`, `api_episodes`.
Expected: ALL PASS. In particular `episode_gold` (the two frozen goldens `aac68973`/`ed82aee9`) must still match — neither fixture is long enough to hit `DRIFT_THRESHOLD`, and `episode_id`/phase derivation is unchanged, so the recorded phase sequences and counts are byte-stable.

If `episode_gold` regresses, STOP — the off-by-one or `emit` rewiring changed a non-drift boundary; re-diff against the "only change is end_idx = i-1" intent before touching the goldens (updating a golden requires explicit justification per `tests/episode_gold.rs:5-10`).

- [ ] **Step 7: Commit (test + fix together)**

```bash
git add src/insight/episode/classifier.rs tests/episode_drift_no_overlap.rs
git commit -m "fix(episode): drift transition double-classifies boundary event (spec §6.4)

Close the Exploration episode at i-1 (not i) and start Drift at i, matching
the normal-boundary off-by-one, so each event belongs to exactly one episode.
Populate evidence_node_ids with spanned event_ids. Fixes overlapping episodes,
zero-duration rows, negative gaps, and empty evidence."
```

(Per CLAUDE.md the test that locks this change ships in the same commit; no AI-attribution footer.)

---

## Task 3: Real-fixture invariant guard (RED → GREEN)

**Files:**
- Create: `tests/episode_no_overlap_real.rs`

Lock the no-overlap / monotonic / evidenced invariants against the **real** frozen transcripts so future classifier edits can't silently reintroduce the bug. Mirrors the fixture-loading pattern of `tests/episode_gold.rs:48-73` (in-memory pool + `store::ingest_file(..., &NoopSink)` + `repo_observed::list_session`) and `tests/episode_determinism.rs`. Both real fixtures are short (6 and 2 events) so they don't hit drift, but the invariant must hold for *every* session regardless of phase — this is the regression net that the corrupted `653ea169` would have tripped.

- [ ] **Step 1: Write the test**

Create `tests/episode_no_overlap_real.rs`:

```rust
//! insight-redesign #4 — real-fixture invariant net for the episode classifier.
//!
//! For every frozen real transcript, the classifier output must:
//!   - partition events: no event_id appears in two episodes;
//!   - be time-monotonic by started_at with ended_at >= started_at;
//!   - carry non-empty evidence_node_ids on every episode.
//! These held vacuously pre-fix on the short fixtures but are the regression
//! net for the long-session double-classification bug (spec §6.4).

use std::collections::{HashMap, HashSet};

use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::{migrate, repo_observed, repo_verification_run};
use witmcc::ingest::store;
use witmcc::insight::episode::classifier::classify_session;
use witmcc::live::NoopSink;
use witmcc::model::observed::ObservedEvent;

async fn load(fixture: &str) -> (String, Vec<ObservedEvent>, Vec<witmcc::db::repo_verification_run::VerificationRunRow>) {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let path = format!("tests/fixtures/transcripts/real/{fixture}.jsonl");
    store::ingest_file(&pool, std::path::Path::new(&path), &NoopSink)
        .await
        .unwrap();
    let sessions = repo_observed::list_sessions(&pool, 10).await.unwrap();
    assert!(!sessions.is_empty(), "fixture {fixture} produced no sessions");
    let sid = sessions[0].session_id.clone();
    let evs = repo_observed::list_session(&pool, &sid, 100_000).await.unwrap();
    let runs = repo_verification_run::list_session(&pool, &sid).await.unwrap_or_default();
    (sid, evs, runs)
}

fn assert_invariants(sid: &str, evs: &[ObservedEvent], runs: &[witmcc::db::repo_verification_run::VerificationRunRow]) {
    let eps = classify_session(sid, evs, runs);
    assert!(!eps.is_empty(), "non-empty stream must yield episodes");

    let idx: HashMap<&str, usize> = evs
        .iter()
        .enumerate()
        .map(|(i, e)| (e.event_id.as_str(), i))
        .collect();

    let mut seen: HashSet<usize> = HashSet::new();
    let mut prev_start: Option<chrono::DateTime<chrono::Utc>> = None;
    for e in &eps {
        let s = idx[e.start_event_id.as_str()];
        let t = idx[e.end_event_id.as_str()];
        assert!(s <= t, "{sid}: start idx {s} > end idx {t}");
        for i in s..=t {
            assert!(seen.insert(i), "{sid}: event idx {i} in two episodes (overlap)");
        }
        assert!(e.ended_at >= e.started_at, "{sid}: negative/zero-duration episode {:?}", e.phase);
        if let Some(p) = prev_start {
            assert!(e.started_at >= p, "{sid}: episodes not monotonic by started_at");
        }
        prev_start = Some(e.started_at);
        assert!(!e.evidence_node_ids.is_empty(), "{sid}: empty evidence on {:?}", e.phase);
    }
    assert_eq!(seen.len(), evs.len(), "{sid}: events not fully partitioned");
}

#[tokio::test]
async fn verification_v01_invariants() {
    let (sid, evs, runs) = load("verification_v01").await;
    assert_invariants(&sid, &evs, &runs);
}

#[tokio::test]
async fn structured_patch_v01_invariants() {
    let (sid, evs, runs) = load("structured_patch_v01").await;
    assert_invariants(&sid, &evs, &runs);
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test --test episode_no_overlap_real 2>&1 | tail -20`
Expected: PASS (Task 2's `emit` fix already populates evidence + partitions events). If `every_episode_has_non_empty_evidence`-style assertion fails here but passed in Task 1, the fixture-loaded events have a different `event_id` shape than the synthetic ones — confirm the `evidence_node_ids` are real event_ids, not empty.

(This test is GREEN immediately after Task 2 because it asserts the now-fixed behaviour against real data; it is the locking net, not a new failing requirement. Committing it here is allowed — it documents/freezes the fix against real fixtures.)

- [ ] **Step 3: Full suite — no regressions**

Run: `cargo test 2>&1 | tail -20`
Expected: whole backend suite green.

- [ ] **Step 4: Commit**

```bash
git add tests/episode_no_overlap_real.rs
git commit -m "test(episode): real-fixture invariant net — no overlap / monotonic / evidenced"
```

---

## Task 4: Regenerate dev data + implementation notes

**Files:**
- Modify: `docs/implementation-notes.html`

`episode` rows are written by `graph::build::rebuild_session` on ingest; existing dev rows carry the pre-fix overlapping spans and empty evidence. Re-ingest regenerates them. No migration (the `evidence_node_ids` column already exists), so a plain re-ingest suffices — but `init-db` is the safe path documented in CLAUDE.md for episode-table changes.

- [ ] **Step 1: Rebuild + re-ingest so episode rows reflect the fix**

Run: `cargo run --bin witmcc -- init-db && cargo run --bin witmcc -- ingest --all 2>&1 | tail -5`
Expected: ingest completes; no errors.

- [ ] **Step 2: Verify the fix against the formerly-corrupted real session**

Spec §3/§6.4 cite session `653ea169` (513 shared event_ids, 39 zero-duration episodes). Confirm the fix on the live DB:

```bash
cargo run --bin witmcc -- serve --bind 127.0.0.1 --port 7878 &
sleep 2
# adjust the session id to the locally-present 653ea169-* session if the suffix differs
curl -s "http://127.0.0.1:7878/v1/sessions/653ea169-1121-442e-9cc9-776471a10895/episodes" \
  | python3 -c "
import sys, json
# EpisodesResponse is { \"data\": [ EpisodeDto, ... ] } (no meta envelope here).
# EpisodeDto.evidence_node_ids is already a JSON array (Vec<Value>), NOT a string.
eps = json.load(sys.stdin)['data']
print('episodes:', len(eps))
bad_dur = [e for e in eps if e['ended_at'] <= e['started_at']]
print('zero/neg-duration:', len(bad_dur))
empty_ev = [e for e in eps if len(e['evidence_node_ids']) == 0]
print('empty-evidence:', len(empty_ev))
print('drift episodes:', sum(1 for e in eps if e['phase'] == 'drift'))
"
# stop the server afterward
kill %1 2>/dev/null
```

Expected after the fix: `zero/neg-duration: 0`, `empty-evidence: 0`, and a plausible (small) drift count instead of the corrupted explosion. If the exact `653ea169-…` UUID isn't present locally, list sessions first (`/v1/sessions`) and substitute the real one — the point is to confirm the previously-broken long session now reports clean spans. (Field shapes confirmed against `src/api/dto.rs:186-201`: `evidence_node_ids` is `Vec<serde_json::Value>` so it's already a list — do not `json.loads` it; `started_at`/`ended_at` are RFC3339 strings, so the lexicographic `<=` comparison is a valid same-zone ordering check.)

- [ ] **Step 3: Document in implementation notes**

Add a new `§` entry to `docs/implementation-notes.html` (follow the existing section markup — self-contained HTML, no external JS/CSS). Record:
- The drift-boundary off-by-one fix: Exploration now closes at `i-1`, Drift starts at `i`, mirroring the normal boundary so each event belongs to exactly one episode (was: both ended and started at `i`).
- `evidence_node_ids` semantics: populated with the **event_ids** the episode spans (the classifier owns event_ids, not derived graph node_ids; the field name is historical from slice-12 and stays for schema stability). This satisfies the CLAUDE.md "Evidence-linked" principle for episodes.
- Determinism preserved: `episode_id = sha256(session_id||phase||start_event_id||end_event_id)` inputs unchanged ⇒ ids stable; both frozen goldens still match.
- Operational note: no migration; re-ingest (or `init-db` + re-ingest) regenerates episode rows with corrected spans + evidence.
- Reference spec §6.4 and the real-fixture invariant test as the lock.

- [ ] **Step 4: Commit**

```bash
git add docs/implementation-notes.html
git commit -m "docs(episode): implementation notes for drift double-classification fix (§6.4)"
```

---

## Done criteria

- Drift transition no longer double-classifies the boundary event; every event belongs to exactly one episode (synthetic + real-fixture invariant tests green).
- No zero-duration / negative-gap episodes; episodes monotonic by `started_at`; `ended_at >= started_at`.
- Every episode — drift included — has non-empty `evidence_node_ids` (the spanned event_ids).
- `episode_id` derivation and the two frozen goldens (`aac68973`, `ed82aee9`) unchanged; determinism invariant holds.
- `cargo test` green, no regressions. Re-ingest confirms session `653ea169` now reports 0 zero-duration / 0 empty-evidence episodes.
- This unblocks spec §3 Q1's drift-as-inefficiency signal and §5's `drift*` "(보정 후 신뢰)" phase-bar badge; the corrupted-episode-duration footgun behind the §3 Q3 "289h" mis-derivation is removed (Q3 total-time should be computed from `observed_event.observed_at`, per spec — not episode durations).
