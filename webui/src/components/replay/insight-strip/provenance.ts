/**
 * insight-surface-redesign slice-7 — provenance vocabulary (design spec §2 P3).
 * Every surfaced value states how much to trust it. The long-form text lives in
 * each card's `?` tooltip; the badge shows the short label. l10n — the label
 * text lives in the catalog under `insight.provenance.*`; this maps each
 * provenance to its catalog key so the badge can localize it via t().
 */
import type { MessageKey } from '../../../i18n';

export type Provenance = 'measured' | 'mixed' | 'estimated' | 'uncollected';

export const PROVENANCE_LABEL_KEY: Record<Provenance, MessageKey> = {
  measured: 'insight.provenance.measured',
  mixed: 'insight.provenance.mixed',
  estimated: 'insight.provenance.estimated',
  uncollected: 'insight.provenance.uncollected',
};
