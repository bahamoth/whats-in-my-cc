# Slice-12 Implementation Plan — Episode Segmentation

**Spec:** `docs/superpowers/specs/2026-05-27-witmcc-slice12-episode-segmentation-design.md`
**Branch:** `slice12-episode-segmentation`
**Strategy:** TDD red-first. Five phases, five commits. Golden output is bootstrapped in commit 4 (the only auto-populate in the project).

---

## Phase 0 — Branch & baseline

| # | Task | Action |
|---|---|---|
| 0a | Cut branch | `git checkout main && git pull && git checkout -b slice12-episode-segmentation` |
| 0b | Baseline counts | Record `cargo test` count + `vitest run` count |
| 0c | Confirm slice-11 merged | `git log --oneline \| grep slice-11` exists; otherwise rebase |

No commit.

---

## Phase 1 — Red-locking tests

### Task 1 — Rule registry invariant

**Files:** Create `tests/episode_rule_registry.rs`.

- [ ] **Step 1: Write failing test**

```rust
use witmcc::insight::episode::rules::RULE_IDS;

#[test]
fn rule_ids_are_canonical() {
    let expected: &[&str] = &[
        "phase_intake_fresh_user_message@v1",
        "phase_exploration_read_only_window@v1",
        "phase_diagnosis_after_error@v1",
        "phase_action_first_mutation@v1",
        "phase_verification_run_window@v1",
        "phase_repair_after_failed_verification@v1",
        "phase_drift_long_exploration@v1",
    ];
    assert_eq!(RULE_IDS, expected);
}
```

- [ ] **Step 2: Stub the module so the test compiles**

```rust
// src/insight/episode/mod.rs
pub mod rules;
pub mod classifier;

// src/insight/episode/rules.rs
pub const RULE_IDS: &[&str] = &[];
```

Register `pub mod episode;` in `src/insight/mod.rs`.

- [ ] **Step 3: Run; expect assertion fail**

```bash
cargo test --test episode_rule_registry
```

Expected: `left == right` failure showing the empty slice.

### Task 2 — Classifier API skeleton (compile-only red)

**Files:**
- Create: `src/insight/episode/classifier.rs`
- Create: `tests/episode_classifier_basic.rs`

- [ ] **Step 1: Test the classifier on a hand-built event stream**

```rust
use chrono::{TimeZone, Utc};
use witmcc::insight::episode::classifier::classify_session;
use witmcc::insight::episode::types::Phase;
use witmcc::model::observed::{ObservedEvent, EventKind, Actor};

fn ev(i: usize, actor: Actor, kind: EventKind, tool: Option<&str>) -> ObservedEvent {
    ObservedEvent {
        event_id: format!("ev_{i:03}"),
        raw_event_id: format!("raw_{i:03}"),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_t".into(),
        event_uuid: Some(format!("uuid_{i}")),
        observed_at: Utc.timestamp_opt(1_700_000_000 + i as i64, 0).unwrap(),
        actor, kind,
        tool_name: tool.map(String::from),
        parser_version: "test".into(),
        ..Default::default()
    }
}

#[test]
fn classifies_intake_then_exploration_then_action() {
    let evs = vec![
        ev(0, Actor::User,      EventKind::UserMessage,      None),
        ev(1, Actor::Assistant, EventKind::ToolCall,         Some("Read")),
        ev(2, Actor::Tool,      EventKind::ToolResult,       Some("Read")),
        ev(3, Actor::Assistant, EventKind::ToolCall,         Some("Edit")),
        ev(4, Actor::Tool,      EventKind::ToolResult,       Some("Edit")),
    ];
    let eps = classify_session("sess_t", &evs, &[]);
    let phases: Vec<Phase> = eps.iter().map(|e| e.phase).collect();
    assert_eq!(phases, vec![Phase::Intake, Phase::Exploration, Phase::Action]);
}

#[test]
fn empty_session_emits_zero_episodes() {
    let eps = classify_session("sess_t", &[], &[]);
    assert!(eps.is_empty());
}

#[test]
fn diagnosis_after_error() {
    let evs = vec![
        ev(0, Actor::User,      EventKind::UserMessage, None),
        ev(1, Actor::Assistant, EventKind::ToolCall,    Some("Bash")),
        {
            let mut e = ev(2, Actor::Tool, EventKind::ToolResult, Some("Bash"));
            e.payload = serde_json::json!({"is_error": true, "stderr": "boom"});
            e
        },
        ev(3, Actor::Assistant, EventKind::ToolCall, Some("Read")),
        ev(4, Actor::Tool,      EventKind::ToolResult, Some("Read")),
    ];
    let eps = classify_session("sess_t", &evs, &[]);
    assert!(eps.iter().any(|e| e.phase == Phase::Diagnosis),
            "expected diagnosis phase; got {:?}", eps.iter().map(|e| e.phase).collect::<Vec<_>>());
}

#[test]
fn verification_phase_when_run_present() {
    use witmcc::db::repo_verification_run::VerificationRunRow;
    let evs = vec![
        ev(0, Actor::User,      EventKind::UserMessage, None),
        ev(1, Actor::Assistant, EventKind::ToolCall,    Some("Bash")),
        ev(2, Actor::Tool,      EventKind::ToolResult,  Some("Bash")),
    ];
    let runs = vec![VerificationRunRow {
        trigger_event_id: "ev_002".into(),
        started_at: "1970-01-01T00:00:00Z".into(),
        ..Default::default()
    }];
    let eps = classify_session("sess_t", &evs, &runs);
    assert!(eps.iter().any(|e| e.phase == Phase::Verification));
}
```

