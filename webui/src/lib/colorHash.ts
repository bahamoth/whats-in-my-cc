// Deterministic per-agent color = hash(agent_id) → stable palette. The SINGLE
// source of color truth for background-subagent identity: the hairline gutter
// rail, its ▢/✓ glyphs, and the SubagentGroup block header swatch all read this
// for the same agent so the color ties block ↔ gutter together. Distinct from
// the SEMANTIC --wimcc-lane-* tokens (those mean event kind, not identity).
//
// Tens of agents WILL collide across this fixed palette — that is accepted: the
// gutter is a calm presence indicator, and the authoritative re-confirmable
// identity is the block header (swatch + agent_id text), not the hue.

/** 8 distinct, vivid hues tuned for legibility as THIN rails / small bars on the
 *  dark bg (brighter + better-separated than the prior mid-tones); still readable
 *  in light mode. Distinct from the human accent + semantic --wimcc-lane-* tokens. */
export const AGENT_PALETTE = [
  '#6db3ff', // blue
  '#4fd08a', // green
  '#c98cff', // violet
  '#ff9d54', // orange
  '#33d2cf', // teal
  '#f5c542', // amber
  '#ff7ea8', // pink
  '#9f93ff', // periwinkle
] as const;

const NEUTRAL = 'var(--wimcc-fg-subtle, #6a7180)';

/** Stable color for an agent. null/'' (pre-0023 ingests / non-subagent) → neutral. */
export function agentColor(agentId: string | null | undefined): string {
  if (!agentId) return NEUTRAL;
  let h = 0;
  for (let i = 0; i < agentId.length; i++) h = (h * 31 + agentId.charCodeAt(i)) >>> 0;
  return AGENT_PALETTE[h % AGENT_PALETTE.length];
}
