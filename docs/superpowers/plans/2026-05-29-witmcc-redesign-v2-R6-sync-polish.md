# Redesign v2 — R6: Selection Sync + Memory Hardening + Final Smoke — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or executing-plans. Checkbox steps, TDD red→green→commit.

**Goal:** Close the redesign with bidirectional selection that actually brings the selected item into view, lock the high-volume memory bound with a regression test, confirm reduced-motion, and run a comprehensive end-to-end smoke across R1–R5.

**Context — what's already wired (do not rebuild):** Selection state is shared via `ReplaySelection.selectedNodeId`. Timeline click → `setSelectedNodeId` (R4); stream card click → `selectStreamCard` → `setSelectedNodeId` (R2); subgraph node click → `setSelectedNodeId` (R5); DetailPanel + stream highlight follow `selectedNodeId` (R2/R3). The remaining gap: when selection changes from the timeline or subgraph, the matching **stream card is highlighted but not scrolled into view** if it's off-screen. The stream only auto-scrolls to the bottom on new data (R2). This slice adds scroll-into-view. The conversation stream is virtualized (R2 `@tanstack/react-virtual` with a jsdom fallback) and the timeline has a density cap (R4); the windowed buffer caps at 5000 events. This slice adds an explicit regression test that the stream mounts a bounded number of cards for a large input.

**Tech Stack:** React 18, TypeScript, `@tanstack/react-virtual`, Vitest + Testing Library. No new deps.

**Spec:** §3 (stream sync), §5.7 (timeline↔stream sync), §7 (bounded rendering), §10 (reduced-motion). Resolves the remaining sync/polish items.

---

## File Structure

- **Modify** `webui/src/components/replay/stream/ConversationStream.tsx` (+ test) — scroll the selected card into view when `selectedEventId` changes to a card the user did not just click.
- **Modify** `webui/src/components/replay/stream/__tests__/ConversationStream.test.tsx` — add scroll-into-view + bounded-mount tests.
- **(Verify only)** reduced-motion gating in `Timeline.tsx` (R4) — add a test if not already covered.

---

### Task 1: ConversationStream scrolls the selected card into view (TDD)

When `selectedEventId` changes (e.g. user clicked a timeline node or a subgraph node), the stream should scroll that card into view. It must NOT fight the bottom-autoscroll for live appends, and must not yank the view when the user clicked the card themselves (already visible).

Design: add a `useEffect` keyed on `selectedEventId` that, when set and present in `cards`, scrolls the virtualizer to that index (`virtualizer.scrollToIndex(idx, { align: 'center' })`) in the virtual path, or in the jsdom/fallback path finds the card element by a `data-event-id` attribute and calls `scrollIntoView`. Guard: skip if `selectedEventId` is null. The bottom-autoscroll effect stays keyed on `cards.length` only, so the two don't conflict (selection change ≠ length change).

To make this assertable in jsdom (no layout, scrollIntoView is a no-op stub), add a `data-event-id` attribute to each card wrapper and assert the effect calls `scrollIntoView` on the right element via a spy. Also expose the scroll target choice deterministically.

- [ ] **Step 1: add failing tests** to `ConversationStream.test.tsx`:

