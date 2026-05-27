# Remaining-Milestones Roadmap — slice-11 through slice-19

**Date:** 2026-05-27
**Branch:** `planning-remaining-milestones`
**Status of repo at planning time:** slice-1 ~ slice-10a + slice-10a follow-up merged. `cargo test` 189 green, `vitest run` 68 green. Real session `aac68973` covers all currently-implemented kinds.
**Scope of this document:** Lock the design surface for every remaining MVP milestone (M3 잔여 / M5 / M6 / M7) and slice-decompose it so the next 9 slices are individually shippable, test-first, and regression-locked. UI/UX redesign is **explicitly out of scope** of this roadmap — it is moved to a separate epic document (`2026-05-27-witmcc-ux-redesign-epic.md`) outside the milestone path.

---

## 0. Why this exists

The remaining work is no longer a sequence of independent data sources to wire up. It is now an analysis pipeline that has to land **carefully** — particularly the Insight engine, which by user direction must avoid both naïve LLM-per-event cost and brittle pure-deterministic rules. This document is the single artifact that:

- Decomposes the remaining milestones into the smallest shippable units.
- Locks design decisions for the slices that are most risky to redesign mid-implementation (Insight engine architecture, MCP transport, hardening).
- Gives each slice a **TDD plan**, a **smoke plan**, and a **regression-test inventory** so verification is the same as long-lived test coverage.
- Maps every plan to MVP acceptance criteria (AC-1 ~ AC-7 in `docs/06_mvp_execution_plan.html`) so coverage can be checked at any point.

Pre-existing slice docs (`slice-1` ~ `slice-10a`) carry their own design specs + plans + implementation notes; this roadmap extends that pattern.

---

## 1. Remaining acceptance gap

| AC | Status today | Closing slice(s) |
|----|--------------|------------------|
| AC-1 single-session replay | ✅ closed by slice-2/8/9 | — |
| AC-2 OTel linkage ≥90 % of tool/model ops | ✅ closed by slice-3/6 + graph merge | — |
| AC-3 file lineage → tool call → episode → verification | ◐ tool call done (slice-10a); **episode + verification missing** | **slice-11 (VerificationRun)** + **slice-12 (Episode segmentation)** |
| AC-4 every finding has `evidence_refs` + confidence | ❌ no finding generator | **slice-14 (Insight L1)** + **slice-15/16 (Insight L2 infra + categories)** |
| AC-5 read-only Pull API + MCP | ◐ Pull API done; **MCP missing** | **slice-17 (MCP Streamable HTTP)** |
| AC-6 localhost bind + Origin reject + token | ◐ bind + Origin done; **token missing** | **slice-19 (token auth + retention sweep)** |
| AC-7 redaction manifest on export | ❌ no manifest emission | **slice-18 (redaction gate + manifest)** |

Two MVP-scoped gaps not directly in AC numbering but called out by the same execution plan:

| Gap | Closing slice |
|-----|---------------|
| M3 "basic causal inference" — edges still mostly deterministic | **slice-13 (causal-edge inference)** |
| M4 Why Panel / Source Views / Resource Drawer | **deliberately deferred to UX redesign epic** — see §11 |

---

## 2. Slice graph (dependencies)

```
              slice-11 ──┐
              (verif)    │
                         ▼
              slice-12 ──┐
              (episode)  │
                         ▼
slice-13 (causal-inf) ───┤
                         │
                         ▼          [optional UX redesign epic — outside milestone path]
slice-14 ──┐ (insight L1)│
           ▼             │
slice-15 ──┐ (L2 infra)  │
           ▼             ▼
slice-16 ── (L2 cats) ── (data plane complete)
                         │
                         ▼
slice-17 ── (MCP) ── (serving surface complete)
                         │
                         ▼
slice-18 ── (redaction) ──┐
                          ▼
slice-19 ── (auth/retention) ── (M7 hardening complete = MVP exit)
```

