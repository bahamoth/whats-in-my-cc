/**
 * PR-7 RED — edge style is the visual contract for "inferred vs
 * deterministic" causality. The same formula is used by both Waterfall
 * (which already renders edges) and CausalGraph (React Flow). Locked
 * formula:
 *   deterministic ⇒ solid, strokeWidth=1.5, opacity=0.7
 *   inferred ⇒ dashed "4 3", strokeWidth=1 + 2*confidence, opacity=0.35 + 0.5*confidence
 */
import { describe, expect, it } from 'vitest';
import { causalEdgeStyle } from '../causalEdgeStyle';

describe('causalEdgeStyle', () => {
  it('deterministic edge: solid, strokeWidth 1.5, opacity 0.7', () => {
    const s = causalEdgeStyle({ origin: 'deterministic' });
    expect(s.strokeDasharray).toBeUndefined();
    expect(s.strokeWidth).toBeCloseTo(1.5);
    expect(s.opacity).toBeCloseTo(0.7);
  });

  it('inferred edge with confidence=1 ⇒ dashed, strokeWidth 3, opacity 0.85', () => {
    const s = causalEdgeStyle({ origin: 'inferred', confidence: 1 });
    expect(s.strokeDasharray).toBe('4 3');
    expect(s.strokeWidth).toBeCloseTo(3);
    expect(s.opacity).toBeCloseTo(0.85);
  });

  it('inferred edge with confidence=0 ⇒ dashed, strokeWidth 1, opacity 0.35', () => {
    const s = causalEdgeStyle({ origin: 'inferred', confidence: 0 });
    expect(s.strokeDasharray).toBe('4 3');
    expect(s.strokeWidth).toBeCloseTo(1);
    expect(s.opacity).toBeCloseTo(0.35);
  });

  it('missing confidence on inferred edge ⇒ treated as 0.5 fallback', () => {
    const s = causalEdgeStyle({ origin: 'inferred' });
    expect(s.strokeWidth).toBeCloseTo(2);
    expect(s.opacity).toBeCloseTo(0.6);
  });

  it('unknown origin treated as deterministic', () => {
    const s = causalEdgeStyle({ origin: 'wat' as 'deterministic' });
    expect(s.strokeDasharray).toBeUndefined();
  });
});
