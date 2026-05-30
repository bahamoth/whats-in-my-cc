# Insight Surface Redesign — From KPI Tiles to a 5-Question Efficiency Diagnostic

**Date:** 2026-05-30
**Status:** Design spec — approved 2026-05-30; §11 decisions resolved; proceeding to implementation plan (writing-plans).
**Supersedes (UI only):** the current `KpiStrip` (6 tiles: outcome · verification · episodes · risk · cost · latency) shipped in redesign PR-3. The conversation stream, detail panel, and timeline from `2026-05-29-witmcc-ux-redesign-v2-design.md` survive unchanged; this spec only rethinks the **top insight surface** (the KPI strip + the colored episode bar) and the **collection** that feeds it.
**Charter:** `2026-05-27-witmcc-ux-redesign-epic.md`. Honors its no-inheritance list and the project non-goals (no improvement patches, no annotation/correction model, read-only, local-first).

---

## 1. Problem statement (user feedback, 2026-05-30)

The current KPI strip was the trigger. Walking through it surfaced three classes of failure, confirmed against real data (session `653ea169`, this very work session):

1. **Over-summarized abstractions that mislead.** "Risk: N" is a single count. In `653ea169` it would read **1953** — but 1941 of those (99%) are internal `StructuredOutput` schema-retry cycles from workflow subagents, plus benign `Read`/`grep` non-zero exits. The real user-visible tool-failure count is **~28**. A number you must explain away ("it's actually mostly noise") is not a metric.
2. **Broken / incomplete collection.** Verification reads **0%** for this session despite heavy TDD, because the closed 16-pattern allowlist plus `normalise_command` (cuts at the first `&&`, keeping the leading `cd`) cannot see `cd webui && npx vitest run` or `npx tsc -b`. `cost` and `latency` are unwired placeholders.
3. **Wrong unit of insight.** A raw count ("Episodes: ~1900") duplicates and is less honest than the phase bar already below it; and the user does not actually want scores — they want answers to diagnostic questions.

The user then stated the **actual purpose** of the tool — five questions it should answer:

- **Q1 — Efficiency.** How efficiently am I using the AI agent? Where does inefficiency occur, and what should I fix?
- **Q2 — Cost/token waste.** Where is cost/token waste, and *what causes it*? High-tier model? Excessive input? Mass tool calls? Too many sub-agents?
- **Q3 — Time.** How long did solving this take, and *why* if long? API latency? Trial-and-error? Tool failures from outages?
- **Q4 — Was it actually solved?** If "solved" is fuzzy, quantify it: how many *guards* (tests/builds/lints/checks) ran, and how many passed?
- **Q5 — Prompt/instruction accumulation.** System-prompt and agent instructions keep stacking. Is there a quantitative way to monitor stale/contaminated accumulated context?

This spec reorganizes the surface around these five questions.

---

## 2. Design principles (locked in brainstorming)

- **P1 — No correction-needing abstraction.** If a headline number must be explained away ("really it's mostly noise") it is dropped or reframed. Drop: Risk score, Episodes count, Outcome 3-state, latency p95 (this session). (Confirmed against `653ea169` real data.)
- **P2 — Measured-or-concrete only.** Every surfaced value is either (a) directly computed from observed data, or (b) a drill-able evidence-linked concrete fact. No single score that fuses heterogeneous, low-trust events.
- **P3 — Provenance on every value.** A text badge states how much to trust it: **측정** (directly computed) / **혼합** (tool-match high-confidence + keyword guess) / **추정** (heuristic) / **미수집·예정** (data exists at source but parsing not yet implemented). Long form in a `?` hover/click tooltip.
- **P4 — Surface the gaps honestly.** What is *not* derivable (Q3 API latency, Q5 semantic staleness) is shown as a stated limitation, not hidden or faked.
- **P5 — No auto-remediation (non-goal).** Q1's "what to fix" is answered by *pointing at the evidence/spot* (e.g., "this file was re-read 10×", "this input invalidated the cache"), never by generating a patch or prescribing a fix. Read-only insight; the user decides.

---

## 3. The five questions → answer design

Each question maps to a headline answer (with provenance badge) plus a drill-down to concrete evidence. All numbers below are from `653ea169` and are **live-variable** (the session was in progress); they are illustrative anchors, not frozen invariants.

### Q1 — Efficiency: where is the inefficiency? ✅ mostly derivable