**Critical observations:**
- Slice-11 and slice-12 are independent in implementation but both feed slice-13 (which needs phase labels + verification edges to draw `error → repair` and `edit → verification` inferred edges). Within this roadmap they are **sequenced 11 → 12 → 13** but parallel execution by two contributors is allowed; the integration commit lives in slice-13.
- Slice-14 has a **hard dependency** on slice-11 (a finding like `missing_verification` has no evidence without `VerificationRun` rows).
- Slice-17 (MCP) only needs slices 11–13 to expose meaningful data; it can run in parallel with the Insight track (14–16). The roadmap sequences MCP **after** Insight v1 because demonstrating MCP without findings is low value — but this ordering is a recommendation, not a code dependency.
- Hardening (18–19) is **sequenced last** because adding redaction/auth retroactively to a working pipeline is safer than gating an unfinished pipeline behind redaction.

---

## 3. Per-slice charter (capsule design)

Each slice in §4–§10 has its own detailed design spec + TDD plan files. This section gives the one-page capsule so a reader can hold the whole arc in one view.

### Slice-11 · VerificationRun ingest (M3 closer · AC-3)

**Goal:** emit `VerificationRun` rows whenever an observed `Bash` tool_result is a "test command" (deterministic name match), or a hook PreToolUse/PostToolUse explicitly identifies itself as `verification`, or an OTel span carries `attributes["verification.kind"]`. Wire them onto the graph as `verification_run` nodes with `tool_call → verification_run` edges.

**Why now:** AC-3 and AC-4 both depend on it. Without `VerificationRun` rows, the entire `missing_verification` finding category is structurally unfilable.

**Source contract:**
- Bash branch: `tool_name == "Bash"` + command string matches a frozen allowlist (`npm test`, `npm run test*`, `cargo test`, `cargo nextest`, `pytest`, `go test`, `mvn test`, `gradle test`, `vitest`, `jest`, `mocha`, plus `--lib`/`--bin`/`--package` suffix tolerance). The match is encoded as a closed list, locked by a real-fixture invariant test (we have ≥3 real Bash test runs in `~/.claude/projects/`).
- Hook branch: PreToolUse/PostToolUse `hook_event_name` with `hook_input.tool_name == "Bash"` *and* the surrounding hook payload carries `"verification": true` OR `hook_event_name == "PostToolUse"` *and* the matched Bash command was on the allowlist.
- OTel branch: `OtelSpan` with `attributes["verification.kind"]` (string, e.g., `"test_suite"`, `"build"`, `"lint"`). This is a defensive-future slot; no real-data anchoring is currently possible (we have not seen this attribute in any captured span). The branch is parser-version-gated and the test asserts it stays off until a real fixture lands.

**Outputs:**
- New side-table `verification_run` (see schema in slice-11 design spec §4).
- New graph node kind `verification_run` (no new `EventKind`; the row references its trigger event).
- New edge kind `triggered_verification`: `tool_call → verification_run` (deterministic, derived from `trigger_event_id`).
- New edge kind `covers_diff_hunk`: `verification_run → diff_hunk`, populated only when the diff_hunk's introducing tool_use happens **before** the verification_run's start_at and within the same session. **No fuzzy file-path matching** yet — slice-11 only links by temporal precedence + same session.

**Coverage check:** AC-3 partially closed (lineage tool_call → verification works; diff_hunk → verification coverage requires slice-12 episode segmentation to scope the "before" window correctly).

### Slice-12 · Episode segmentation (M3 closer · AC-3)

**Goal:** deterministic phase labels (`intake / exploration / diagnosis / action / verification / repair / drift`) per contiguous run of events. One `Episode` row per phase span. Stored, served via Pull API, exposed as graph nodes for later UI.

**Why now:** Episode is the scoping primitive for nearly every finding in slice-14/16. "missing_verification" only fires if an `action` phase is *not* followed by a `verification` phase within the same `intake → … → next intake` window. Without phases that decision rule is hand-rolled per finding.

**Algorithm:** classification by deterministic rule on a sliding window of (actor, kind, subkind, tool_name) tuples. The classifier is a function `classify_phase(window: &[EventSummary]) -> Phase` that uses **explicit transition rules** (encoded as a state machine in `src/insight/episode.rs`). The state machine is locked by **invariant tests over the 9 real transcripts** that already live in `tests/fixtures/transcripts/real/`. Phase labels for those transcripts are pinned as golden output; future code changes that move boundaries must update the golden file and rationalise the move in the commit body.