```tsx
// add to webui/src/components/replay/stream/__tests__/ConversationStream.test.tsx
it('scrolls the selected card into view when selectedEventId changes', () => {
  const spy = vi.spyOn(Element.prototype, 'scrollIntoView').mockImplementation(() => {});
  const cards = [c('a', 'first'), c('b', 'second'), c('z', 'last')];
  const { rerender } = render(
    <ConversationStream cards={cards} selectedEventId={null} phaseByEventId={{}} findingEventIds={new Set()} onSelect={() => {}} />,
  );
  rerender(
    <ConversationStream cards={cards} selectedEventId="b" phaseByEventId={{}} findingEventIds={new Set()} onSelect={() => {}} />,
  );
  expect(spy).toHaveBeenCalled();
  spy.mockRestore();
});

it('does not scroll when selectedEventId is null', () => {
  const spy = vi.spyOn(Element.prototype, 'scrollIntoView').mockImplementation(() => {});
  const cards = [c('a', 'first')];
  render(
    <ConversationStream cards={cards} selectedEventId={null} phaseByEventId={{}} findingEventIds={new Set()} onSelect={() => {}} />,
  );
  expect(spy).not.toHaveBeenCalled();
  spy.mockRestore();
});

it('mounts a bounded number of cards for a large input (does not render all 2000)', () => {
  const many = Array.from({ length: 2000 }, (_, i) => c(`n${i}`, `msg ${i}`));
  render(
    <ConversationStream cards={many} selectedEventId={null} phaseByEventId={{}} findingEventIds={new Set()} onSelect={() => {}} />,
  );
  // In jsdom the virtualizer yields 0 items and we fall back to rendering all;
  // guard the §7 bound by asserting the fallback is itself capped.
  const mounted = screen.getAllByTestId('stream-card').length;
  expect(mounted).toBeLessThanOrEqual(300);
});
```

> Note on the bounded-mount test: the R2 jsdom fallback renders ALL cards when the virtualizer reports zero items, which would render 2000 and fail this test. So Task 1 must also **cap the fallback** (e.g. render at most the last `FALLBACK_CAP = 200` cards when not virtualizing) so the §7 bound holds even in the degenerate path. Newest-at-bottom means the last N are the relevant ones. Add `data-fallback-capped` when truncation happens, and surface a small "showing latest N" affordance is NOT required — just cap.

- [ ] **Step 2: run, verify the new tests fail** (scrollIntoView not called; 2000 mounted).

- [ ] **Step 3: implement** in `ConversationStream.tsx`:
  - Add `data-event-id={card.eventId}` to each card wrapper element (both virtual and fallback paths).
  - Add a `useEffect` on `[selectedEventId]`: if non-null and the index exists, in the virtual path call `virtualizer.scrollToIndex(idx, { align: 'center' })`; always also attempt `parentRef.current?.querySelector(`[data-event-id="${CSS.escape(selectedEventId)}"]`)?.scrollIntoView({ block: 'nearest' })` (works in browser; no-op stub in jsdom but the spy observes the call). Guard null.
  - Cap the fallback path: when `!useVirtual`, render `cards.slice(-FALLBACK_CAP)` (FALLBACK_CAP = 200) instead of all cards; set `data-fallback-capped="true"` on the container when truncated. (Virtual path already bounds the DOM.)
  - Keep the bottom-autoscroll effect keyed on `cards.length` only.

- [ ] **Step 4: run full stream test file** — all pass (existing + 3 new). Confirm the existing "renders one card per item in source order" test still holds (it uses 2 cards, under the cap).

- [ ] **Step 5: commit**

```bash
git add webui/src/components/replay/stream/ConversationStream.tsx webui/src/components/replay/stream/__tests__/ConversationStream.test.tsx
git commit -m "webui(redesign-v2) R6: scroll selected card into view + cap fallback render"
```

---

### Task 2: reduced-motion test for Timeline edge animation (TDD-verify)

Spec §10 requires the inferred-edge dash animation to be gated by `prefers-reduced-motion`. R4 implemented this (class gate + CSS media query). Lock it with a test if not already present.

**Files:** `webui/src/components/replay/timeline/__tests__/Timeline.test.tsx`.

- [ ] **Step 1:** Check whether a reduced-motion test already exists. If yes, this task is a no-op — record that and skip to Task 3. If no, add:

