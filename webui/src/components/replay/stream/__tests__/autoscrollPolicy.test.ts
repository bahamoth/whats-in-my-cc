// Pure decision logic for the explicit autoscroll (stick-to-bottom) model.
// Extracted as pure functions so the policy is unit-testable without layout
// (jsdom has none); the DOM wiring lives in useAutoscroll and is verified by
// browser smoke — same split as scrollAnchor.ts.
import { describe, expect, it } from 'vitest';
import { isAtBottom, classifyChange, DEFAULT_BOTTOM_THRESHOLD } from '../autoscrollPolicy';

describe('isAtBottom', () => {
  const m = (scrollTop: number, scrollHeight: number, clientHeight: number) => ({
    scrollTop,
    scrollHeight,
    clientHeight,
  });

  it('is true exactly at the bottom (distance 0)', () => {
    expect(isAtBottom(m(900, 1000, 100))).toBe(true);
  });

  it('is true within the threshold of the bottom', () => {
    // distance = 1000 - 850 - 100 = 50 <= 80
    expect(isAtBottom(m(850, 1000, 100))).toBe(true);
  });

  it('is false beyond the threshold', () => {
    // distance = 1000 - 500 - 100 = 400 > 80
    expect(isAtBottom(m(500, 1000, 100))).toBe(false);
  });

  it('honours a custom threshold', () => {
    // distance = 400; with threshold 500 it counts as at-bottom
    expect(isAtBottom(m(500, 1000, 100), 500)).toBe(true);
  });

  it('treats a non-overflowing container (fits) as at-bottom', () => {
    // content shorter than viewport → distance negative → at bottom
    expect(isAtBottom(m(0, 80, 600))).toBe(true);
  });

  it('exposes a sane default threshold', () => {
    expect(DEFAULT_BOTTOM_THRESHOLD).toBeGreaterThan(0);
  });
});

describe('classifyChange', () => {
  const sig = (first: string | null, last: string | null, count: number) => ({ first, last, count });

  it('returns no movement when there is no previous signature (initial mount)', () => {
    expect(classifyChange(null, sig('a', 'c', 3))).toEqual({ appended: 0, prepended: false });
  });

  it('detects a pure tip append (last changed, first same) and counts it', () => {
    const prev = sig('a', 'c', 3);
    const next = sig('a', 'e', 5);
    expect(classifyChange(prev, next)).toEqual({ appended: 2, prepended: false });
  });

  it('detects a pure prepend (first changed, last same) and does not count it', () => {
    const prev = sig('c', 'e', 3);
    const next = sig('a', 'e', 6);
    expect(classifyChange(prev, next)).toEqual({ appended: 0, prepended: true });
  });

  it('reports nothing when the signature is unchanged', () => {
    const s = sig('a', 'e', 5);
    expect(classifyChange(s, { ...s })).toEqual({ appended: 0, prepended: false });
  });

  it('reports both when first and last both change (mixed) — counts the tip growth', () => {
    const prev = sig('b', 'd', 3);
    const next = sig('a', 'f', 6); // older prepended AND newer appended
    const r = classifyChange(prev, next);
    expect(r.prepended).toBe(true);
    expect(r.appended).toBe(3);
  });
});
