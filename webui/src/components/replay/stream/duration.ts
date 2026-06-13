// webui/src/components/replay/stream/duration.ts
// Shared duration formatting for stream surfaces (ActivityStack, SubagentGroup).

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  const m = Math.floor(ms / 60_000);
  const s = Math.round((ms % 60_000) / 1000);
  return `${m}m ${s}s`;
}

// Heat tiers so long-running work reads at a glance: most CC tool executions
// finish well under 10s, so ≥10s is notable (warn) and ≥60s clearly long (hot).
// Absolute thresholds (not session-relative) keep the colour meaning stable
// across sessions. Rendered as `data-heat` on duration cells; CSS colours them.
export const HEAT_WARN_MS = 10_000;
export const HEAT_HOT_MS = 60_000;

export type DurationHeat = 'warn' | 'hot';

export function durationHeat(ms: number | null | undefined): DurationHeat | null {
  if (ms == null || ms < HEAT_WARN_MS) return null;
  return ms >= HEAT_HOT_MS ? 'hot' : 'warn';
}
