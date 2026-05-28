/**
 * PR-7 — shared edge style helper. Both Waterfall and CausalGraph use
 * this so the visual contract for "deterministic vs inferred" cannot
 * drift between the two views.
 *
 *   deterministic ⇒ solid, strokeWidth 1.5, opacity 0.7
 *   inferred      ⇒ dashed "4 3", strokeWidth = 1 + 2*confidence,
 *                                 opacity     = 0.35 + 0.5*confidence
 *
 * Missing confidence on an inferred edge is treated as 0.5 (the
 * server-side default when no judge has run).
 */

export interface CausalEdgeInput {
  origin: string;
  confidence?: number | null;
}

export interface CausalEdgeStyle {
  strokeDasharray?: string;
  strokeWidth: number;
  opacity: number;
}

const INFERRED_CONF_FALLBACK = 0.5;

export function causalEdgeStyle(edge: CausalEdgeInput): CausalEdgeStyle {
  if (edge.origin === 'inferred') {
    const c = typeof edge.confidence === 'number' ? edge.confidence : INFERRED_CONF_FALLBACK;
    return {
      strokeDasharray: '4 3',
      strokeWidth: 1 + 2 * c,
      opacity: 0.35 + 0.5 * c,
    };
  }
  // deterministic (and anything unrecognised) gets the solid treatment.
  return { strokeWidth: 1.5, opacity: 0.7 };
}
