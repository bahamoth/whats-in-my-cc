/**
 * insight-surface-redesign slice-7 — provenance vocabulary (design spec §2 P3).
 * Every surfaced value states how much to trust it. The long-form text lives in
 * each card's `?` tooltip; the badge shows the short Korean label.
 */
export type Provenance = 'measured' | 'mixed' | 'estimated' | 'uncollected';

export const PROVENANCE_LABEL: Record<Provenance, string> = {
  measured: '측정',
  mixed: '혼합',
  estimated: '추정',
  uncollected: '미수집·예정',
};
