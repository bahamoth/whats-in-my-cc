# UX Redesign Epic — Deferral Charter & Data-Contract Preconditions

**Date:** 2026-05-27
**Status:** Epic charter — **not a slice**. No TDD plan. No code lands under this document.
**Companion roadmap:** `2026-05-27-witmcc-remaining-milestones-roadmap.md` §6.
**Why this file exists:** the milestone path (slice-11 ~ slice-19) deliberately ships **zero new UI surface**. This document records the deferral, lists the data-contract preconditions the eventual redesign will consume, and provides a smoke checklist that locks those contracts *now* — before the redesign begins — so the redesign starts on solid data.

---

## 1. Deferral decision

### What was decided (2026-05-27)

The remaining UI work from M4 — Why Panel, Resource Drawer, Source Views, dashboard re-do, file-lineage explorer — is **not** decomposed into milestone slices. It is collected into a single later epic.

### Why this is correct

- The current WebUI (slice-2/8/9/10a) was built incrementally for engineering verification, not as a user-facing design. Adding panels piecemeal (Why Panel as slice-11, Resource Drawer as slice-12, etc.) would lock partial choices into production code that the eventual redesign would then have to undo.
- The product UX bar — "user sees execution replay, not chat transcript, and every claim has clickable evidence" (PRD §1) — is a coherent design problem, not a sum-of-panels problem. Information architecture, navigation, state shape, and rendering all interact.
- Doing the redesign *after* the data plane is complete (post slice-16) means the redesign can consume **complete** findings, episodes, verification runs, and inferred edges. Doing it earlier would force the redesign to handle missing-data states differently from how it will handle them once the engine ships.

### What this deferral does NOT mean

- The milestone path **must not** narrow the data surface for the eventual redesign. Slice-11 ~ slice-19 are evaluated against §3 of this document as part of their PR review.
- "No UI surface" is not "no UI work". Bugs and regressions in the existing WebUI continue to be fixed as part of the slices that introduce them. The existing copy-and-glue dashboard remains usable for engineering verification through MVP.

---

## 2. What the eventual redesign needs (rough scope)

This is **not** a design spec. It is the list of UX problems the redesign must address. The redesign's own spec will own the actual design choices.

| Problem | Constraint from current state | Note |
|---|---|---|
| Replay-first navigation | Current entry is session list → events table. Redesign should land on replay with timeline. | UI work, not data. |
| Why Panel | Click any graph node ⇒ evidence + claim + confidence + resource URI in a peripheral panel. | **Data must include `evidence_refs`, `confidence`, and `resource_uri` on every finding.** Slice-14 + slice-15 land this. |
| Resource Drawer | Open a finding ⇒ view evidence subgraph + raw source links + Pull API URL + MCP URI to copy. | Same data as Why Panel + an MCP URI. **Slice-17 lands the MCP URI surface.** |
| Source Views | Independent transcript / OTel / hook / file lineage views, switchable from any node. | Existing endpoints sufficient post slice-11/12. |
| Episode lane | Lane on the timeline labelled by episode phase. | **Slice-12 lands the `/v1/sessions/:id/episodes` endpoint.** |
| Inferred edges | Visually distinguish deterministic edges from inferred ones; click ⇒ rule_id + confidence. | **Slice-13 lands `inference_rule_id` + `confidence` on edges.** |
| File lineage view | diff hunk ⇒ tool call ⇒ verification trail. | **Slice-11 lands `covers_diff_hunk` edges; slice-13 inferred coverage if needed.** |
| Redaction preview | Show `redaction_state` + manifest summary at every raw source open. | **Slice-18 lands the manifest.** |
| Pending judge surface | Findings queued for L2 but budget-exhausted appear in a "pending judge" stub list. | **Slice-15 lands the `findings_pending_judge` table.** |

---

## 3. Data-contract preconditions (locked by milestone slices)

Each row here is a contract the milestone path **must** ship so the redesign has the optionality it needs. Each row also points to the slice that lands the contract and the smoke test that proves it ships green.