- **Shows:** redundant tool calls (e.g., `SessionDetailPage.tsx` Read ×10, `routes.rs` ×9), cache misses (§Q5), sub-agent overhead share (§Q2), exploration/drift wandering.
- **Data:** `observed_event` tool_call counts grouped by input path/command (DB, present today); cache from the new usage facet (§6.1).
- **Drill (P5):** clicking a signal lists the concrete events (which file, which turns) — evidence pointing at the spot, no prescribed fix.
- **Badge:** 측정 (counts) / 미수집·예정 (cache-derived parts).
- **Limit:** "drift" as an efficiency signal is unreliable until the episode-classifier bug (§7) is fixed.

### Q2 — Cost/token waste attribution: what causes high cost? ✅ strongest, most actionable

This is the **centerpiece**. Instead of one "cost" number, a **decomposition by cause** (a small attribution breakdown):

| Cause | This session | Source |
|---|---|---|
| **Model tier** | opus 769 turns · haiku 332 turns | `assistant_message.payload.model` (DB, present) |
| **Sub-agents / multi-agent** | sidechain events **5916 / 7399 (80%)**, StructuredOutput 1967 | `is_sidechain`, tool_name Agent/Workflow/Task |
| **Tool-call volume** | per-tool counts | `observed_event` (present) |
| **Input / cache** | billed ~5.4M vs cache-read ~199.5M (free) | usage facet (§6.1) |

- **Headline:** *Billed ~5.4M tokens* (input 237K + cache_creation 3.9M + output 1.3M) and *cache-read 199.5M (free)* shown **separately** — never summed (the old "197M billed-in" was a category error: cache_read is not billed).
- **Drill:** "biggest cost driver" → e.g., "80% of events were sub-agent; opus carried N% of output tokens."
- **Badge:** 측정 (model/tool/subagent counts) / 미수집·예정 (token split until facet lands) / 추정 (dollar cost — see §6.5).
- **Note:** `653ea169`'s 80% sub-agent share is atypical (the audit workflows run *for this very design* inflated it); the metric's derivation is sound regardless.

### Q3 — Time: how long, and why? 🟡 partial (honest about the gap)

- **Total:** ~9.75h wall-clock (event timestamps `15:12 → 00:57`). Computed from `observed_event.observed_at`, **not** from episode durations (those are corrupted by the classifier bug — an earlier analysis mis-derived "289h" from them).
- **Attribution (derivable):** trial-and-error (repair/retry patterns), tool-failure time (is_error events + timing), idle gaps.
- **Attribution (NOT derivable — P4):** **per-call API latency** requires OTel trace spans; `653ea169` has **0** events with `latency_ms`/`trace_id`/`span_id` (transcript-ingested session, no OTLP). This is stated as a limitation, and only surfaces if/when OTel traces are ingested.
- **Badge:** 측정 (total, gap decomposition) / 미수집 (API latency).

### Q4 — Was it solved? Guards run and passed. ✅ derivable (with the verification rewrite)

Reframes the fuzzy "did it work" into **guard coverage + pass rate** — directly the user's "정량적으로 얼마나 많은 가드가 있었고 통과했나."

- **Shows:** guards detected by kind (test / build / lint / format), pass / fail / unknown, and change↔guard linkage (were code changes followed by a guard?).
- **Detection (new, §6.2):** segment-split the Bash command on `&& || | ; &`; Tier-1 match known tools after stripping wrappers (`npx`, `pnpm dlx`, `bunx`, `poetry run`, …) → `detection_basis = known_tool` (높음); Tier-2 fallback keyword (`test`/`spec`) → `detection_basis = test_keyword` (추정/guess); status from `tool_result.is_error`, conservatively `unknown` when the segment is piped (exit code masked).
- **This session:** would detect `npx vitest run` ×34, `npm test` ×25 (currently 0).
- **Badge:** 혼합 (도구매칭 측정 + 키워드 추정). Pass/fail carries a `status_basis` (exit / piped→unknown).
- **Limit (P4):** non-Bash guards — browser smoke (`mcp__claude-in-chrome__*`), MCP/IDE runners, sub-agent tests — are **not detected**; completeness is never claimed.

### Q5 — Accumulated/contaminated context monitoring 🟡 quantitative proxy only

