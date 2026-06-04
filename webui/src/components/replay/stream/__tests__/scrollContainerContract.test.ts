// Regression guard for the nested-scroll bug (issue: scrollbars don't hide /
// stay stuck on smaller / higher-DPI viewports, and the "fetch more" UX is
// not reachable).
//
// Root cause that this locks out: there were TWO nested scroll containers —
// the grid slot `.stream` (overflow:auto) AND the virtualizer's own scroll
// element `.scroll` (overflow-y:auto + min-height:320px). On a short viewport
// the KPI strip eats most of the height, the `.stream` slot shrinks below
// 320px, but `.scroll`'s fixed `min-height:320px` refuses to shrink — so the
// slot ALSO becomes a scroll container. Two stacked scrollbars, and the
// virtualizer (whose scrollElement is `.scroll`) measures a 320px viewport
// while the reader actually sees ~84px, so its isAtEnd / load-older math is
// wrong. Measured live in a 423px-tall viewport: both `.stream` and `.scroll`
// reported scrollHeight > clientHeight.
//
// The contract: there is exactly ONE scroll container in the stream column —
// the inner virtualizer `.scroll`. The grid slot must NOT scroll, and `.scroll`
// must never be forced taller than its slot by a fixed min-height. jsdom has no
// layout, so this is locked by reading the CSS module text (same "lock the
// contract, verify pixels by browser smoke" approach as the sibling
// virtualizer-options tests).
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

// Read the CSS sources as text (vitest stubs CSS-module imports, so `?raw`
// would not yield a string there — node:fs is reliable in the node-backed test
// runtime; @types/node types it for the `tsc -b` build).
function block(css: string, selector: string): string {
  // Grab the first `<selector> { ... }` rule body (no nested braces in these files).
  const re = new RegExp(`\\${selector}\\s*\\{([^}]*)\\}`);
  const m = css.match(re);
  if (!m) throw new Error(`rule ${selector} not found`);
  return m[1];
}

// vitest runs with cwd = webui/, so resolve from the source tree root.
const detailCss = readFileSync(resolve(process.cwd(), 'src/routes/SessionDetailPage.module.css'), 'utf8');
const streamCss = readFileSync(
  resolve(process.cwd(), 'src/components/replay/stream/ConversationStream.module.css'),
  'utf8',
);
const globalCss = readFileSync(resolve(process.cwd(), 'src/styles/global.css'), 'utf8');

describe('stream column has a single scroll container (nested-scroll regression)', () => {
  it('the .stream grid slot is NOT itself a scroll container', () => {
    const stream = block(detailCss, '.stream');
    // The slot must not own overflow scrolling — the inner virtualizer does.
    expect(stream).not.toMatch(/overflow(-y)?\s*:\s*(auto|scroll)/);
  });

  it('the inner .scroll has no fixed min-height that can exceed its slot', () => {
    const scroll = block(streamCss, '.scroll');
    const m = scroll.match(/min-height\s*:\s*([^;]+)/);
    if (m) {
      const v = m[1].trim();
      // Only a 0 floor is allowed; a fixed px floor recreates the nested scroll
      // when the slot is shorter than it (short / high-DPI viewports).
      expect(v).toMatch(/^0(px)?$/);
    }
  });

  it('the inner .scroll is the scroll container (overflow-y auto/scroll)', () => {
    const scroll = block(streamCss, '.scroll');
    expect(scroll).toMatch(/overflow-y\s*:\s*(auto|scroll)/);
  });

  it('the inner .scroll reserves a stable gutter so the thumb does not shift content', () => {
    const scroll = block(streamCss, '.scroll');
    expect(scroll).toMatch(/scrollbar-gutter\s*:\s*stable/);
  });
});

// The scrollbar's VISUAL form lives once in global.css so every scroll region
// (stream, detail/raw panel, page, lists) is consistent — the reader asked for
// a single uniform style, not a stream-only one. The OS default here is a fixed
// 15px classic bar that always takes space; we want a thin bar whose thumb is
// transparent until the region is hovered / focused. Verified visually by
// browser smoke; the contract is locked here because jsdom has no layout.
describe('app-wide scrollbar form is consistent + auto-hiding (global)', () => {
  it('defines the thin auto-hide scrollbar globally (not per-component)', () => {
    // a global webkit thumb rule, transparent by default
    expect(globalCss).toMatch(
      /\*::-webkit-scrollbar-thumb\s*\{[^}]*background\s*:\s*transparent/,
    );
    // revealed on hover / focus-within with a real colour (not transparent)
    expect(globalCss).toMatch(
      /\*:(hover|focus-within)::-webkit-scrollbar-thumb[\s,]*[^{]*\{[^}]*background\s*:\s*var\(/,
    );
    // Firefox: thin + hidden until hover
    expect(globalCss).toMatch(/scrollbar-width\s*:\s*thin/);
  });

  it('does not redefine scrollbar visuals inside the stream module (stays DRY/global)', () => {
    // the per-component module must not carry its own ::-webkit-scrollbar rules,
    // otherwise the app-wide form drifts out of sync.
    expect(streamCss).not.toMatch(/::-webkit-scrollbar/);
  });
});

// The session-detail page must AUTO-FIT its scroll area (the AppShell `.main`):
// only the inner panels scroll, the page itself does not. The old layout sized
// the grid with a magic `height: calc(100vh - 56px)` that ignored the `.page`
// padding (and the real TopBar height), so `.page` overshot the viewport and
// `.main` grew a page-level scrollbar — and on a short viewport the overshoot
// pushed the bottom of the stream/detail below the fold, unreachable. The fix
// is a flex column that fills its container, so the grid takes exactly the
// space left after the TopBar. Verified by browser smoke (page not scrollable).
describe('session-detail page auto-fits (no page-level scrollbar)', () => {
  it('the grid fills via flex, not a hard-coded viewport-height calc', () => {
    const grid = block(detailCss, '.grid');
    expect(grid).not.toMatch(/height\s*:\s*calc\([^)]*100vh/);
    expect(grid).toMatch(/flex\s*:/);
    expect(grid).toMatch(/min-height\s*:\s*0/);
  });

  it('the page is a flex column bounded to its container height', () => {
    const page = block(detailCss, '.page');
    expect(page).toMatch(/display\s*:\s*flex/);
    expect(page).toMatch(/flex-direction\s*:\s*column/);
    expect(page).toMatch(/height\s*:\s*100%/);
  });
});