**Outputs:**
- `episode` side-table per `docs/03_data_model_spec.html` §6.
- Pull API endpoint `GET /v1/sessions/:id/episodes`.
- Graph wiring deferred: episodes are *not* graph nodes in slice-12; they are a side-table read by slice-14 and the UI epic. (Adding them as graph nodes would force a schema decision UX redesign should make.)

### Slice-13 · Causal-edge inference (M3 closer)

**Goal:** add inferred edges that the current deterministic-only builder misses:

1. `caused_repair` — `tool_result(error) → next tool_call(same file or same trace)` within N seconds, where the second call's input mentions text from the first's error string (lexical overlap rule with a frozen threshold).
2. `triggered_by_user_message` — `user_message → tool_call` whose timestamp is within the same turn and whose tool_call has no preceding `assistant_message` in the same turn (defensive — covers slash-command-shaped invocations).
3. `large_output_to_next_action` — `tool_result(payload_size > threshold) → next assistant_message` flagged so slice-16 can mark `context_bloat` candidates.

Each inferred edge carries:

```rust
{
  rule_id: "caused_repair_v1",
  confidence: 0.65,
  match_terms: ["NameError", "users.py"]  // diagnostic
}
```

**Locking:** rules are version-suffixed (`_v1`); changing thresholds requires bumping `_v2` and updating the golden. The 9 real transcripts produce a frozen counts file `tests/fixtures/inferred_edge_counts.json` that the test asserts against.

### Slice-14 · Insight Engine v1 — L1 deterministic (M5 partial · AC-4)

See dedicated architecture document `2026-05-27-witmcc-insight-engine-architecture.md` for the L1/L2 split rationale. Slice-14 ships the **L1-only** subset:

- `missing_verification` — action episode without following verification episode in same `intake → next-intake` window.
- `tool_failure` — `tool_result.is_error == true` with no compensating successful retry within the same window.

Both categories use **purely deterministic rules** — no LLM calls. `evidence_refs` is constructed at extraction time and locked schema-side. `confidence` is fixed by rule (`missing_verification: 0.9`, `tool_failure: 1.0` since we read the error flag directly).

### Slice-15 · Insight Engine L2 — judge infrastructure (M5 partial)

**Goal:** add the LLM judge layer, but make it (a) **off-by-default** on a per-installation basis, (b) **budget-bounded** per session, (c) **cache-keyed** by candidate-evidence-hash so repeated rebuilds do not re-spend tokens.

Spec details in `2026-05-27-witmcc-insight-engine-architecture.md` §5–7. Slice-15 ships **only the infrastructure** — no new finding categories. A `noop_judge` test category is added solely to exercise the cache + budget paths end-to-end. Categories that actually use the judge land in slice-16.

### Slice-16 · Insight Engine L2 — categories (M5 closer · AC-4)

**Goal:** three categories that benefit from the L2 judge:
- `risky_action` — destructive command or `userModified` flag on a Claude-introduced hunk, gated through a judge that decides "was this a deliberate user-supervised destructive action or genuinely unsupervised?"
- `context_bloat` — large `tool_result.payload_size_bytes` followed by an assistant_message of low effective newness; judge decides "was the bloat reused or wasted?"
- `final_state_mismatch` — explicit goal in `user_message` was not reflected in the final state (last `assistant_message` and any closing verification result); judge produces a structured `mismatch_summary`.

### Slice-17 · MCP Streamable HTTP (M6 · AC-5)

**Goal:** add the `/mcp` endpoint per `docs/04_api_mcp_spec.html` §5. JSON-RPC over POST + SSE on GET. Resources (`whats-in-my-cc://sessions/{id}`, etc.) backed by the same Pull-API data layer. Read-only tools `get_session_graph`, `search_findings`, `explain_node`, `get_file_lineage`, `get_otel_trace`.

**Implementation lever:** we deliberately do not pull in `rmcp` or another MCP SDK. The MCP wire protocol (JSON-RPC framing, `initialize`/`tools/list`/`tools/call`/`resources/*`) is small enough and the spec stable enough that we hand-implement it on top of the existing `axum` server. The hand-implementation gives us exact control over Origin/token enforcement (slice-19) and over SSE notification fan-out (already used by `/v1/sessions/:id/events/stream`).

