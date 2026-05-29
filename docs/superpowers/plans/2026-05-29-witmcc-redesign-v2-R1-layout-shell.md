# Redesign v2 — R1: Layout Shell + Single Top Bar — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restructure the session-detail page into the approved grid — single breadcrumb top bar, KPI strip, a two-column main (left/right placeholders), and a full-width bottom region — removing the duplicate "Sessions" link and the Waterfall∥SourcePanel `.split`.

**Architecture:** A new `TopBar` breadcrumb replaces the in-page `<header>` duplicate link. `SessionDetailPage` switches from the `.split` (3fr 2fr Waterfall∥SourcePanel) to a CSS-grid skeleton with named slots (`kpi`, `stream`, `detail`, `timeline`). R1 keeps the existing `Waterfall` mounted in the `timeline` slot full-width and the existing `SourcePanel` in the `detail` slot so the page stays functional; later slices (R2 stream, R3 detail tabs, R4 timeline) replace each slot's contents. `ViewToggle` and the whole-session `CausalGraph` are removed in R1 because the toggle has no second view after the redesign.

**Tech Stack:** React 18, TypeScript, Vite, Vitest + Testing Library (jsdom), CSS Modules, react-router-dom v6. jsdom implements no layout, so grid contracts are asserted via `data-*` attributes (existing project convention, see `AppShell.test.tsx`).

**Spec:** `docs/superpowers/specs/2026-05-29-witmcc-ux-redesign-v2-design.md` §2, §8 (layout regression), resolves feedback #7 (empty bottom) and #8 (top overlap).

---

## File Structure

- **Create** `webui/src/components/layout/TopBar.tsx` — breadcrumb bar (`witmcc / Sessions / <sessionId>`). One responsibility: top-of-page navigation context.
- **Create** `webui/src/components/layout/TopBar.module.css` — breadcrumb styling using existing design tokens.
- **Create** `webui/src/components/layout/__tests__/TopBar.test.tsx` — behavior lock.
- **Modify** `webui/src/routes/SessionDetailPage.tsx` — remove in-page `<header>` duplicate link + `ViewToggle` + `CausalGraph` branch; mount `TopBar`; replace `.split` with the grid skeleton (Waterfall → `timeline` slot, SourcePanel → `detail` slot).
- **Modify** `webui/src/routes/SessionDetailPage.module.css` — replace `.split`/`.header`/`.viewToggleRow` with the grid.
- **Modify** `webui/src/routes/__tests__/SessionDetailPage.test.tsx` — add the layout-regression assertions (single Sessions link; grid slots present).
- **Delete** `webui/src/components/replay/ViewToggle.tsx`, `ViewToggle.module.css`, `__tests__/ViewToggle.test.tsx`.
- **Delete** `webui/src/components/replay/CausalGraph.tsx`, `CausalGraph.module.css`, `__tests__/CausalGraph.test.tsx` (whole-session graph; the focused subgraph lands fresh in R5).

> Note for the engineer: `CausalGraph` is removed now even though R5 introduces a *focused* subgraph, because R5's component has a different contract (neighborhood slice, not full payload) and will be written from scratch. Keeping the dead full-graph file would only rot.

---

### Task 1: TopBar breadcrumb component

**Files:**
- Create: `webui/src/components/layout/TopBar.tsx`
- Create: `webui/src/components/layout/TopBar.module.css`
- Test: `webui/src/components/layout/__tests__/TopBar.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// webui/src/components/layout/__tests__/TopBar.test.tsx
/**
 * R1 RED — TopBar is the single breadcrumb at the top of the session page.
 * It replaces the in-page "← Sessions" header that duplicated the nav rail.
 * See plan R1 Task 1 / spec §2 (#8 top overlap).
 */
import { render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { MemoryRouter } from 'react-router-dom';
import { TopBar } from '../TopBar';

function renderBar(sessionId = 'aac68973') {
  return render(
    <MemoryRouter>
      <TopBar sessionId={sessionId} />
    </MemoryRouter>,
  );
}

describe('TopBar', () => {
  it('renders a breadcrumb navigation landmark', () => {
    renderBar();
    expect(screen.getByRole('navigation', { name: /breadcrumb/i })).toBeInTheDocument();
  });

  it('links "Sessions" back to the list', () => {
    renderBar();
    const nav = screen.getByRole('navigation', { name: /breadcrumb/i });
    const link = within(nav).getByRole('link', { name: /sessions/i });
    expect(link).toHaveAttribute('href', '/sessions');
  });

  it('shows the current session id as the trailing crumb (not a link)', () => {
    renderBar('aac68973');
    const nav = screen.getByRole('navigation', { name: /breadcrumb/i });
    const current = within(nav).getByText('aac68973');
    expect(current.closest('a')).toBeNull();
    expect(current).toHaveAttribute('aria-current', 'page');
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd webui && npx vitest run src/components/layout/__tests__/TopBar.test.tsx`
Expected: FAIL — `Cannot find module '../TopBar'`.

