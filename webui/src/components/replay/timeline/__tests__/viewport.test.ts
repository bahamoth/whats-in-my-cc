// webui/src/components/replay/timeline/__tests__/viewport.test.ts
/** R4 RED — viewport pan/zoom/fit math. Plan R4 Task 2. */
import { describe, expect, it } from 'vitest';
import { fit, zoomAt, pan, clamp, type Viewport } from '../viewport';

const FULL: [number, number] = [1000, 2000];

describe('viewport', () => {
  it('fit returns the full extent', () => {
    expect(fit(FULL)).toEqual({ t0: 1000, t1: 2000 });
  });
  it('zoomAt with factor <1 narrows the window around the focus time', () => {
    const v: Viewport = { t0: 1000, t1: 2000 };
    const z = zoomAt(v, 0.5, 1500); // zoom in 2x centered at 1500
    expect(z.t1 - z.t0).toBeCloseTo(500);
    expect((z.t0 + z.t1) / 2).toBeCloseTo(1500);
  });
  it('zoomAt keeps the focus time fixed in proportion', () => {
    const v: Viewport = { t0: 1000, t1: 2000 };
    const z = zoomAt(v, 0.5, 1250); // focus at 25% of window
    const beforeFrac = (1250 - v.t0) / (v.t1 - v.t0);
    const afterFrac = (1250 - z.t0) / (z.t1 - z.t0);
    expect(afterFrac).toBeCloseTo(beforeFrac);
  });
  it('pan shifts the window by a time delta', () => {
    expect(pan({ t0: 1000, t1: 2000 }, 100)).toEqual({ t0: 1100, t1: 2100 });
  });
  it('clamp keeps the window within the full extent and preserves width when possible', () => {
    const c = clamp({ t0: 1800, t1: 2800 }, FULL);
    expect(c.t1).toBe(2000);
    expect(c.t0).toBe(1000); // width 1000 preserved, shifted back into extent
  });
  it('clamp never produces a window wider than the extent', () => {
    const c = clamp({ t0: 0, t1: 5000 }, FULL);
    expect(c).toEqual({ t0: 1000, t1: 2000 });
  });
});
