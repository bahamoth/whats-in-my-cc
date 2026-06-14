import { describe, it, expect } from 'vitest';
import { isNearTop, shouldLoadOlder, shouldLoadNewer, shouldAdjustOnItemResize, LOAD_OLDER_TOP_PX } from '../scrollAnchor';

describe('isNearTop', () => {
  it('true at the very top', () => {
    expect(isNearTop({ scrollHeight: 5000, scrollTop: 0, clientHeight: 800 })).toBe(true);
  });
  it('true within the top threshold', () => {
    expect(isNearTop({ scrollHeight: 5000, scrollTop: LOAD_OLDER_TOP_PX - 1, clientHeight: 800 })).toBe(true);
  });
  it('false once scrolled down past the threshold', () => {
    expect(isNearTop({ scrollHeight: 5000, scrollTop: LOAD_OLDER_TOP_PX + 200, clientHeight: 800 })).toBe(false);
  });
});

describe('shouldLoadOlder — page older history only on an upward near-top user scroll', () => {
  const base = { hasInteracted: true, canLoadOlder: true };

  it('true: interacted, scrolling up, near top, older pages remain', () => {
    expect(shouldLoadOlder({ ...base, scrollTop: 20, prevScrollTop: 400 })).toBe(true);
  });

  // The cascade / mount guard: before the reader interacts, the initial
  // pin-to-bottom and any programmatic scroll must NOT page older history
  // (that was the auto-load-the-whole-session bug).
  it('false: not yet interacted (mount / programmatic scroll)', () => {
    expect(shouldLoadOlder({ ...base, hasInteracted: false, scrollTop: 0, prevScrollTop: 400 })).toBe(false);
  });

  // Excludes the native anchorTo:'end' re-anchor + initial bottom-pin, which
  // scroll DOWN — so a load can never re-trigger itself into a cascade.
  it('false: scrolling DOWN (anchorTo re-anchor / bottom-pin), even near the top', () => {
    expect(shouldLoadOlder({ ...base, scrollTop: 20, prevScrollTop: 5 })).toBe(false);
  });

  it('false: scrolling up but not near the top', () => {
    expect(shouldLoadOlder({ ...base, scrollTop: 2000, prevScrollTop: 3000 })).toBe(false);
  });

  it('false: no older pages remain (canLoadOlder=false)', () => {
    expect(shouldLoadOlder({ ...base, canLoadOlder: false, scrollTop: 20, prevScrollTop: 400 })).toBe(false);
  });

  // Prefetch-ahead: trigger BEFORE the reader hits the absolute top, so the
  // next older page is prepended while there is still content above to scroll
  // into — removing the "load, stuck at top, scroll down then up to re-trigger"
  // dance. The default zone must be ~a viewport, not a thin 240px strip.
  it('true: prefetches well above the absolute top (upward, ~600px from top)', () => {
    expect(shouldLoadOlder({ ...base, scrollTop: 600, prevScrollTop: 900 })).toBe(true);
  });

  it('default prefetch zone is roughly a viewport (>= 800px)', () => {
    expect(LOAD_OLDER_TOP_PX).toBeGreaterThanOrEqual(800);
  });

  // Stuck-at-top recovery (2026-06-11): fast upward scroll + a slow older-fetch
  // drop the near-top trigger (loadOlder no-ops while a prior load is in
  // flight); the reader reaches the ABSOLUTE top (scrollTop 0) and an
  // under-anchored prepend leaves them there. At scrollTop 0 you cannot produce
  // `scrollTop < prevScrollTop`, AND no further scroll event fires (you cannot
  // scroll past the top) — so the strict upward-delta rule can never re-trigger
  // and older history stops loading until you scroll DOWN then UP. Being pinned
  // at the absolute top must therefore page older history regardless of delta.
  it('true: pinned at the absolute top even with no upward delta (stuck-at-top recovery)', () => {
    expect(shouldLoadOlder({ ...base, scrollTop: 0, prevScrollTop: 0 })).toBe(true);
  });

  it('false: at the absolute top but not yet interacted (mount on a short session)', () => {
    expect(shouldLoadOlder({ ...base, hasInteracted: false, scrollTop: 0, prevScrollTop: 0 })).toBe(false);
  });

  it('false: at the absolute top but no older pages remain', () => {
    expect(shouldLoadOlder({ ...base, canLoadOlder: false, scrollTop: 0, prevScrollTop: 0 })).toBe(false);
  });
});

describe('shouldAdjustOnItemResize — measurement growth must not eat upward scroll (2026-06-11 freeze)', () => {
  // Live-captured freeze: a wheel batch moved scrollTop -1000, then the
  // ResizeObserver measurement of a row STRADDLING the viewport top fired
  // applyScrollAdjustment(+delta) and returned the viewport to its origin —
  // net zero movement, perceived as "scrolling up stops". The core default
  // (itemStart < offset, guarded by a scrollDirection that races to null
  // after the wheel ends) adjusts for straddling rows; the geometric rule
  // below adjusts ONLY for rows entirely above the viewport.
  it('adjusts when the resized row sits ENTIRELY above the viewport top', () => {
    expect(
      shouldAdjustOnItemResize({ itemEnd: 500, scrollOffset: 700, scrollAdjustments: 0 }),
    ).toBe(true);
  });
  it('does NOT adjust when the resized row straddles the viewport top (reader is inside it)', () => {
    expect(
      shouldAdjustOnItemResize({ itemEnd: 7300, scrollOffset: 6400, scrollAdjustments: 0 }),
    ).toBe(false);
  });
  it('includes pending scrollAdjustments in the viewport-top frame', () => {
    // offset 600 + pending +500 → effective top 1100; row ending at 1000 is above.
    expect(
      shouldAdjustOnItemResize({ itemEnd: 1000, scrollOffset: 600, scrollAdjustments: 500 }),
    ).toBe(true);
  });
});

describe('shouldLoadNewer — page NEWER history on a downward near-bottom user scroll (forward paging)', () => {
  const base = { hasInteracted: true, canLoadNewer: true, scrollHeight: 5000, clientHeight: 1000 };
  it('fires when scrolling DOWN into the near-bottom zone', () => {
    // distanceFromBottom = 5000 - (3800+1000) = 200 (within zone), scrolling down
    expect(shouldLoadNewer({ ...base, scrollTop: 3800, prevScrollTop: 3400 })).toBe(true);
  });
  it('fires when pinned at the absolute bottom regardless of direction', () => {
    expect(shouldLoadNewer({ ...base, scrollTop: 4000, prevScrollTop: 4000 })).toBe(true); // distance 0
  });
  it('does NOT fire when scrolling UP (excludes the live-append/anchor)', () => {
    expect(shouldLoadNewer({ ...base, scrollTop: 3900, prevScrollTop: 4000 })).toBe(false);
  });
  it('does NOT fire far from the bottom', () => {
    expect(shouldLoadNewer({ ...base, scrollTop: 1000, prevScrollTop: 900 })).toBe(false);
  });
  it('does NOT fire without interaction or when no newer pages remain', () => {
    expect(shouldLoadNewer({ ...base, hasInteracted: false, scrollTop: 4000, prevScrollTop: 4000 })).toBe(false);
    expect(shouldLoadNewer({ ...base, canLoadNewer: false, scrollTop: 4000, prevScrollTop: 4000 })).toBe(false);
  });
});
