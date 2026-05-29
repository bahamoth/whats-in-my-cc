// webui/src/components/replay/timeline/__tests__/nodeLane.test.ts
/** R4 RED — node→lane mapping reuses laneMapping. Plan R4 Task 3. */
import { describe, expect, it } from 'vitest';
import { laneOfNodeKind, nodesByLane } from '../nodeLane';
import { LANES } from '../../../../api/laneMapping';

describe('nodeLane', () => {
  it('maps known kinds to their lane (per laneMapping)', () => {
    expect(laneOfNodeKind('user_message')).toBe('Intent');
    expect(laneOfNodeKind('tool_call')).toBe('Action');
    expect(laneOfNodeKind('otel_span')).toBe('OTel');
  });
  it('returns null for an unknown kind', () => {
    expect(laneOfNodeKind('made_up_kind')).toBeNull();
  });
  it('groups nodes into lane buckets keyed by lane name', () => {
    const nodes = [
      { node_id: 'a', node_kind: 'user_message' },
      { node_id: 'b', node_kind: 'tool_call' },
    ] as any;
    const byLane = nodesByLane(nodes);
    expect(byLane.get('Intent')?.map((n: any) => n.node_id)).toEqual(['a']);
    expect(byLane.get('Action')?.map((n: any) => n.node_id)).toEqual(['b']);
    expect(LANES).toContain('Intent');
  });
});