- [ ] **Step 2: Stub the module**

```rust
// src/insight/episode/types.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase { Intake, Exploration, Diagnosis, Action, Verification, Repair, Drift }

#[derive(Debug, Clone)]
pub struct EpisodeRecord {
    pub phase: Phase,
    pub start_event_id: String,
    pub end_event_id: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub ended_at: chrono::DateTime<chrono::Utc>,
    pub evidence_node_ids: Vec<String>,
    pub classification_basis: Vec<&'static str>,
    pub confidence: f32,
}

// src/insight/episode/classifier.rs
pub fn classify_session(
    _session_id: &str,
    _events: &[crate::model::observed::ObservedEvent],
    _runs: &[crate::db::repo_verification_run::VerificationRunRow],
) -> Vec<super::types::EpisodeRecord> {
    Vec::new()
}
```

- [ ] **Step 3: Re-run, expect logical failures**

```bash
cargo test --test episode_classifier_basic
```

Expected: assertions fail (empty Vec returned).

### Task 3 — Schema-shape invariant

**Files:** Create `tests/migration_episode_schema.rs`.

```rust
#[tokio::test]
async fn migration_creates_episode_table() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let cols: Vec<(String, String)> = sqlx::query_as(
        "SELECT name, type FROM pragma_table_info('episode')"
    ).fetch_all(&pool).await.unwrap();
    let names: Vec<&str> = cols.iter().map(|c| c.0.as_str()).collect();
    for c in [
        "episode_id","schema_version","session_id","phase",
        "start_event_id","end_event_id","started_at","ended_at",
        "evidence_node_ids","classification_basis","confidence",
        "summary","classifier_version","created_at",
    ] {
        assert!(names.contains(&c), "missing column {}", c);
    }
}
```

- [ ] Run, expect panic (migration missing).

### Task 4 — Golden test (red — golden file empty)

**Files:**
- Create: `tests/fixtures/episode_gold/aac68973.json` (with `"expected_episodes": []`)
- Create: `tests/episode_gold.rs`

```rust
use serde::Deserialize;
use witmcc::insight::episode::classifier::classify_session;

#[derive(Deserialize)]
struct Gold {
    expected_episodes: Vec<ExpectedEpisode>,
}

#[derive(Deserialize)]
struct ExpectedEpisode {
    phase: String,
    start_event_offset_in_session: usize,
    end_event_offset_in_session: usize,
    classification_basis: Vec<String>,
}

#[test]
fn aac68973_golden_matches() {
    let evs = load_real_transcript("aac68973");
    let runs = load_real_verification_runs_for("aac68973");
    let got = classify_session("aac68973", &evs, &runs);
    let want: Gold = serde_json::from_str(
        &std::fs::read_to_string("tests/fixtures/episode_gold/aac68973.json").unwrap()
    ).unwrap();
    assert_eq!(got.len(), want.expected_episodes.len(),
               "episode count diverged: got {}, expected {}",
               got.len(), want.expected_episodes.len());
    for (i, (g, w)) in got.iter().zip(want.expected_episodes.iter()).enumerate() {
        assert_eq!(format!("{:?}", g.phase).to_lowercase(), w.phase, "episode {i} phase");
        // Offsets are checked against the source order; helper finds the offset of
        // start_event_id / end_event_id inside `evs`.
    }
}
```

(`load_real_transcript` + `load_real_verification_runs_for` are helpers in `tests/common/fixtures.rs`.)

- [ ] Run. Will fail with `got.len() == 0` and `want.expected_episodes.len() == 0` matching at first; the assertion only becomes meaningful after the golden is populated in commit 4. For commit 1 we ensure the test compiles and passes (zero on both sides). This is intentional — the test arms itself once the golden has content.

### Task 5 — API endpoint shape (red)

**Files:** Create `tests/api_episodes.rs`.

```rust
#[tokio::test]
async fn episodes_endpoint_returns_rows() {
    let pool = test_pool_with_seeded_episodes().await;  // helper inserts 2 episode rows
    let server = axum_test::TestServer::new(witmcc::api::build_router(pool)).unwrap();
    let r = server.get("/v1/sessions/sess_t1/episodes").await;
    r.assert_status_ok();
    let body: serde_json::Value = r.json();
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
    let p = body["data"][0]["phase"].as_str().unwrap();
    assert!(["intake","exploration","diagnosis","action","verification","repair","drift"].contains(&p));
}
```

