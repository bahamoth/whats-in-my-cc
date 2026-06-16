// ObservedEvent.kind is either native (directly observed from Claude Code)
// or derived (produced by wimcc's own pipeline: diff_hunk, verification_run,
// signal). Native → blue badge; derived → purple badge. (spec §3 층위 — 원본/가공)
// l10n — only the provenance KIND is decided here; the badge text lives in the
// catalog under `detail.provenance.*` and is resolved by the consumer via t().
const DERIVED = new Set(['diff_hunk', 'verification_run', 'signal']);

export function eventProvenance(kind: string): { kind: 'native' | 'derived' } {
  return DERIVED.has(kind) ? { kind: 'derived' } : { kind: 'native' };
}
