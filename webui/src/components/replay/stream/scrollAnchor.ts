// Decision logic for paging OLDER history into the conversation stream,
// extracted as a pure function so the "page older history only on a genuine
// upward user scroll near the top — never on a mount / measurement / re-anchor
// scroll" rule is unit-testable.
//
// This is the guard against the windowing cascade: previously an
// IntersectionObserver on a sentinel in a non-scrolling container re-fired on
// every render and auto-loaded the whole session. The stream now drives
// loadOlder from its own scroll, gated three ways:
//   1. hasInteracted — the reader has scrolled/clicked at least once, so the
//      initial pin-to-bottom and any pre-interaction programmatic scroll never
//      trigger a load.
//   2. scrolling UP — excludes the manual prepend re-anchor (ConversationStream
//      shifts scrollTop DOWN by the prepended height to hold the viewport) and
//      the initial bottom-pin, so a load never re-triggers itself into a cascade.
//   3. near the top — only the zone where older history belongs.
// A time-based "recent gesture" window was tried first but missed loads when
// smooth-scroll momentum outlived the window; the interaction latch + direction
// is robust to scroll cadence.
//
// Live-append follow is owned by the `useAutoscroll` hook; prepend anchoring is
// done manually in ConversationStream (scrollHeight-delta) — react-virtual's
// `anchorTo:'end'`/`followOnAppend` were removed (they jumped to the new top
// when a prepend landed at scrollTop≈0). Neither concern lives here.

/** Distance (px) from the top within which scrolling pages in older history.
 *  Sized to ~a viewport (not a thin strip) so the next older page is PREFETCHED
 *  before the reader reaches the absolute top — there is still content above to
 *  scroll into when it prepends, so upward reading stays seamless instead of
 *  "load → stuck at top → scroll down then up to re-trigger". ConversationStream
 *  may pass a larger, viewport-relative threshold on tall screens. */
export const LOAD_OLDER_TOP_PX = 800;

export interface ScrollMetrics {
  scrollHeight: number;
  scrollTop: number;
  clientHeight: number;
}

/** True when the viewport is near the top — the zone where older history
 *  should be paged in. */
export function isNearTop(m: ScrollMetrics, threshold = LOAD_OLDER_TOP_PX): boolean {
  return m.scrollTop <= threshold;
}

/** Whether a scroll event should trigger loading the next older window: the
 *  reader must have interacted (so mount/programmatic scrolls are excluded), be
 *  scrolling UP (so the manual prepend re-anchor and the initial bottom-pin —
 *  both downward — are excluded, preventing a self-retriggering cascade), land
 *  near the top, and older pages must still remain. */
export function shouldLoadOlder(args: {
  scrollTop: number;
  prevScrollTop: number;
  hasInteracted: boolean;
  canLoadOlder: boolean;
  topThreshold?: number;
}): boolean {
  if (!args.canLoadOlder || !args.hasInteracted) return false;
  if (args.scrollTop >= args.prevScrollTop) return false; // not scrolling up
  return args.scrollTop <= (args.topThreshold ?? LOAD_OLDER_TOP_PX);
}