- [ ] Run; expect 404.

### Phase-1 commit

```bash
git add src/insight/episode/ src/insight/mod.rs \
        tests/episode_rule_registry.rs tests/episode_classifier_basic.rs \
        tests/migration_episode_schema.rs tests/episode_gold.rs tests/api_episodes.rs \
        tests/fixtures/episode_gold/aac68973.json
git commit -m "test(slice-12): red-locking tests for episode classifier + schema + API"
```

---

## Phase 2 — DB migration + repo

| # | Task | Verify |
|---|---|---|
| 6 | Author `migrations/20260528120000_0006_episode.sql` per spec §4 | `cargo test --test migration_episode_schema` green |
| 7 | Create `src/db/repo_episode.rs` with `insert(row) / list_session(pool, sid) / get(id)` | Roundtrip test green |

**Commit 2:** `feat(db): 0006_episode migration + repo_episode`

---

## Phase 3 — Classifier body

| # | Task | Verify |
|---|---|---|
| 8 | Implement state machine in `src/insight/episode/classifier.rs`. Use the lookahead window per spec §3. Populate `RULE_IDS` per spec §4. | `cargo test --test episode_rule_registry` + `episode_classifier_basic` green |
| 9 | Wire `classify_session` into `rebuild_session` in `src/graph/build.rs` so episode rows are written every rebuild. Wrap the call in `catch_unwind` per spec §8. | A new test `tests/episode_rebuild_writes_rows.rs` asserts non-empty rows on a real session. |
| 10 | Determinism test `tests/episode_determinism.rs` — running the classifier twice on the same input returns identical IDs (recall the ID is sha256 of session/phase/start/end). | Green |

**Commit 3:** `feat(insight): episode classifier state machine + rule registry`

---

## Phase 4 — Golden bootstrap

| # | Task | Verify |
|---|---|---|
| 11 | Run `cargo test --test episode_gold aac68973_golden_matches 2>&1 | head -20`. Pipe the actual `got` output to `tests/fixtures/episode_gold/aac68973.json` (write a small helper in `tests/episode_gold.rs` behind `--features write-golden` that, when run with `WITMCC_WRITE_GOLDEN=1`, writes instead of compares). | After bootstrap, test passes against itself. |
| 12 | Repeat for every other transcript in `~/.claude/projects/.../` (5–10 files). One golden file per transcript. | Each test green. |
| 13 | Add `tests/episode_gold_count_invariant.rs` — asserts the number of `episode_gold/*.json` files matches a constant; raising/lowering requires updating the constant in the same commit. | Green. |

**Commit 4:** `feat(insight): episode_gold bootstrap from real transcripts`

> **Self-check:** the golden output is now real-data anchored. Every transcript's per-phase rule_id is recorded. Any future change to the classifier produces a diff in the golden — which becomes the unit of review.

---

## Phase 5 — Pull API

| # | Task | Verify |
|---|---|---|
| 14 | Add handler `list_episodes(State, Path)` + `episode_detail(State, Path)` in `src/api/routes.rs`. | API tests green |
| 15 | Register routes in `src/api/mod.rs` | n/a |
| 16 | Add a small `DTO` in `src/api/dto.rs` mapping `EpisodeRow → EpisodeResponse` (JSON-parses the `evidence_node_ids` + `classification_basis` columns) | n/a |

**Commit 5:** `feat(api): /v1/sessions/:id/episodes + /v1/episodes/:id`

---

## Phase 6 — Smoke + verification

```
Smoke — slice-12

[ ] witmcc init-db  (delete old .witmcc.sqlite* first)
[ ] witmcc ingest /Users/bahamoth/.claude/projects/.../aac68973*.jsonl
[ ] witmcc serve --port 4337 &
[ ] curl -s http://127.0.0.1:4337/v1/sessions/aac68973/episodes | jq 'length'
    # Expected: non-zero, equal to golden episode count.
[ ] curl -s http://127.0.0.1:4337/v1/sessions/aac68973/episodes | \
      jq '[.data[] | .phase] | unique'
    # Expected: subset of [intake, exploration, diagnosis, action, verification, repair, drift]
[ ] curl -s http://127.0.0.1:4337/v1/sessions/aac68973/episodes | \
      jq '.data[0].classification_basis'
    # Expected: non-empty array of versioned rule ids
```

```
Verification — slice-12

- cargo test count: baseline (post slice-11) → expected + 10..14 (rule registry, classifier basic 4, schema, golden 1, episode rebuild 1, determinism 1, api 1, gold count invariant 1)
- vitest run count: unchanged
- aac68973 episode count: locked by golden
- Rebuild latency: ≤ 1.25 × pre-slice-12 baseline
```

---

## Phase 7 — PR

Same template as slice-11. Title: `feat(slice-12): Episode segmentation (state machine + golden)`. Implementation notes update + CLAUDE.md status pointer.
