import { describe, it, expect } from 'vitest';
import { isNearBottom, nextStickState, STICK_THRESHOLD } from '../scrollAnchor';

describe('isNearBottom', () => {
  it('true when parked at the bottom', () => {
    expect(isNearBottom({ scrollHeight: 1000, scrollTop: 900, clientHeight: 100 })).toBe(true); // dist 0
  });
  it('true within the stick threshold', () => {
    expect(isNearBottom({ scrollHeight: 1000, scrollTop: 870, clientHeight: 100 })).toBe(true); // dist 30 < 48
  });
  it('false once scrolled up beyond the threshold', () => {
    expect(isNearBottom({ scrollHeight: 1000, scrollTop: 700, clientHeight: 100 })).toBe(false); // dist 200
  });
});

describe('nextStickState — only genuine user gestures change stick', () => {
  const atBottom = { scrollHeight: 1000, scrollTop: 900, clientHeight: 100 };
  const scrolledUp = { scrollHeight: 1000, scrollTop: 600, clientHeight: 100 };

  it('returns null (ignore) for a measurement/programmatic scroll long after the last user gesture', () => {
    // This is the "can't focus while streaming" guard: the virtualizer fires
    // synthetic scrolls as it measures rows; those must NOT re-engage autoscroll.
    expect(nextStickState(5000, 100, scrolledUp)).toBeNull(); // 4900ms > 200ms window
    expect(nextStickState(5000, 100, atBottom)).toBeNull();
  });
  it('follows (true) on a user scroll that lands at the bottom', () => {
    expect(nextStickState(300, 250, atBottom)).toBe(true); // 50ms <= 200ms window
  });
  it('anchors (false) on a user scroll up — keeps position, no follow', () => {
    expect(nextStickState(300, 250, scrolledUp)).toBe(false);
  });
  it('honors a custom gesture window + threshold', () => {
    expect(nextStickState(300, 250, scrolledUp, { gestureWindowMs: 30 })).toBeNull(); // 50ms > 30ms → ignore
    expect(STICK_THRESHOLD).toBeGreaterThan(0);
  });
});
