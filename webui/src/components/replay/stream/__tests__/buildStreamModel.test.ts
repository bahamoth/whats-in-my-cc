import { describe, it, expect } from 'vitest';
import { buildStreamModel } from '../streamModel';
import type { ObservedEventDto } from '../../../../api/types';

function ev(p: Partial<ObservedEventDto> & { event_id: string; kind: string }): ObservedEventDto {
  return { raw_event_id: '', session_id: 's', event_uuid: null, parent_uuid: null,
    observed_at: '2026-05-28T00:00:00Z', actor: 'user', subkind: null, tool_use_id: null,
    tool_name: null, turn_id: null, is_sidechain: false, is_meta: false, payload: {}, ...p } as ObservedEventDto;
}

describe('buildStreamModel', () => {
  it('reads user text from BOTH content and text fields (#bug: 7971 empty cards)', () => {
    const items = buildStreamModel([
      ev({ event_id: 'a', kind: 'user_message', payload: { content: '질문1' } }),
      ev({ event_id: 'b', kind: 'user_message', payload: { content_ordinal: 0, text: '질문2' } }),
    ]);
    const msgs = items.filter((i) => i.type === 'message');
    expect(msgs.map((m: any) => m.text)).toEqual(['질문1', '질문2']);
  });

  it('excludes empty and scaffolding user messages from first-class cards', () => {
    const items = buildStreamModel([
      ev({ event_id: 'a', kind: 'user_message', payload: { text: '' } }),
      ev({ event_id: 'b', kind: 'user_message', payload: { content: '<command-name>/clear</command-name>' } }),
      ev({ event_id: 'c', kind: 'user_message', payload: { content: 'Base directory for this skill: /x' } }),
    ]);
    expect(items.filter((i) => i.type === 'message')).toHaveLength(0);
    expect(items.some((i) => i.type === 'activity-run')).toBe(true);
  });

  it('keeps readable thinking as a message; redacted thinking becomes a selectable thinking marker (not activity)', () => {
    const items = buildStreamModel([
      ev({ event_id: 't1', kind: 'thinking', actor: 'assistant', payload: { thinking: '먼저 확인하자' } }),
      ev({ event_id: 't2', kind: 'thinking', actor: 'assistant', payload: { thinking: '', signature: 'sig' } }),
    ]);
    const msgs = items.filter((i: any) => i.type === 'message' && i.role === 'thinking');
    expect(msgs).toHaveLength(1);
    expect((msgs[0] as any).text).toBe('먼저 확인하자');
    // Redacted (content-less) thinking is surfaced as its own thinking marker
    // — not buried in an activity run, not dropped.
    const markers = items.filter((i: any) => i.type === 'thinking');
    expect(markers).toHaveLength(1);
    expect((markers[0] as any).events[0].eventId).toBe('t2');
    expect(items.some((i: any) => i.type === 'activity-run')).toBe(false);
  });

  it('attaches per-response metrics to a thinking marker via request_id join', () => {
    const metricsByReq = new Map([
      ['req-1', { requestId: 'req-1', durationMs: 11900, ttftMs: 3100, inputTokens: 2,
        outputTokens: 1540, cacheReadTokens: 290000, cacheCreationTokens: 2200,
        stopReason: 'tool_use', attempt: 1, success: true, model: 'claude-opus-4-8' }],
    ]);
    const items = buildStreamModel(
      [ev({ event_id: 't1', kind: 'thinking', actor: 'assistant', request_id: 'req-1', payload: { thinking: '', signature: 'sig' } })],
      metricsByReq,
    );
    const marker: any = items.find((i: any) => i.type === 'thinking');
    expect(marker.events[0].requestId).toBe('req-1');
    expect(marker.events[0].metrics?.durationMs).toBe(11900);
    expect(marker.events[0].metrics?.outputTokens).toBe(1540);
  });

  it('groups a contiguous run of non-message events into one activity-run with its events', () => {
    const items = buildStreamModel([
      ev({ event_id: 'u', kind: 'user_message', payload: { content: 'go' } }),
      ev({ event_id: 'c1', kind: 'tool_call', actor: 'assistant', tool_name: 'Read', payload: { tool_name: 'Read', input: { file_path: '/a' } } }),
      ev({ event_id: 'h1', kind: 'hook_event', actor: 'hook', payload: { hookName: 'PreToolUse' } }),
      ev({ event_id: 'a', kind: 'assistant_message', actor: 'assistant', payload: { text: 'done', model: 'claude-opus-4-8' } }),
    ]);
    expect(items.map((i: any) => i.type)).toEqual(['message', 'activity-run', 'message']);
    expect((items[1] as any).events.map((e: any) => e.event.event_id)).toEqual(['c1', 'h1']);
  });

  it('merges tool_result into its tool_call (ok/error) inside the run events', () => {
    const items = buildStreamModel([
      ev({ event_id: 'c1', kind: 'tool_call', tool_use_id: 'x', tool_name: 'Read', payload: { tool_name: 'Read', input: {} } }),
      ev({ event_id: 'r1', kind: 'tool_result', actor: 'system', tool_use_id: 'x', payload: { tool_result: { is_error: true } } }),
    ]);
    const run: any = items.find((i: any) => i.type === 'activity-run');
    expect(run.events).toHaveLength(1);
    expect(run.events[0].result).toEqual({ isError: true });
  });
});

