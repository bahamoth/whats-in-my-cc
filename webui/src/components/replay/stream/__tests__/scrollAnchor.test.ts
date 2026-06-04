import { describe, it, expect } from 'vitest';
import { isNearTop, shouldLoadOlder, LOAD_OLDER_TOP_PX } from '../scrollAnchor';

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
});
