import { describe, it, expect } from 'vitest';
import { buildRawBlocksFromEvents } from '../rawBlocks';
import type { ObservedEventDto } from '../../../../api/types';

function ev(p: Partial<ObservedEventDto> & { event_id: string; kind: string }): ObservedEventDto {
  return {
    raw_event_id: '', session_id: 's', event_uuid: null, parent_uuid: null,
    observed_at: '2026-05-28T00:00:00Z', actor: 'system', subkind: null, tool_use_id: null,
    tool_name: null, turn_id: null, is_sidechain: false, is_meta: false, payload: {}, ...p,
  } as ObservedEventDto;
}

// Source-split Raw blocks built DIRECTLY from events by correlation key — no
// graph node, no facet fold. A tool_call's output (tool_result) and any folded
// telemetry are found among the loaded events by tool_use_id.
describe('buildRawBlocksFromEvents', () => {
  it('splits a tool_call into entity + tool_result + correlated telemetry blocks', () => {
    const call = ev({ event_id: 'c1', kind: 'tool_call', tool_use_id: 'u1',
      payload: { tool_name: 'Bash', input: { command: 'ls' } } });
    const result = ev({ event_id: 'r1', kind: 'tool_result', tool_use_id: 'u1',
      payload: { tool_result: { is_error: false, content: 'ok' } } });
    const log = ev({ event_id: 'l1', kind: 'log_record',
      payload: { event_name: 'tool_result', attributes: { tool_use_id: 'u1', duration_ms: '57' } } });
    const blocks = buildRawBlocksFromEvents(call, [call, result, log]);
    expect(blocks).toBeDefined();
    const sources = blocks!.map((b) => b.source);
    expect(sources).toContain('transcript'); // the call entity
    expect(sources).toContain('tool_result'); // the matched result output
    // the tool_result block is labelled by ok/error
    const tr = blocks!.find((b) => b.source === 'tool_result');
    expect(tr!.label).toBe('ok');
  });

  it('returns undefined for a plain event with nothing to split (single-record fallback)', () => {
    const msg = ev({ event_id: 'm1', kind: 'user_message', actor: 'user', payload: { text: 'hi' } });
    expect(buildRawBlocksFromEvents(msg, [msg])).toBeUndefined();
  });
});