### Slice-18 · Redaction gate + manifest emission (M7 partial · AC-7)

**Goal:** populate the `redaction_state` column already present on `raw_event` and surface a `RedactionManifest` on every Pull API response *and* every MCP resource read. Rule pack: API keys, bearer tokens, AWS keys, OpenAI keys, Anthropic keys, GitHub tokens, generic private-key headers, password-y query-string params, common PII patterns (email, phone, SSN/RRN). Rule pack is versioned (`rule_pack@v1`); the version is stamped on every manifest.

### Slice-19 · Token auth + retention sweep (M7 closer · AC-6)

**Goal:**

- **Token auth:** Pull API + MCP both require `Authorization: Bearer <token>`. Token is generated on first `witmcc serve` run, written to `~/.config/witmcc/token` (mode 600) and printed once to stderr. `serve --print-token` re-prints it.
- **Retention sweep:** background tokio task wakes every 6 h, deletes rows older than the configured retention per data class (raw payload 30 d, normalized events 180 d, graph/insights 180 d, audit 90 d). Defaults match `docs/05_security_governance_spec.html` §5. Configurable via `witmcc serve --retention-profile`.

---

## 4. Standard verification structure (applies to every slice)

Every slice plan in slice-11..19 follows the same outer shell. This subsection defines the shell **once** so the slice plans can reference it.

### 4.1 Test layers

| Layer | Tool | Purpose | Becomes regression? |
|---|---|---|---|
| L1 unit | `cargo test --lib` | Pure functions: rule classifier, edge inference, cache key derivation, redaction rule application | ✅ permanent |
| L2 module | `cargo test` (integration in `tests/`) | DB roundtrip, repo functions, ingestion pipeline | ✅ permanent |
| L3 HTTP | `cargo test --test api*` / `events_subprocess` / `sse_subprocess` | HTTP route via `axum-test` or raw TCP subprocess | ✅ permanent |
| L4 webui | `vitest run` (component) + `vitest run` (e2e harness via mock fetch/SSE) | UI behaviour where touched | ✅ permanent |
| L5 cross-process | `assert_cmd` + portpicker + reqwest | Spawns `witmcc serve` subprocess, exercises end-to-end | ✅ permanent |
| L6 browser smoke | Manual or `claude-in-chrome` MCP | Real Claude Code transcript replayed in browser | ❌ ad hoc — but checklist mandated for any UI change |

**TDD rule:** every slice plan opens with a Phase 1 that adds **L1 + L2 + at least one L3** failing tests before any production code is written. Phase 2 (and onward) turns them green.

**Regression rule:** every test introduced by a slice plan must remain after the slice is merged. If a behaviour gets removed in a later slice, the test gets **updated to lock the new behaviour** (e.g., slice-10a converted three watcher tests into negative-lock tests that now assert the removed flags are rejected). The test count never drops without an explicit deviation note.

### 4.2 Smoke plan template

Each slice's plan ends with a Phase N "smoke" with the following structure:

```
Phase N · Smoke
- [ ] Bring up dev DB: `witmcc init-db && witmcc ingest <path-to-real-transcript>`
- [ ] Bring up server: `witmcc serve --bind 127.0.0.1 --port 4337`
- [ ] Verify CLI: `curl -s http://127.0.0.1:4337/v1/health | jq` → status ok
- [ ] Verify slice-specific surface (curl + jq snippets per slice)
- [ ] (UI slice only) Browser smoke: open `http://127.0.0.1:4337/`, navigate per slice spec, verify visual + console clean
- [ ] Record smoke output in commit body as "Smoke evidence:" block
```

The smoke block in the commit body is the user-visible proof; the test-suite green is the automation proof. Neither alone is sufficient.

### 4.3 Verification plan template

Each slice's plan also lists a Verification block:

```
Verification
- cargo test count: baseline N → expected N + ΔN (Δ from this slice's tests)
- vitest run count: baseline M → expected M + ΔM
- Real transcript counts (frozen): nodes/edges/episodes/findings expected counts on aac68973 and any other transcript the slice touches
- Performance: rebuild_session(aac68973) latency (recorded at slice start, must not regress more than 25 % without rationale in commit body)
```

### 4.4 PR description template

Each slice's PR description must include:

```
## Summary
<2-4 bullets of what changed>

## Acceptance Criterion mapping
<which AC this slice closes / advances>

## Test inventory (regression locks)
<bulleted list of every test added, with one-line purpose>

## Smoke evidence
<copy of the Smoke evidence block from the merge commit>

## Real-data anchoring
<which fixtures locked which invariant; if synthetic, why and where the explicit tradeoff is recorded>

## Risks / open questions
<anything the reviewer should look at hardest>
```

---

## 5. Insight engine — design pointers

The Insight engine design is deep enough that it lives in `2026-05-27-witmcc-insight-engine-architecture.md`. This roadmap section is the index into that document:

| Topic | Where |
|---|---|
| L1/L2 layer split + per-category routing table | architecture §3 |
| Candidate extractor framework (`InsightExtractor` trait) | architecture §4 |
| LLM judge interface, cache key, budget guard | architecture §5 |
| `evidence_refs` schema + serialisation contract | architecture §6 |
| False positive control policy | architecture §7 |
| Cost model + opt-in default policy | architecture §8 |
| Category catalogue (one row per finding kind) | architecture §9 |

---

## 6. UX redesign epic (deferred)

The remaining M4 surface — Why Panel, Resource Drawer, Source Views, dashboard re-do — is **deliberately removed from the slice path**. Per the 2026-05-27 design conversation, UI requires holistic redesign rather than incremental panel-by-panel addition; doing it slice-by-slice would create disposable work.

The UX redesign epic doc (`2026-05-27-witmcc-ux-redesign-epic.md`) records:

- Why the deferral is correct (incremental UI would discard once a coherent redesign starts)
- What the milestone-path data model **must** expose so the redesign has full optionality (this constrains slice-11/12/13/14/15/16 outputs)
- The data contracts the eventual redesign will consume (so we can write smoke tests against them now, even before the redesign exists)

The epic itself is not a slice. It does not have a TDD plan. It does have a smoke checklist for the **data contracts** so that when redesign lands later, the data plane is already verified.

---

## 7. Risk register (whole programme)

| Risk | Trigger | Mitigation | Owner-slice |
|---|---|---|---|
| Insight false-positive rate | Deterministic rules fire on edge cases without judge | Architecture §7: per-category confidence floor + manual gold-set test | slice-14, 16 |
| LLM judge cost explosion | Repeated rebuilds across many sessions | Architecture §5: persistent cache keyed by candidate-evidence-hash + per-session budget cap (`max_judge_calls_per_session`) | slice-15 |
| MCP transport drift | MCP spec evolves (we hand-implemented) | Pin to MCP `2024-11-05` revision (current at planning time); add a `tests/mcp_spec_compat.rs` golden fixture covering `initialize` + `tools/list` + `resources/list` shape | slice-17 |
| Migration history hash divergence | sqlx validates migration history on startup | Each slice that adds migrations uses **new numbered files** (not in-place edits). Slice-10a's in-place edit was a one-time tradeoff documented in DEV-S10A-05 | slice-11+ |
| Redaction false negatives (secrets leak) | Rule pack misses a new secret format | rule_pack version stamping (`rule_pack@v1`) + manifest reports unmatched-suspect-token counts for human review | slice-18 |
| Token rotation surprises existing clients | Re-running `witmcc serve` regenerates token | Token generated **once** on first run, persisted to disk. `--rotate-token` is opt-in. | slice-19 |
| Retention deletes user-needed evidence | Default 30 d raw payload retention deletes data user expected to keep | Retention is **off by default** in this MVP; opt-in via `--retention-profile`. Tombstone retained for deleted rows so resource IDs do not 404. | slice-19 |
| UX redesign deferral leaks scope back into milestone path | A reviewer asks "but we need at least Why Panel for the demo" | Roadmap §6 + every slice plan **explicitly states no UI surface is added**, only data-plane endpoints. Visual verification of new data uses curl + jq, not UI screenshots. | every slice |

---

## 8. Definition-of-done for the whole programme

The MVP exits when **all** of the following hold:

1. AC-1 through AC-7 in `docs/06_mvp_execution_plan.html` all green per the criterion-mapping table in §1.
2. Real session `aac68973` produces non-zero findings of at least categories `missing_verification`, `tool_failure`, `risky_action`, `context_bloat` when ingested with the default profile (smoke gate).
3. `cargo test` and `vitest run` are both green; total test count is the planning-time baseline (189 cargo + 68 vitest) plus the sum of ΔN/ΔM expected per slice plan (the actual final number is fixed in the last slice's PR description).
4. MCP endpoint `http://127.0.0.1:{port}/mcp` accepts `initialize` / `tools/list` / `tools/call` / `resources/list` / `resources/read` from a stock MCP client without any custom auth flags beyond the bearer token from `~/.config/witmcc/token`.
5. `redaction_manifest` is non-null on every Pull API + MCP response when the underlying data contains a payload of `raw_*` source_type.
6. `witmcc serve --rotate-token` rotates the token and prints the new one; existing clients receive a clean 401 (not a hang).
7. CLAUDE.md status block reflects MVP-exit; `docs/implementation-notes.html` carries an `Overview (slice-19)` section that summarises the closing of every AC.

---

## 9. Document index (planning artefacts created in this branch)

| File | Purpose |
|---|---|
| `docs/superpowers/specs/2026-05-27-witmcc-remaining-milestones-roadmap.md` | **This file**. |
| `docs/superpowers/specs/2026-05-27-witmcc-insight-engine-architecture.md` | L1/L2 split, judge cost model, cache, category routing. |
| `docs/superpowers/specs/2026-05-27-witmcc-ux-redesign-epic.md` | Deferral charter for UI redesign, data-contract preconditions. |
| `docs/superpowers/specs/2026-05-27-witmcc-slice11-verification-run-design.md` | Slice-11 design. |
| `docs/superpowers/plans/2026-05-27-witmcc-slice11-verification-run.md` | Slice-11 TDD plan. |
| `docs/superpowers/specs/2026-05-27-witmcc-slice12-episode-segmentation-design.md` | Slice-12 design. |
| `docs/superpowers/plans/2026-05-27-witmcc-slice12-episode-segmentation.md` | Slice-12 TDD plan. |
| `docs/superpowers/specs/2026-05-27-witmcc-slice13-causal-edge-inference-design.md` | Slice-13 design. |
| `docs/superpowers/plans/2026-05-27-witmcc-slice13-causal-edge-inference.md` | Slice-13 TDD plan. |
| `docs/superpowers/specs/2026-05-27-witmcc-slice14-insight-l1-design.md` | Slice-14 design. |
| `docs/superpowers/plans/2026-05-27-witmcc-slice14-insight-l1.md` | Slice-14 TDD plan. |
| `docs/superpowers/specs/2026-05-27-witmcc-slice15-insight-l2-infra-design.md` | Slice-15 design. |
| `docs/superpowers/plans/2026-05-27-witmcc-slice15-insight-l2-infra.md` | Slice-15 TDD plan. |
| `docs/superpowers/specs/2026-05-27-witmcc-slice16-insight-l2-categories-design.md` | Slice-16 design. |
| `docs/superpowers/plans/2026-05-27-witmcc-slice16-insight-l2-categories.md` | Slice-16 TDD plan. |
| `docs/superpowers/specs/2026-05-27-witmcc-slice17-mcp-streamable-http-design.md` | Slice-17 design. |
| `docs/superpowers/plans/2026-05-27-witmcc-slice17-mcp-streamable-http.md` | Slice-17 TDD plan. |
| `docs/superpowers/specs/2026-05-27-witmcc-slice18-redaction-gate-design.md` | Slice-18 design. |
| `docs/superpowers/plans/2026-05-27-witmcc-slice18-redaction-gate.md` | Slice-18 TDD plan. |
| `docs/superpowers/specs/2026-05-27-witmcc-slice19-auth-retention-design.md` | Slice-19 design. |
| `docs/superpowers/plans/2026-05-27-witmcc-slice19-auth-retention.md` | Slice-19 TDD plan. |

Each design spec follows the slice-1..slice-10a template (motivation, scope, real-data invariants, schema, deviations index). Each TDD plan follows the slice-10a plan template (phased red-first tasks with explicit commit boundaries and self-check blocks).