describe('buildStreamModel — classify refinement (#7)', () => {
  it('drops telemetry/facet events from the stream but keeps state-change logs', () => {
    const items = buildStreamModel([
      ev({ event_id: '1', kind: 'metric_sample', actor: 'system', payload: { instrument_name: 'claude_code.token.usage' } }),
      ev({ event_id: '2', kind: 'otel_span', actor: 'system', payload: { raw_span: { name: 'claude_code.tool' } } }),
      ev({ event_id: '3', kind: 'log_record', actor: 'system', payload: { event_name: 'tool_result', attributes: {} } }),
      ev({ event_id: '4', kind: 'log_record', actor: 'system', payload: { event_name: 'compaction', attributes: {} } }),
    ]);
    const acts = items.filter((i) => i.type === 'activity-run');
    const evIds = acts.flatMap((a: any) => a.events.map((e: any) => e.event.event_id));
    expect(evIds).toContain('4');     // state-change log kept
    expect(evIds).not.toContain('1'); // metric dropped
    expect(evIds).not.toContain('2'); // span dropped
    expect(evIds).not.toContain('3'); // facet log dropped
  });
});

describe('buildStreamModel — sidechain grouping (#3)', () => {
  it('tags message items with their sidechain origin', () => {
    const items = buildStreamModel([
      ev({ event_id: 'u', kind: 'user_message', payload: { content: 'hi' } }),
      ev({ event_id: 's', kind: 'user_message', is_sidechain: true, payload: { content: 'explore' } }),
    ]);
    const mainMsg = items.find((i: any) => i.type === 'message') as any;
    expect(mainMsg.sidechain).toBe(false);
    const group = items.find((i: any) => i.type === 'sidechain-group') as any;
    expect(group).toBeTruthy();
    const subMsg = group.items.find((i: any) => i.type === 'message') as any;
    expect(subMsg.sidechain).toBe(true);
  });

  it('groups a contiguous sidechain run into one sidechain-group, preserving main flow order', () => {
    const items = buildStreamModel([
      ev({ event_id: 'u1', kind: 'user_message', payload: { content: 'main q' } }),
      ev({ event_id: 'a1', kind: 'assistant_message', actor: 'assistant', payload: { text: 'dispatching', model: 'claude-opus-4-8' } }),
      ev({ event_id: 's-u', kind: 'user_message', is_sidechain: true, payload: { content: 'subagent prompt' } }),
      ev({ event_id: 's-a', kind: 'assistant_message', is_sidechain: true, actor: 'assistant', payload: { text: 'subagent reply', model: 'claude-opus-4-8' } }),
      ev({ event_id: 'u2', kind: 'user_message', payload: { content: 'main q2' } }),
    ]);
    expect(items.map((i: any) => i.type)).toEqual(['message', 'message', 'sidechain-group', 'message']);
    const group = items[2] as any;
    expect(group.items.map((i: any) => i.type)).toEqual(['message', 'message']);
    expect(group.items.map((i: any) => i.text)).toEqual(['subagent prompt', 'subagent reply']);
  });

  it('keeps sidechain tool activity inside the group, not the main flow', () => {
    const items = buildStreamModel([
      ev({ event_id: 's-u', kind: 'user_message', is_sidechain: true, payload: { content: 'p' } }),
      ev({ event_id: 's-c', kind: 'tool_call', is_sidechain: true, actor: 'assistant', tool_name: 'Read', payload: { tool_name: 'Read', input: {} } }),
    ]);
    expect(items).toHaveLength(1);
    const group = items[0] as any;
    expect(group.type).toBe('sidechain-group');
    expect(group.items.map((i: any) => i.type)).toEqual(['message', 'activity-run']);
  });

  it('separates two distinct subagent dispatches split by main-thread events', () => {
    const items = buildStreamModel([
      ev({ event_id: 's1', kind: 'user_message', is_sidechain: true, payload: { content: 'sub1' } }),
      ev({ event_id: 'm', kind: 'assistant_message', actor: 'assistant', payload: { text: 'back to main', model: 'claude-opus-4-8' } }),
      ev({ event_id: 's2', kind: 'user_message', is_sidechain: true, payload: { content: 'sub2' } }),
    ]);
    expect(items.map((i: any) => i.type)).toEqual(['sidechain-group', 'message', 'sidechain-group']);
  });
});