| # | Contract | Owning slice | Locked-by test (regression) |
|---|---|---|---|
| C1 | `GET /v1/sessions/:id/episodes` returns ordered episode list with phase + boundary node ids + confidence | slice-12 | `tests/api_episodes.rs::episodes_endpoint_returns_ordered_episodes_for_real_session` |
| C2 | `GET /v1/sessions/:id/verification-runs` returns rows; `covers_diff_hunk_ids` populated where applicable | slice-11 | `tests/api_verification_runs.rs::verification_runs_endpoint_covers_diff_hunks` |
| C3 | `GraphEdge.inference_rule_id` + `GraphEdge.confidence` columns present and non-null for inferred edges | slice-13 | `tests/graph_inferred_edges.rs::inferred_edges_carry_rule_id_and_confidence` |
| C4 | `GET /v1/findings` + `GET /v1/findings/:id` + `GET /v1/sessions/:id/findings` all return findings with `evidence_refs[]`, `confidence`, `category`, `severity` | slice-14 | `tests/api_findings.rs::findings_have_evidence_refs_confidence_category_severity` |
| C5 | `GET /v1/findings/:id/evidence` returns subgraph slice + raw source references | slice-14 | `tests/api_findings.rs::findings_evidence_endpoint_returns_subgraph_and_raw_refs` |
| C6 | `Finding.provenance.judge` is null for L1 findings, non-null for L2 findings | slice-15 | `tests/insight_provenance.rs::l1_finding_has_null_judge_l2_has_judge` |
| C7 | `findings_pending_judge` is queryable via `GET /v1/findings?status=pending` | slice-15 | `tests/api_findings.rs::pending_judge_findings_are_queryable_separately` |
| C8 | MCP resource `whats-in-my-cc://sessions/{id}` and tools `get_session_graph`, `search_findings`, `explain_node`, `get_file_lineage`, `get_otel_trace` all work against a stock MCP client | slice-17 | `tests/mcp_initialize.rs` + `tests/mcp_tools_list.rs` + `tests/mcp_resources_list.rs` |
| C9 | Every Pull API + MCP response carries a `redaction_manifest` block when underlying data has raw payload | slice-18 | `tests/redaction_manifest.rs::every_response_with_raw_payload_carries_manifest` |
| C10 | Bearer token required on Pull API + MCP; localhost still 200 with no Origin header given valid token | slice-19 | `tests/auth_token.rs::token_required_on_pull_api_and_mcp` |

If any row above is silently weakened during slice implementation (e.g., "we'll add `inference_rule_id` to edges later"), the redesign starts with a hole. The slice PRs reference this table by row number.

---

## 4. Smoke checklist (run after each milestone slice)

The smoke list below is **run every time** a slice that touches a precondition row lands. It is not a test file — it is a manual check, ~5 minutes, that confirms the contract row is observable end-to-end on the real `aac68973` transcript.

```
Smoke — Data Contract Preconditions for UX Redesign

[ ] curl /v1/sessions/aac68973/episodes | jq 'length' — non-zero, ordered, phase field set
[ ] curl /v1/sessions/aac68973/verification-runs | jq — non-empty if Bash test runs present; covers_diff_hunk_ids non-empty where verification ran after edits
[ ] curl /v1/sessions/aac68973/graph | jq '.data.edges | map(select(.inference_rule_id != null)) | length' — non-zero post slice-13
[ ] curl /v1/sessions/aac68973/findings | jq 'length' — non-zero post slice-14 (missing_verification + tool_failure expected)
[ ] curl /v1/findings/<id>/evidence | jq — evidence subgraph + raw source refs present
[ ] curl /v1/findings | jq 'map(select(.provenance.judge != null)) | length' — non-zero post slice-16
[ ] mcp inspector connect ws://127.0.0.1:4337/mcp; list tools and resources — all expected entries present post slice-17
[ ] curl /v1/sessions/aac68973/events | jq '.data.events[0].redaction_manifest' — non-null on first event with payload, post slice-18
[ ] curl -H "Authorization: Bearer $(cat ~/.config/witmcc/token)" /v1/health — 200 post slice-19; same call without header — 401
```

Each slice's plan checklist includes the subset of this block relevant to that slice. The full list is run as the **MVP-exit smoke** before declaring the programme done.

---

## 5. What the redesign explicitly will NOT inherit from current UI

The redesign is free to discard:

- The current `lanes` layout in `webui/src/components/Timeline.tsx`.
- The `SourcePanel` two-column layout in `webui/src/components/SourcePanel.tsx`.
- The dashboard summary cards in `webui/src/routes/SessionListPage.tsx`.
- The current routing (`/`, `/sessions/:id`) — a redesigned IA may use deeper paths.

The data layer (`webui/src/api/*.ts`, fetch + SSE primitives) **is** expected to survive; the redesign builds on the same Pull API. Tests in `webui/src/api/__tests__/*` are regression locks on the data layer and must remain green across the redesign.

---

## 6. Out of scope (do not allow back into milestone path)

These ideas surfaced in earlier conversations and are **explicitly** rejected from the milestone path:

- Adding Why Panel as a small standalone slice now ("we can replace it later").
- A `/v1/why/:node_id` endpoint independent from the existing finding/resource model.
- An "MVP UI" track parallel to the redesign.
- Custom dashboards or per-user views.
- Theming, dark mode, accessibility passes — all part of the redesign, not piecemeal slices.

If a future contributor proposes any of the above as a milestone slice, the proposal is rejected with a pointer to this section.

---

## 7. When the redesign starts

The redesign starts after **slice-19 merges** (MVP exit) **or** after a separate user decision to begin earlier with frozen partial data. Either path requires a new design spec (`YYYY-MM-DD-witmcc-ux-redesign-design.md`) that consumes this charter's §3 contracts as preconditions and §5 as the no-inheritance list.

The redesign is itself decomposed into slices using the same writing-plans pattern. It is **not** a single big PR.