- [ ] **Step 3: Write minimal implementation**

```tsx
// webui/src/components/layout/TopBar.tsx
import { Link } from 'react-router-dom';
import styles from './TopBar.module.css';

interface TopBarProps {
  sessionId: string;
}

export function TopBar({ sessionId }: TopBarProps) {
  return (
    <nav className={styles.bar} aria-label="Breadcrumb">
      <ol className={styles.crumbs}>
        <li>
          <Link to="/sessions" className={styles.link}>
            Sessions
          </Link>
        </li>
        <li aria-hidden="true" className={styles.sep}>
          /
        </li>
        <li>
          <code className={styles.current} aria-current="page">
            {sessionId}
          </code>
        </li>
      </ol>
    </nav>
  );
}
```

```css
/* webui/src/components/layout/TopBar.module.css */
.bar {
  display: flex;
  align-items: center;
  padding: 8px 0 12px;
  border-bottom: 1px solid var(--witmcc-border, #2c313a);
  margin-bottom: 12px;
}
.crumbs {
  display: flex;
  align-items: center;
  gap: 8px;
  list-style: none;
  margin: 0;
  padding: 0;
}
.link {
  color: var(--witmcc-accent, #61afef);
  text-decoration: none;
}
.link:hover {
  text-decoration: underline;
}
.sep {
  color: var(--witmcc-muted, #5c6370);
}
.current {
  color: var(--witmcc-text, #abb2bf);
  font-family: var(--witmcc-mono, ui-monospace, monospace);
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd webui && npx vitest run src/components/layout/__tests__/TopBar.test.tsx`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add webui/src/components/layout/TopBar.tsx webui/src/components/layout/TopBar.module.css webui/src/components/layout/__tests__/TopBar.test.tsx
git commit -m "webui(redesign-v2) R1: TopBar breadcrumb component"
```

---

### Task 2: Layout-regression test for SessionDetailPage (RED)

This task writes the failing assertions that lock the new layout before we change the page. It extends the existing test file; reuse its current render harness (it already mocks the queries — read the top of the file first to match the existing setup).

**Files:**
- Modify: `webui/src/routes/__tests__/SessionDetailPage.test.tsx`

- [ ] **Step 1: Read the existing test file to learn its render/mock harness**

Run: `sed -n '1,80p' webui/src/routes/__tests__/SessionDetailPage.test.tsx`
You must reuse whatever provider/mock wrapper it already defines (TanStack Query client + MemoryRouter + fetch mocks). Do not invent a new harness.

- [ ] **Step 2: Add the failing layout-regression tests**

Append this `describe` block, adapting `renderPage(...)` to the file's existing helper name:

```tsx
// append to webui/src/routes/__tests__/SessionDetailPage.test.tsx
describe('R1 layout shell', () => {
  it('renders exactly one link to /sessions (no duplicate header link)', async () => {
    await renderPage('aac68973'); // use the file's existing helper
    const sessionLinks = screen
      .getAllByRole('link')
      .filter((a) => a.getAttribute('href') === '/sessions');
    expect(sessionLinks).toHaveLength(1);
  });

  it('exposes named grid slots for stream, detail, and timeline', async () => {
    const { container } = await renderPage('aac68973');
    expect(container.querySelector('[data-slot="stream"]')).not.toBeNull();
    expect(container.querySelector('[data-slot="detail"]')).not.toBeNull();
    expect(container.querySelector('[data-slot="timeline"]')).not.toBeNull();
  });

  it('does not render the Waterfall/Graph ViewToggle', async () => {
    await renderPage('aac68973');
    expect(screen.queryByRole('button', { name: /graph/i })).toBeNull();
    expect(screen.queryByRole('tab', { name: /graph/i })).toBeNull();
  });
});
```

> If the existing helper is synchronous or named differently (e.g. `renderDetail`), match it. The assertions are what matter.

- [ ] **Step 3: Run to verify it fails**

Run: `cd webui && npx vitest run src/routes/__tests__/SessionDetailPage.test.tsx -t "R1 layout shell"`
Expected: FAIL — two `/sessions` links (header + nav rail), no `data-slot` attributes yet.

- [ ] **Step 4: Commit the red test**

```bash
git add webui/src/routes/__tests__/SessionDetailPage.test.tsx
git commit -m "webui(redesign-v2) R1: failing layout-shell regression tests"
```

---

### Task 3: Restructure SessionDetailPage to the grid skeleton

**Files:**
- Modify: `webui/src/routes/SessionDetailPage.tsx:137-205` (the returned JSX)
- Modify: `webui/src/routes/SessionDetailPage.module.css`

- [ ] **Step 1: Replace the imports (remove ViewToggle/CausalGraph, add TopBar)**

In `SessionDetailPage.tsx`, delete the `CausalGraph` lazy import (lines ~10-12), the `ViewToggle, useReplayView` import (line ~13), and add:

```tsx
import { TopBar } from '../components/layout/TopBar';
```

Also remove the `const view = useReplayView();` line inside `SessionDetailInner`.

- [ ] **Step 2: Replace the returned JSX**

Replace the `return (...)` block (currently lines ~137-205) with:

```tsx
  return (
    <div className={styles.page}>
      <TopBar sessionId={sessionId} />

      {isLoading && <p>Loading…</p>}

      {is404 && (
        <p>
          Session not found. <Link to="/sessions">Back to list</Link>
        </p>
      )}

      {!isLoading && !is404 && detail.data && (
        <div className={styles.grid} data-witmcc-detail-grid>
          <div className={styles.kpi} data-slot="kpi">
            <KpiStrip
              outcome={outcome}
              verificationCoverage={verificationCoverage}
              episodeCount={episodes.data?.length ?? 0}
              riskCount={riskCount}
            />
            <EpisodeStrip episodes={episodes.data ?? []} />
            <MetaStrip session={detail.data} events={window_.events} />
          </div>

          <div className={styles.stream} data-slot="stream">
            <div
              ref={sentinelRef}
              aria-hidden
              style={{ height: 1 }}
              data-testid="scroll-sentinel"
            />
            {/* R2 replaces this slot with ConversationStream. */}
            <p className={styles.placeholder}>Conversation stream (R2)</p>
          </div>

          <div className={styles.detail} data-slot="detail">
            {/* R3 replaces this slot with the tabbed DetailPanel. */}
            <SourcePanel eventId={selectedEventId} node={selectedNode} />
          </div>

          <div className={styles.timeline} data-slot="timeline">
            <Waterfall
              graph={effectiveGraph}
              selectedNodeId={sel.selectedNodeId}
              onSelect={(id) => sel.setSelectedNodeId(id)}
            />
          </div>
        </div>
      )}

      {!isLoading && !is404 && !detail.data && detail.isError && (
        <p role="alert">{detail.error?.message ?? 'failed'}</p>
      )}

      <WhyPanel
        open={sel.whyPanelOpen}
        finding={selectedFinding}
        evidence={evidenceQuery.data}
        onClose={sel.closeWhyPanel}
        onEvidenceHover={sel.setHoveredNodeId}
      />
    </div>
  );
