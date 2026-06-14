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

/** Whether the virtualizer may shift `scrollTop` to compensate a row's
 *  measured-size change (estimate 64px → real height). Passed to react-virtual
 *  as `shouldAdjustScrollPositionOnItemSizeChange`.
 *
 *  Geometric rule: adjust ONLY when the resized row sits ENTIRELY above the
 *  viewport top. Such a row's growth shifts everything at/below the viewport
 *  down by `delta`, so a `+delta` scroll adjustment keeps the view stable
 *  (this is what keeps manual prepend anchoring drift-free after the new
 *  page's rows measure). A row STRADDLING the viewport top is different: its
 *  own start is fixed, so the content at the viewport top does not move when
 *  it grows — adjusting would scroll the reader DOWN away from it.
 *
 *  This replaces virtual-core's default (`itemStart < offset`, guarded by
 *  `scrollDirection !== 'backward'`): the direction guard races — a wheel
 *  burst ends, direction resets to null, THEN the ResizeObserver delivers the
 *  giant straddling-row measurement, and the adjustment cancels the wheel's
 *  movement (live-captured 2026-06-11: -1000px wheel + ~+1000 adjustment →
 *  net zero, perceived as "위로 스크롤이 멈춤"). The geometric rule needs no
 *  direction tracking, so it cannot race. */
export function shouldAdjustOnItemResize(args: {
  /** The resized row's pre-resize end offset (start + old size). */
  itemEnd: number;
  scrollOffset: number;
  /** Adjustments already queued but not yet applied to `scrollOffset`. */
  scrollAdjustments: number;
}): boolean {
  return args.itemEnd <= args.scrollOffset + args.scrollAdjustments;
}

/** Distance (px) from the absolute top within which the viewport counts as
 *  "pinned at the top". At/under this, older history pages in regardless of
 *  scroll direction (see `shouldLoadOlder`). Small (sub-pixel + HiDPI slack)
 *  so it never overlaps the downward-near-top exclusion. */
export const AT_TOP_PX = 4;

/** Whether to trigger loading the next older window. The reader must have
 *  interacted (so mount/programmatic scrolls are excluded) and older pages must
 *  remain. Then it fires when EITHER:
 *    - pinned at the absolute top (`scrollTop <= AT_TOP_PX`), regardless of
 *      direction — because at the very top you cannot produce an upward delta
 *      and no further scroll event fires, so a dropped/under-anchored prepend
 *      would otherwise strand the reader there with older history un-loadable
 *      (the "scroll down then up to un-stick" freeze, 2026-06-11); or
 *    - scrolling UP into the near-top prefetch zone. The upward-delta guard
 *      still excludes the manual prepend re-anchor and the initial bottom-pin
 *      (both downward) away from the top, preventing a self-retriggering
 *      cascade — and the at-top branch does not reopen that cascade because a
 *      successful prepend anchor scrolls the reader DOWN off the top. */
export function shouldLoadOlder(args: {
  scrollTop: number;
  prevScrollTop: number;
  hasInteracted: boolean;
  canLoadOlder: boolean;
  topThreshold?: number;
}): boolean {
  if (!args.canLoadOlder || !args.hasInteracted) return false;
  if (args.scrollTop <= AT_TOP_PX) return true; // pinned at the absolute top
  if (args.scrollTop >= args.prevScrollTop) return false; // not scrolling up
  return args.scrollTop <= (args.topThreshold ?? LOAD_OLDER_TOP_PX);
}

/** Symmetric to LOAD_OLDER_TOP_PX, for the bottom edge. */
export const LOAD_NEWER_BOTTOM_PX = 800;
/** Symmetric to AT_TOP_PX, for the bottom edge. */
export const AT_BOTTOM_PX = 4;

/** Whether to page the next NEWER window in — FORWARD paging on a downward
 *  near-bottom user scroll. Mirror of `shouldLoadOlder`. This is what lets a
 *  reader who jumped into history (a `?selected=` deep-link, autoscroll OFF)
 *  scroll DOWN through the rest of the session toward the live tip; without it
 *  the only forward load was live-tip following, so a detached window stayed a
 *  stuck slice (the "스크롤 내려도 최신이 안 옴" bug). The caller gates this to
 *  the detached (not-following) case so it never competes with the autoscroll /
 *  SSE live-append path. */
export function shouldLoadNewer(args: {
  scrollTop: number;
  prevScrollTop: number;
  scrollHeight: number;
  clientHeight: number;
  hasInteracted: boolean;
  canLoadNewer: boolean;
  bottomThreshold?: number;
}): boolean {
  if (!args.canLoadNewer || !args.hasInteracted) return false;
  const distanceFromBottom = args.scrollHeight - (args.scrollTop + args.clientHeight);
  if (distanceFromBottom <= AT_BOTTOM_PX) return true; // pinned at the absolute bottom
  if (args.scrollTop <= args.prevScrollTop) return false; // not scrolling down
  return distanceFromBottom <= (args.bottomThreshold ?? LOAD_NEWER_BOTTOM_PX);
}
