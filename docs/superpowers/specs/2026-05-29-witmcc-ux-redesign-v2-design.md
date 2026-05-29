# UX Redesign v2 — Replay-First Layout, Conversation Stream, Time-Series Timeline, Focused Insight Graph

**Date:** 2026-05-29
**Status:** Design spec — approved direction, pending user review of this document.
**Supersedes (UI only):** the current `SessionDetailPage` `.split` layout (Waterfall ∥ SourcePanel) shipped in redesign PR-1~9.
**Charter:** `2026-05-27-witmcc-ux-redesign-epic.md`. This spec consumes that charter's §3 data contracts as preconditions and §5 as the no-inheritance list. The data layer (`webui/src/api/*`, fetch + SSE, TanStack Query hooks) survives unchanged.

---

## 1. Problem statement (user feedback, 2026-05-29)

The current session-detail UI was built incrementally for engineering verification and fails as a user-facing design. Concrete complaints:

1. **Raw JSON dominates.** `SourcePanel` (a `react-json-view-lite` dump) occupies 2/5 of the width and gives no insight. The raw record is the *most primitive* unit we collect — it should be viewable on demand, not granted prime real estate.
2. **JSON tree collapses on every refresh.** `JsonView` uses `shouldExpandNode={(level) => level < 1}` on an uncontrolled tree, so every SSE append / refetch resets expansion. Infuriating. The currently-viewed tree must keep its expansion across data refreshes.
3. **The wide right panel should show clearer information**, not raw JSON. Time-series data is wide; the timeline should be free to use full width.
4. **No chat-like reading of the run.** User input, assistant replies, thinking, and tool calls should read like a chat app — scrollable, newest stacked at the bottom.
5. **Waterfall doesn't look like a time series.** No time axis, no pan/zoom, no familiar navigation. Connection lines carry no information; on focus they could express *what kind of trigger* occurred via color/animation/type.
6. **Graph view is unusable.** Whole-session node dump; nodes can't be found or understood; purpose unclear.
7. **Bottom of the page is empty** for no reason.
8. **Top bar UI overlaps** (duplicate "Sessions" nav rail link + in-page "← Sessions" header).

---

## 2. Approved layout

A single session-detail page, top to bottom:

```
┌───────────────────────────────────────────────────────────┐
│ Top bar:  ◐ witmcc / Sessions / aac68973   (single breadcrumb) │
├───────────────────────────────────────────────────────────┤
│ KPI strip:  outcome · verification coverage · episodes · risk │
├──────────────────────────────┬────────────────────────────┤
│  Conversation Stream (left)   │  Detail Panel (right)        │
│  vertical, newest at bottom   │  tabs: Insight · Detail · Raw│
│  scrollable, virtualized      │  content for selected message│
├──────────────────────────────┴────────────────────────────┤
│  Timeline (bottom, full width)                              │
│  episode band · lanes · time axis · zoom/pan · minimap brush │
└───────────────────────────────────────────────────────────┘
```

Decisions locked during brainstorming:

- **Two-column main**, not full-width chat. Left = conversation stream (vertical flow). Right = detail of the clicked message.
- **Timeline at the bottom**, full width. (Resolves #3, #7.)
- **Single top breadcrumb bar** replaces the duplicate nav-rail link + in-page header. (Resolves #8.)
- Selecting a message in the stream, or a node on the timeline, drives the right Detail Panel and is **bidirectionally synced**.

This resolves #1 (raw demoted to one tab), #3, #4, #7, #8 structurally.

---

## 3. Conversation Stream (left)

Renders the run as chat-style cards, oldest→newest, **newest stacked at the bottom**, auto-scrolls to tip on live append (unless the user has scrolled up). Source: the existing windowed events (`useSessionWindow`, cursor-paged) + SSE bridge. Newest-at-bottom matches the existing append model.

### 3.1 Card types (from real transcript block types)

Real Claude Code transcript messages are `message.content` block arrays. Block types observed in `tests/fixtures/transcripts/real/`: `text`, `thinking`, `tool_use` (`name`, `input`), `tool_result` (`tool_use_id`, `content`, `is_error`); plus `message.usage` (token counts), top-level `timestamp`, `durationMs`, `gitBranch`, `isSidechain`. (Real-data anchoring: claims here are locked by invariant assertions on those frozen fixtures — see §8.)

| Card | Source block | Inline content |
|---|---|---|
| **User** | `type:user` with a `text`/string content (a real human prompt) | prompt text |
| **Thinking** | assistant `thinking` block | collapsed by default; word count; first-line preview; click → full in Detail |
| **Assistant** | assistant `text` block | response text |
| **Tool** | assistant `tool_use` block, **merged** with its matching `tool_result` (`user` message carrying `tool_result` for the same `tool_use_id`) | tool name + key arg summary; result folded (collapsed); success/error badge |

**Folding rule:** a `type:user` message whose content is *only* `tool_result` is NOT a human-prompt card. It is folded into the result of the Tool card that owns its `tool_use_id`.

### 3.2 Inline fields (all 8 adopted)

1. Actor icon + color (User / Assistant / Thinking / Tool) — **icons from `lucide-react`**, not emoji.
2. Timestamp + duration (`HH:MM:SS` · `durationMs`).
3. Body preview (1–2 lines; truncated, click → full in Detail).
4. Tool summary (name + key arg: Bash→`command`, Edit/Read→`file_path`).
5. Success/error badge (`tool_result.is_error` → red `error`).
6. Token badge (assistant `usage`: out / in / cache).
7. Insight marker (🔗-equivalent lucide icon when ≥1 finding references this node → click opens Insight tab).
8. Episode-phase chip (the `phase` of the episode containing this message; color-coded).

### 3.3 Icon library

`lucide-react` (Radix + Tailwind ecosystem standard, tree-shakeable). New dependency.

---

## 4. Detail Panel (right) — tabs

When a stream card or a timeline node is selected, the right panel shows three tabs. Tab selection persists per session (does not reset on data refresh).

- **Insight** — findings referencing this node (`evidence_refs`, `confidence`, `category`, `severity`, `summary`) **plus the focused insight graph** (§6). This subsumes the current standalone `WhyPanel` drawer.
- **Detail** — structured, human-readable fields: actor, tool name, tool input/output (rendered, not raw), duration, token usage breakdown, episode phase, git branch, linked diff hunks / verification runs.
- **Raw** — the existing JSON object (`react-json-view-lite`), now confined to this tab. **Its expansion state must persist across re-renders and data refreshes** (fix for #2): make the tree controlled, keyed by node id, with expansion state held in React state (or a controlled `react-json-view-lite` configuration), so SSE appends / refetches do not collapse it.

Default tab: **Insight** (the product's point — evidence-linked "why"), falling back to Detail when a node has no findings.

---

## 5. Timeline (bottom, full width) — time-series redesign

Replaces the static d3-scale Waterfall with a familiar time-series surface. All 8 affordances adopted:

1. **Time axis + gridlines** — `d3-time` ticks; granularity adapts to zoom (s / min / hr).
2. **Zoom / pan** — wheel zoom, drag pan, `fit` button.
3. **Minimap brush** — full-run overview with a draggable selection window driving the main view's range.
4. **Episode phase band** — colored phases (`intake`…`verification`) along the top, from `/v1/sessions/:id/episodes`.
5. **Focus edge emphasis** — selecting a node brightens only its in/out edges; the rest dim.
6. **Edge-type encoding** — deterministic = solid; inferred = dashed + flowing dash-offset animation, labeled with `inference_rule_id` and `confidence`. (Consumes slice-13 contract C3.)
7. **Stream sync** — timeline node click ↔ conversation stream scroll/select, both directions.
8. **Hover tooltip** — node summary (kind · time · duration · gist).

Lanes carry over the existing semantic grouping (Intent / Context / Action / State / Files / Hook / OTel / Quality) from `laneMapping.ts`.

---

## 6. Focused Insight Graph (replaces whole-session Graph view)

**Decision: remove the standalone whole-session `CausalGraph` view + `ViewToggle`.** Replace with a **focused causal subgraph** rendered inside the Detail Panel's Insight tab. When a node is selected, show only its 1–2 hop causal neighborhood (the node, what triggered it, what it caused), using the same edge encoding as §5.6. This is the "simplified insight graph"; the user will re-evaluate against the rendered result.

Rationale: the whole-session dump answered no question. The timeline now owns the time-series role; the focused subgraph owns the "why" role with a findable, comprehensible scope. Aligns with the charter's Why-Panel / evidence-subgraph intent (epic §2).

`@xyflow/react` + `dagre` are retained but driven with a small neighborhood slice instead of the full graph (much smaller render; helps §7).

---

## 7. Non-functional: memory under high event volume

A run can contain very many events. The design must stay bounded:

- **Conversation stream**: virtualized rendering (windowing) — only on-screen cards mount. Keep using the cursor-paged `useSessionWindow`; do not hold the entire event set as live React nodes.
- **Timeline**: level-of-detail aggregation per zoom level (bucket/aggregate off-screen and dense regions); prefer canvas (or a capped SVG node budget) over one DOM node per event. The minimap renders an aggregated overview, not every node.
- **Focused subgraph**: bounded by hop count, so it never renders the whole graph.
- **Raw tab**: render JSON only for the selected node, lazily.
- Reuse the existing windowed buffer + SSE gap/resync; no unbounded in-memory accumulation.

---

## 8. Testing strategy (TDD red-first)

Per CLAUDE.md, each unit lands test-first. Coverage targets:

- **Stream card mapping** — pure function: transcript event → card model (type, inline fields, fold rule for tool_result-only user messages). Locked by invariant assertions on `tests/fixtures/transcripts/real/{structured_patch_v01,verification_v01}.jsonl` (real-data anchoring; no field-meaning claim without a fixture assertion).
- **Tool call/result merge** — `tool_use` ↔ `tool_result` pairing by `tool_use_id`; `is_error` → error badge.
- **Raw tab expansion persistence** — expansion survives a simulated refetch/SSE append (the #2 regression lock).
- **Detail Panel tab persistence** — selected tab unchanged across data refresh.
- **Timeline** — axis tick generation; zoom/pan range math; brush→range; edge encoding (deterministic solid vs inferred dashed + rule_id/confidence label, consuming C3).
- **Focused subgraph** — neighborhood slice is bounded to N hops; contains the selected node + its inferred/deterministic edges.
- **Stream↔timeline sync** — selecting in one updates the other.
- **Virtualization** — only a bounded number of cards mount for a large synthetic event set.
- **Layout regression** — single top bar (no duplicate Sessions link).

Existing data-layer tests in `webui/src/api/__tests__/*` remain green (charter §5).

**UI smoke (CLAUDE.md mandate):** every increment is verified with `witmcc serve` + browser navigation (claude-in-chrome) + visual check before commit, not just `cargo build`/`vitest`.

---

## 9. Component inventory

**New / reworked (webui):**
- `TopBar` (breadcrumb) — replaces duplicate nav-rail link + in-page header.
- `ConversationStream` + `StreamCard` (User / Thinking / Assistant / Tool variants) + virtualization.
- `DetailPanel` with `InsightTab` (absorbs `WhyPanel`) / `DetailTab` / `RawTab`.
- `Timeline` (axis, zoom/pan, minimap brush, episode band, edge encoding) — replaces `Waterfall`.
- `FocusedInsightGraph` — replaces `CausalGraph` whole-session view.
- Controlled JSON tree (fix `JsonView` expansion persistence).

**Removed:**
- `ViewToggle` (no more Waterfall/Graph toggle).
- Standalone whole-session `CausalGraph` view.
- `SourcePanel` two-column placement (its diff/hook/otel structured sections migrate into `DetailTab`).

**Retained:** `KpiStrip`, `EpisodeStrip` (or folded into timeline band), data layer, TanStack Query hooks, SSE bridge, `useSessionWindow`, `causalEdgeStyle`.

**New dependency:** `lucide-react`.

---

## 10. Out of scope

- Backend / Pull API / MCP changes (data contracts already shipped; charter §3).
- New endpoints. The redesign consumes existing endpoints only.
- Theming/dark-mode/accessibility *passes* beyond what AppShell tokens already provide (charter §6) — though reduced-motion must gate the edge animation.

---

## 11. Open questions

- **Episode strip vs timeline band**: keep the separate `EpisodeStrip` row, or fold it entirely into the timeline's phase band? (Lean: fold, to reclaim vertical space.)
- **Focused subgraph hop count**: 1 hop vs 2 hops default. (Lean: 1 hop with expand control; decide against rendered result, per user.)
- **Stream/timeline split orientation on narrow screens**: stack vertically below a breakpoint (existing `useMediaQuery` available).
