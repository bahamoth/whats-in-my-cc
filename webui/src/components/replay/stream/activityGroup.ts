import type { ActivityEvent } from './streamModel';

export interface ActivityStackData { events: ActivityEvent[]; }
export interface StackSummary { count: number; topTools: string[]; errorCount: number; durationMs: number; }

export function summarizeStack(stack: ActivityStackData): StackSummary {
  const counts = new Map<string, number>();
  let errorCount = 0;
  let min = Infinity, max = -Infinity;
  for (const { event, result } of stack.events) {
    const name = event.tool_name ?? event.kind;
    counts.set(name, (counts.get(name) ?? 0) + 1);
    if (result?.isError) errorCount++;
    const t = new Date(event.observed_at).getTime();
    if (!Number.isNaN(t)) { min = Math.min(min, t); max = Math.max(max, t); }
  }
  const topTools = [...counts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 2)
    .map(([n, c]) => (c > 1 ? `${n} ×${c}` : n));
  return { count: stack.events.length, topTools, errorCount, durationMs: max >= min ? max - min : 0 };
}