```tsx
it('does not animate inferred edges when prefers-reduced-motion is set', () => {
  // matchMedia is stubbed in tests; force reduced-motion = true
  const mql = { matches: true, media: '(prefers-reduced-motion: reduce)', addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {}, onchange: null, dispatchEvent: () => false } as unknown as MediaQueryList;
  const spy = vi.spyOn(window, 'matchMedia').mockImplementation(() => mql);
  const { container } = renderTL();
  const inferred = container.querySelector('[data-edge-id="e1"]');
  // animation class absent (or data-animated="false") under reduced motion
  expect(inferred?.classList.contains('animated') || inferred?.getAttribute('data-animated') === 'true').toBeFalsy();
  spy.mockRestore();
});
```
> Adapt to how `Timeline.tsx` actually exposes the animation (class name vs data-attr) and how it reads reduced-motion (`useMediaQuery` hook vs direct matchMedia). Read the component first; assert the real mechanism. If `useMediaQuery` is used, mock its return instead of matchMedia. The point: a reduced-motion test exists and passes.

- [ ] **Step 2-4:** make it pass (it should, since R4 gated it — if it fails, the gating is broken; fix `Timeline.tsx` to honor reduced-motion). **Step 5: commit** `webui(redesign-v2) R6: lock reduced-motion gating on timeline edges` (skip the commit if Task 1 already covered it / no-op).

---

### Task 3: Final full suite + comprehensive browser smoke (MVP-exit style)

- [ ] **Step 1:** `cd webui && npx vitest run && npx tsc --noEmit && npm run build` — all green. Record counts.
- [ ] **Step 2:** Rebuild + `cargo build` + `witmcc serve`. Walk the whole redesign on a real session (use one with findings + inferred edges, e.g. via KPI RISK > 0):
  - **Top bar:** single breadcrumb, no overlap.
  - **Layout:** KPI strip; 2-col main (stream left, detail right); full-width timeline at bottom; no empty band.
  - **Stream:** chat cards (User/Thinking/Assistant/Tool), newest at bottom, internal scroll, error/ok badges, episode chips, finding markers; scrolling up loads older (sentinel).
  - **Detail panel:** Insight/Detail/Raw tabs; default Insight (findings) / Detail (none); token badge in Detail; Raw tree expansion persists across an SSE tick / refetch.
  - **Timeline:** time axis, zoom +/−/fit, drag-pan, minimap brush, episode band, hover tooltip; inferred edges dashed + rule label on focus, deterministic solid; wheel-zoom does NOT scroll the page.
  - **Cross-sync:** click a timeline node → the matching stream card highlights AND scrolls into view, and the DetailPanel updates; click a stream card → the timeline node highlights; click a subgraph neighbor → selection moves everywhere.
  - **Subgraph:** Insight tab shows the focused neighborhood, center marked, hop 1↔2 toggle.
- [ ] **Step 3:** Capture 2–3 screenshots for the user to re-judge (esp. the focused subgraph per spec §11). Fix any issue found (re-enter the relevant task). When clean, the redesign R1–R6 is complete.

---

## Self-Review

- **Spec coverage:** §3/§5.7 stream↔timeline sync now brings the selected card into view (Task 1) on top of the already-wired highlight; §7 bound locked by the bounded-mount test + fallback cap (Task 1); §10 reduced-motion locked by a test (Task 2); comprehensive smoke (Task 3) validates R1–R5 end-to-end.
- **Placeholder scan:** Task 1 has full test code + concrete implementation steps against the real R2 component (virtual + fallback paths, the exact effect + cap). Task 2 is verify-or-add with adaptation guidance. No TBD.
- **Type consistency:** `ConversationStream` props unchanged (`cards/selectedEventId/phaseByEventId/findingEventIds/onSelect`); only internal behavior added. `data-event-id`, `data-fallback-capped` are new attributes used consistently between impl and tests.
- **Open risk:** the bottom-autoscroll vs scroll-into-view interaction — both effects can fire on the same render; keying bottom-autoscroll on `cards.length` and scroll-into-view on `selectedEventId` keeps them independent (a length change is a new message → bottom; a selection change → center the selected card). Verified in the smoke.