```

> `WhyPanel` stays for now (R3 absorbs it into the Insight tab). `Link` is still imported for the 404 branch.

- [ ] **Step 3: Replace the CSS**

In `SessionDetailPage.module.css`, delete `.header`, `.viewToggleRow`, `.split`, and add:

```css
.page { padding: 24px; }

.grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  grid-template-areas:
    "kpi      kpi"
    "stream   detail"
    "timeline timeline";
  gap: 16px;
  margin-top: 8px;
}
.kpi { grid-area: kpi; }
.stream { grid-area: stream; min-height: 320px; overflow: auto; }
.detail { grid-area: detail; min-height: 320px; overflow: auto; }
.timeline { grid-area: timeline; }
.placeholder { color: var(--witmcc-muted, #5c6370); }

@media (max-width: 860px) {
  .grid {
    grid-template-columns: 1fr;
    grid-template-areas:
      "kpi"
      "stream"
      "detail"
      "timeline";
  }
}
```

- [ ] **Step 4: Run the layout-shell tests**

Run: `cd webui && npx vitest run src/routes/__tests__/SessionDetailPage.test.tsx`
Expected: PASS including the `R1 layout shell` block. If older tests in this file referenced `ViewToggle`/`split`, update them to the new structure (they are in the same file).

- [ ] **Step 5: Typecheck + build**

Run: `cd webui && npx tsc --noEmit && npm run build`
Expected: no errors. (If `tsc` flags the now-unused `useReplayView`/`CausalGraph`, you missed a Step-1 deletion.)

- [ ] **Step 6: Commit**

```bash
git add webui/src/routes/SessionDetailPage.tsx webui/src/routes/SessionDetailPage.module.css webui/src/routes/__tests__/SessionDetailPage.test.tsx
git commit -m "webui(redesign-v2) R1: grid skeleton + TopBar, drop split/ViewToggle"
```

---

### Task 4: Delete the dead ViewToggle and whole-session CausalGraph

**Files:**
- Delete: `webui/src/components/replay/ViewToggle.tsx`, `ViewToggle.module.css`, `__tests__/ViewToggle.test.tsx`
- Delete: `webui/src/components/replay/CausalGraph.tsx`, `CausalGraph.module.css`, `__tests__/CausalGraph.test.tsx`

- [ ] **Step 1: Confirm nothing still imports them**

Run: `cd webui && grep -rn "ViewToggle\|CausalGraph" src --include=*.tsx --include=*.ts | grep -v "__tests__/ViewToggle\|__tests__/CausalGraph\|replay/ViewToggle\|replay/CausalGraph"`
Expected: no output. If anything prints, fix that importer first.

- [ ] **Step 2: Delete the files**

```bash
cd webui
git rm src/components/replay/ViewToggle.tsx src/components/replay/ViewToggle.module.css src/components/replay/__tests__/ViewToggle.test.tsx
git rm src/components/replay/CausalGraph.tsx src/components/replay/CausalGraph.module.css src/components/replay/__tests__/CausalGraph.test.tsx
```

- [ ] **Step 3: Full test run + build**

Run: `cd webui && npx vitest run && npx tsc --noEmit && npm run build`
Expected: all green, no dangling references.

- [ ] **Step 4: Commit**

```bash
git commit -m "webui(redesign-v2) R1: remove ViewToggle + whole-session CausalGraph"
```

---

### Task 5: Browser smoke (CLAUDE.md mandate)

UI changes are not done on `vitest` alone. Verify in the real app before considering R1 complete.

- [ ] **Step 1: Start the server**

Run (from repo root): `witmcc serve` (default `--auth off`). Note the URL.

- [ ] **Step 2: Navigate and visually verify** (use claude-in-chrome tools)

Open `http://127.0.0.1:<port>/sessions`, click a session. Confirm:
- Exactly one "Sessions" affordance leads back (no overlapping duplicate at top).
- KPI strip on top; left "Conversation stream (R2)" placeholder; right SourcePanel; **full-width timeline (Waterfall) at the bottom** — no empty bottom band.
- No Waterfall/Graph toggle control.

- [ ] **Step 3: Record outcome**

If anything is off, fix and re-run Tasks 3–5. When clean, R1 is done.

---

## Self-Review

- **Spec coverage:** R1 covers spec §2 layout skeleton, #7 (timeline fills the bottom), #8 (single top bar). #1–#6 are explicitly deferred to R2–R5 (slots are placeholders). No spec requirement assigned to R1 is left unimplemented.
- **Placeholder scan:** The `Conversation stream (R2)` text is an intentional, labeled slot placeholder for a later slice — not a plan placeholder; every R1 step has concrete code/commands.
- **Type consistency:** `TopBar` prop is `{ sessionId: string }` in both the test and impl; `data-slot` values (`kpi`/`stream`/`detail`/`timeline`) match between CSS `grid-template-areas`, the JSX, and the regression test.

---

## Next slices (written when reached, same TDD format)

- **R2** — `ConversationStream` + `StreamCard` (event→card model, tool_use/tool_result merge, 8 inline fields, lucide-react icons, virtualization). Fills the `stream` slot.
- **R3** — `DetailPanel` tabs (Insight absorbs `WhyPanel` / Detail / Raw) + controlled JSON tree expansion persistence (#2). Fills the `detail` slot; removes `SourcePanel` placement + `WhyPanel` drawer.
- **R4** — `Timeline` time-series (axis, zoom/pan, minimap brush, episode band, focus edge emphasis, edge-type encoding) replacing `Waterfall` in the `timeline` slot.
- **R5** — `FocusedInsightGraph` (1–2 hop neighborhood) inside the Insight tab.
- **R6** — stream↔timeline bidirectional sync + memory/virtualization hardening + reduced-motion gating + MVP-style smoke.
