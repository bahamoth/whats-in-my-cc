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

  // REGRESSION GUARD: the stated contract is "a failed tool's error output must
  // be visible in Raw". A tool_result whose tool_result.is_error === true must
  // be split out AND labelled 'error' (the 'ok' case above cannot catch a
  // regression that hard-codes 'ok' or inverts the is_error check).
  it("labels a failed tool_result block 'error' and carries its error record", () => {
    const call = ev({ event_id: 'c1', kind: 'tool_call', tool_use_id: 'u1',
      payload: { tool_name: 'Bash', input: { command: 'false' } } });
    const result = ev({ event_id: 'r1', kind: 'tool_result', tool_use_id: 'u1',
      payload: { tool_result: { is_error: true, content: 'boom' } } });
    const blocks = buildRawBlocksFromEvents(call, [call, result]);
    const tr = blocks!.find((b) => b.source === 'tool_result');
    expect(tr).toBeDefined();
    expect(tr!.label).toBe('error');
    expect((tr!.record as Record<string, unknown>).content).toBe('boom');
  });

  it('returns undefined for a plain event with nothing to split (single-record fallback)', () => {
    const msg = ev({ event_id: 'm1', kind: 'user_message', actor: 'user', payload: { text: 'hi' } });
    expect(buildRawBlocksFromEvents(msg, [msg])).toBeUndefined();
  });

  // C4 (Tier 3-1): the llm_request span no longer re-embeds payload.raw_span;
  // span name + attributes live in the telemetry facet. The correlated
  // llm_request_span Raw block must be matched by the facet (not payload) and
  // carry the facet as its record so the span data is still visible.
  it('splits an assistant turn into entity + correlated llm_request span block (from telemetry facet)', () => {
    const asst = ev({ event_id: 'a1', kind: 'assistant_message', actor: 'assistant',
      request_id: 'req_1', payload: { model: 'claude-opus-4-8', text: 'hi' } });
    const span = ev({ event_id: 's1', kind: 'otel_span', actor: 'system', request_id: 'req_1',
      telemetry: { span_name: 'claude_code.llm_request',
        attributes: { request_id: 'req_1', output_tokens: 900 } },
      payload: {} });
    const blocks = buildRawBlocksFromEvents(asst, [asst, span]);
    expect(blocks).toBeDefined();
    const llm = blocks!.find((b) => b.source === 'llm_request_span');
    expect(llm).toBeDefined();
    expect(llm!.label).toBe('claude_code.llm_request');
    // the block carries the telemetry facet (the span data), not an empty payload
    const rec = llm!.record as Record<string, unknown>;
    expect((rec.attributes as Record<string, unknown>).output_tokens).toBe(900);
  });
});
