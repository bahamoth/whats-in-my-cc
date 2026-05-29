import type { ActivityEvent } from './streamModel';

export interface ActivityStackData { phase: string | null; events: ActivityEvent[]; }
export interface StackSummary { phase: string | null; count: number; topTools: string[]; errorCount: number; durationMs: number; }

export function splitRunByPhase(run: ActivityEvent[], phaseOf: (eventId: string) => string | null): ActivityStackData[] {
  const stacks: ActivityStackData[] = [];
  for (const ae of run) {
    const ph = phaseOf(ae.event.event_id);
    const last = stacks.at(-1);
    if (last && (last.phase === ph || stacks.length >= 2)) last.events.push(ae);
    else stacks.push({ phase: ph, events: [ae] });
  }
  return stacks;
}

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
  return { phase: stack.phase, count: stack.events.length, topTools, errorCount, durationMs: max >= min ? max - min : 0 };
}
