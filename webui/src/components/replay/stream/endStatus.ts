// Map a <task-notification> status string → a display kind + glyph label, shared
// by the subagent and workflow end cards so completed/failed/killed read the same
// everywhere. Unknown statuses pass through verbatim (kind 'other').
export type EndStatusKind = 'done' | 'fail' | 'killed' | 'other';

export function endStatusLabel(status: string): { kind: EndStatusKind; text: string } {
  const s = status.toLowerCase();
  if (s === 'completed' || s === 'success' || s === 'done') return { kind: 'done', text: `✓ ${status}` };
  if (s === 'failed' || s === 'error') return { kind: 'fail', text: `✗ ${status}` };
  if (s === 'killed' || s === 'cancelled' || s === 'canceled' || s === 'stopped')
    return { kind: 'killed', text: `⊘ ${status}` };
  return { kind: 'other', text: status };
}