- **Shows (measured):** **fixed cached-context size per turn** (median ~288K, max ~558K tokens — 29–56% of a 1M window held constant every turn), its **growth trajectory**, and **cache-miss events** (~4: turns 48/90/497/579 where cache_read collapsed and was re-created — 1.19M lost / 0.67M recreated). These quantify "accumulation" and "what invalidated context."
- **Badge:** 측정 (from usage facet) / 미수집·예정 (until facet lands).
- **Limit (P4, important):** the usage object gives the **aggregate** cached-prefix size only. It does **not** decompose into system-prompt vs skills vs agents vs memory, and it cannot **judge** which instruction is "stale/contaminated." We can show size, growth, and churn; we cannot attribute or score staleness. Stated plainly on the surface.

---

## 4. Additional proposals (beyond the five questions)

The user invited better ideas. These are recommended; each is optional and flagged.

- **A. Cross-session baseline (recommended).** A single session's "98% cache-hit / 80% sub-agent / 9.75h" is unjudgeable in isolation. Compare against the user's own rolling median across their stored sessions → "this session's cache-hit is below your median," "sub-agent share 3× your norm." Turns raw numbers into signal; directly serves Q1/Q2. Data already present (many sessions in store). *Opt-in question for the user.*
- **B. Cost-attribution decomposition as Q2's primary view** (folded into §Q2 above) — the "where did it go" breakdown is more actionable than any single cost number.
- **C. Time-gap attribution for Q3** — decompose the 9.75h via inter-event timestamp gaps (generation vs tool-exec vs idle vs failure), since API spans are unavailable. Honest, derivable approximation.
- **D. Context-growth timeline for Q5** — a small trajectory (cached-prefix size over turns with cache-miss markers) instead of one number; the user *sees* accumulation.
- **E. Evidence-to-spot, not auto-fix (principle P5)** — reaffirm: Q1 answers "what to fix" by linking to the exact events; it never emits a patch (non-goal).

---

## 5. Surface layout (Direction A — compact strip + click-expand)

Chosen in brainstorming over a grouped dashboard (B) and headline+drawer (C). Minimal structural change, progressive disclosure.

```
┌──────────────────────────────────────────────────────────────────────┐
│ [컨텍스트 효율 98% ▼] [토큰 청구5.4M·캐시199.5M] [검증 도구N·키워드M] [도구실패(사용자) 28] [비용 ≈$0.09] │
│   측정/미수집           미수집·예정              혼합                측정              추정         │
│ ▼ expand: cache-hit, 고정 컨텍스트 288K/558K, 캐시 미스 4회(drill), …                    │
├──────────────────────────────────────────────────────────────────────┤
│ phase bar: action 77% · exploration 11% · drift* 8% · intake 4%   (*drift 보정 후 신뢰) │
└──────────────────────────────────────────────────────────────────────┘
```

