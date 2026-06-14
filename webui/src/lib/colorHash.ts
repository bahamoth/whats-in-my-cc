// Deterministic per-agent color = hash(agent_id) → stable palette. The SINGLE
// source of color truth for background-subagent identity: the hairline gutter
// rail, its ▢/✓ glyphs, and the SubagentGroup block header swatch all read this
// for the same agent so the color ties block ↔ gutter together. Distinct from
// the SEMANTIC --wimcc-lane-* tokens (those mean event kind, not identity).
//
// Tens of agents WILL collide across this fixed palette — that is accepted: the
// gutter is a calm presence indicator, and the authoritative re-confirmable
// identity is the block header (swatch + agent_id text), not the hue.

/** 8 readable hues (dark-bg first; mid-tones stay legible in light mode). */
export const AGENT_PALETTE = [
  '#7da7ff', // blue
  '#41c285', // green
  '#d97aff', // violet
  '#ff8a4c', // orange
  '#2bd0d0', // teal
  '#f0b429', // amber
  '#ef6f9c', // pink
  '#9d8bff', // periwinkle
] as const;

const NEUTRAL = 'var(--wimcc-fg-subtle, #6a7180)';

/** Stable color for an agent. null/'' (pre-0023 ingests / non-subagent) → neutral. */
export function agentColor(agentId: string | null | undefined): string {
  if (!agentId) return NEUTRAL;
  let h = 0;
  for (let i = 0; i < agentId.length; i++) h = (h * 31 + agentId.charCodeAt(i)) >>> 0;
  return AGENT_PALETTE[h % AGENT_PALETTE.length];
}
