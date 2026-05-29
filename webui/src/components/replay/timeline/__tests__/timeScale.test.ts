// webui/src/components/replay/timeline/__tests__/timeScale.test.ts
/** R4 RED — time scale + adaptive ticks. Plan R4 Task 1. */
import { describe, expect, it } from 'vitest';
import { makeTimeScale, axisTicks } from '../timeScale';

const t0 = new Date('2026-05-28T00:00:00Z').getTime();
const t1 = new Date('2026-05-28T00:10:00Z').getTime();

describe('makeTimeScale', () => {
  it('maps domain start to range start and end to range end', () => {
    const s = makeTimeScale([t0, t1], [0, 600]);
    expect(s(t0)).toBeCloseTo(0);
    expect(s(t1)).toBeCloseTo(600);
  });
  it('maps the midpoint to the range midpoint', () => {
    const s = makeTimeScale([t0, t1], [0, 600]);
    expect(s((t0 + t1) / 2)).toBeCloseTo(300);
  });
});

describe('axisTicks', () => {
  it('returns ticks within the domain with x positions inside the range', () => {
    const s = makeTimeScale([t0, t1], [0, 600]);
    const ticks = axisTicks(s, 600);
    expect(ticks.length).toBeGreaterThan(0);
    for (const tk of ticks) {
      expect(tk.t).toBeGreaterThanOrEqual(t0);
      expect(tk.t).toBeLessThanOrEqual(t1);
      expect(tk.x).toBeGreaterThanOrEqual(0);
      expect(tk.x).toBeLessThanOrEqual(600);
      expect(typeof tk.label).toBe('string');
    }
  });
  it('produces more ticks for a wider pixel range', () => {
    const s1 = makeTimeScale([t0, t1], [0, 200]);
    const s2 = makeTimeScale([t0, t1], [0, 1200]);
    expect(axisTicks(s2, 1200).length).toBeGreaterThanOrEqual(axisTicks(s1, 200).length);
  });
});