- Each card: label · value · 1-line micro-detail · provenance badge · `?` tooltip. Click expands an inline detail panel (the question's drill-down) in place.
- Cards map to questions: 컨텍스트 효율 → Q1/Q5; 토큰 + 비용 → Q2; 검증 → Q4; 도구실패 → Q1/Q2; phase bar → Q1/Q3 context. Q3 total-time + Q5 growth live in the relevant expands.
- **Removed from the strip** (per P1): Risk score, Episodes count, Outcome 3-state, latency p95.
- Cross-session baseline (proposal A), if adopted, renders as a small "vs your median" delta under each measured value.

---

## 6. Data model & backend work ("integrity requires adjacent changes")

The user explicitly accepted that redesign may touch collection where integrity demands it. It does.

### 6.1 NEW — usage telemetry facet (largest new piece)

Parse `message.usage` from `assistant_message` events into a per-event **usage telemetry facet** (the `usage` object is 1:1 with an assistant message, so a `usage_facet` side-table keyed by `event_id` — distinct from the existing OTel `metric_sample` EventKind, which carries no rows here — rather than a new EventKind), plus a session-level aggregate (view or rollup table). Fields: `input_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`, `output_tokens`, `model`, `service_tier` (+ `ephemeral_1h/5m` if present). OTel-first and source-preserving per CLAUDE.md: carries `schema_version` + provenance and preserves the raw `usage` reference. Enables Q1, Q2, Q5. *Currently `src/` parses no cache fields; the data is already in the transcripts witmcc ingests.* New migration; `witmcc init-db` + re-ingest.

### 6.2 Verification detection rewrite (Q4)

Replace the closed allowlist match with: segment split → Tier-1 known-tool (wrapper-stripped, extensible seed) → Tier-2 keyword (guess) → `detection_basis` + `status_basis` columns on `verification_run`. Keyword-tier keeps a tiny non-exec denylist (`cat/echo/grep/git/rm/mkdir/cp/mv/ls/find`). Consciously revises the DEV-S11-03 "closed list" stance to "Tier-1 seed (real-fixture-locked) + Tier-2 fallback"; each Tier-1 addition still requires a real-fixture invariant test. The detector mines its own promotion backlog (Tier-2 hits = Tier-1 candidates), surfaced as a small maintenance view.

### 6.3 tool_failure reframe (Q1, Q2)

Split user-visible tool failures (~28: Bash/Read/browser/Edit) from internal auto-retries (~1941 `StructuredOutput` schema cycles) — tag the latter so it never enters a headline. Stop treating benign non-zero exits as "Risk."

### 6.4 Episode classifier drift bug (flagged, gates Q1 drift)

`classifier.rs:216-230` double-classifies events when `exploration_streak ≥ 8` (drift emitted while the same events re-enter exploration) → overlapping episodes, 0-duration / negative-gap rows, empty `evidence_node_ids`, and the corrupted session-duration. Fix needed before drift/episode-duration is trustworthy. (Also explains `missing_verification` false positives, which are an artifact of §6.2.)

### 6.5 Cost (Q2)

Prefer the OTel `claude_code.cost.usage` metric (parser exists in `src/ingest/otel_metrics.rs`, but no metric events have arrived for these sessions). Until then, derive a **추정** from usage tokens × a public pricing table, badged 추정 and never presented as actual billing.

---

## 7. Bugs surfaced (fix for integrity)

1. Verification detection blind to `npx` / compound `cd &&` / `tsc` → 0 runs despite heavy TDD (`verification_allowlist.rs:43-79`, `verification_run.rs:288-306`).
2. Risk/findings inflated by internal `StructuredOutput` retries + benign `is_error` (1941 of 1953 high findings).
3. Episode classifier drift double-classification (§6.4).
4. `missing_verification` false positives (1215) — artifact of #1.

---

## 8. Honest limitations (stated on the surface, P4)

- **Q3 API latency** — unavailable for transcript-ingested sessions (no OTel spans; `latency_ms` all NULL).
- **Q5 semantic staleness & per-component decomposition** — usage gives aggregate cached size only; cannot attribute to system-prompt/skills/agents/memory, cannot judge "contaminated."
- **Q4 non-Bash guards** — browser smoke / MCP / sub-agent tests not detected; no completeness claim.
- **All numbers are per-session and live-variable**; cross-session baseline (proposal A) is the remedy for "is this good?"

---

## 9. Testing approach (TDD red-first, per CLAUDE.md)

- **Backend, real-fixture-anchored:** usage-facet extraction asserted against frozen `tests/fixtures/transcripts/real/` usage objects; verification detection asserted against real `cd && npx vitest`, piped, and dry-run (`--no-run`) command fixtures (red first). New migrations verified by `init-db` + re-ingest.
- **Frontend:** contract tests for the new DTOs/props (jsdom can't test layout/CSS); provenance-badge rendering and drill-expand behavior as component tests; **browser smoke** (witmcc serve + claude-in-chrome) before commit, per the project's UI rule.
- **No metric is shipped to a headline without its provenance badge wired and its data path real (or explicitly 미수집·예정).**

---

## 10. Non-goals (reaffirmed)

No improvement patches / auto-fix (Q1 points at evidence only). No external correction/label/status write (no annotation model). No Claude Code settings/hooks/skills/memory modification. Read-only, local-first `127.0.0.1`.

---

## 11. Resolved decisions (approved 2026-05-30)

1. **Cross-session baseline (proposal A): adopted** — built as an enhancement slice *after* the core metric slices land (renders as a "vs your median" delta under each measured value).
2. **Q5 scope: quantitative proxy only** — cached-prefix size / growth / churn + cache-miss drill. Semantic staleness and per-component (system-prompt vs skills vs agents vs memory) decomposition are explicitly **out of scope** (not in the data).
3. **Cost: interim public-pricing 추정** — derived from usage tokens × a public pricing table, badged 추정, never shown as actual billing; replaced by the OTel `claude_code.cost.usage` metric if/when it arrives.
4. **Q4 guards: include build / lint / format, kind-separated** — shown by kind, not lumped with tests.
5. **Sequencing:** (1) usage telemetry facet §6.1 [unblocks Q1/Q2/Q5] → (2) verification detection rewrite §6.2 [Q4] → (3) tool_failure reframe §6.3 → (4) episode classifier drift fix §6.4 → (5) cost §6.5 → (6) cross-session baseline (proposal A). Frontend surface (Direction A + badges + drill) lands incrementally alongside the data each slice unblocks.
