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
//   2. scrolling UP — excludes the native anchorTo:'end' re-anchor (which
//      scrolls DOWN to keep the viewport stable on a prepend) and the initial
//      bottom-pin, so a load never re-triggers itself into a cascade.
//   3. near the top — only the zone where older history belongs.
// A time-based "recent gesture" window was tried first but missed loads when
// smooth-scroll momentum outlived the window; the interaction latch + direction
// is robust to scroll cadence.
//
// Live-append follow + prepend-anchor are handled natively by
// @tanstack/react-virtual's `anchorTo: 'end'` + `followOnAppend`, so those
// concerns no longer live here.

/** Distance (px) from the top within which scrolling pages in older history. */
export const LOAD_OLDER_TOP_PX = 240;

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
 *  scrolling UP (so the anchorTo:'end' re-anchor and the initial bottom-pin —
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
