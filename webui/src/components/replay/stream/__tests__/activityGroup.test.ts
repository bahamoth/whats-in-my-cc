import { describe, it, expect } from 'vitest';
import { summarizeStack } from '../activityGroup';
import type { ActivityEvent } from '../streamModel';

const a = (id: string, kind: string, tool?: string, isErr?: boolean): ActivityEvent => ({
  event: { event_id: id, kind, observed_at: `2026-05-28T00:00:0${id}Z`, tool_name: tool ?? null,
    payload: tool ? { tool_name: tool, input: {} } : {} } as any,
  result: isErr === undefined ? null : { isError: isErr },
});

describe('summarizeStack', () => {
  it('aggregates top tools with ×N, error count, total + duration', () => {
    const s = summarizeStack({ events: [
      a('1','tool_call','Read'), a('2','tool_call','Read'), a('3','tool_call','Bash', true)] });
    expect(s.count).toBe(3);
    expect(s.topTools).toEqual(['Read ×2', 'Bash']);
    expect(s.errorCount).toBe(1);
  });
});
