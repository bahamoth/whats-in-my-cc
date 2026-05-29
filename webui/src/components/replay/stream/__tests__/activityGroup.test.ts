import { describe, it, expect } from 'vitest';
import { splitRunByPhase, summarizeStack } from '../activityGroup';
import type { ActivityEvent } from '../streamModel';

const a = (id: string, kind: string, tool?: string, isErr?: boolean): ActivityEvent => ({
  event: { event_id: id, kind, observed_at: `2026-05-28T00:00:0${id}Z`, tool_name: tool ?? null,
    payload: tool ? { tool_name: tool, input: {} } : {} } as any,
  result: isErr === undefined ? null : { isError: isErr },
});

const phaseOf = (eid: string): string | null => ({ '1': 'exploration', '2': 'exploration', '3': 'action', '4': 'action' } as any)[eid] ?? null;

describe('splitRunByPhase', () => {
  it('splits a run at phase boundaries (max 2 stacks)', () => {
    const run = [a('1','tool_call','Read'), a('2','tool_call','Read'), a('3','tool_call','Bash'), a('4','hook_event')];
    const stacks = splitRunByPhase(run, phaseOf);
    expect(stacks.map((s) => s.phase)).toEqual(['exploration', 'action']);
    expect(stacks[0].events.map((e) => e.event.event_id)).toEqual(['1', '2']);
    expect(stacks[1].events.map((e) => e.event.event_id)).toEqual(['3', '4']);
  });
  it('single phase → one stack', () => {
    const run = [a('1','tool_call','Read'), a('2','tool_call','Read')];
    expect(splitRunByPhase(run, phaseOf)).toHaveLength(1);
  });
  it('caps at 2 stacks: a third phase merges into the last', () => {
    const run = [a('1','tool_call','Read'), a('3','tool_call','Bash'),
      { event: { event_id: '9', kind: 'tool_call', observed_at: 'z', tool_name: 'Edit', payload: {} } as any, result: null }];
    const stacks = splitRunByPhase(run, phaseOf);
    expect(stacks.length).toBeLessThanOrEqual(2);
    expect(stacks.at(-1)!.events.map((e) => e.event.event_id)).toContain('9');
  });
});

describe('summarizeStack', () => {
  it('aggregates top tools with ×N, error count, total + duration', () => {
    const s = summarizeStack({ phase: 'exploration', events: [
      a('1','tool_call','Read'), a('2','tool_call','Read'), a('3','tool_call','Bash', true)] });
    expect(s.count).toBe(3);
    expect(s.topTools).toEqual(['Read ×2', 'Bash']);
    expect(s.errorCount).toBe(1);
  });
});
