// ObservedEvent.kind is either native (directly observed from Claude Code)
// or derived (produced by wimcc's own pipeline: diff_hunk, verification_run,
// signal). Native → blue badge (원본); derived → purple badge (가공).
// (spec §3 층위 — 원본/가공 구분)
const DERIVED = new Set(['diff_hunk', 'verification_run', 'signal']);

export function eventProvenance(kind: string): { kind: 'native' | 'derived'; label: string } {
  return DERIVED.has(kind)
    ? { kind: 'derived', label: '가공' }
    : { kind: 'native', label: '원본' };
}
