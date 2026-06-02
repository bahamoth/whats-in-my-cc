import { describe, it, expect } from 'vitest';
import { buildStreamModel } from '../streamModel';
import { buildLlmRequestMetrics } from '../llmRequestMetrics';
import type { ObservedEventDto } from '../../../../api/types';

function ev(p: Partial<ObservedEventDto> & { event_id: string; kind: string }): ObservedEventDto {
  return { raw_event_id: '', session_id: 's', event_uuid: null, parent_uuid: null,
    observed_at: '2026-05-28T00:00:00Z', actor: 'user', subkind: null, tool_use_id: null,
    tool_name: null, turn_id: null, is_sidechain: false, is_meta: false, payload: {}, ...p } as ObservedEventDto;
}

/** A claude_code.llm_request OTel span event in the real OTLP attribute shape
 *  (`attributes: [{ key, value: { stringValue | intValue } }]`). */
function llmRequestSpan(
  eventId: string,
  attrs: Record<string, string | number | boolean>,
): ObservedEventDto {
  const attributes = Object.entries(attrs).map(([key, v]) => {
    const value =
      typeof v === 'number'
        ? Number.isInteger(v) ? { intValue: String(v) } : { doubleValue: v }
        : typeof v === 'boolean'
        ? { boolValue: v }
        : { stringValue: v };
    return { key, value };
  });
  return ev({
    event_id: eventId,
    kind: 'otel_span',
    actor: 'system',
    payload: { raw_span: { name: 'claude_code.llm_request', attributes } },
  });
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
        stopReason: 'tool_use', attempt: 1, success: true, model: 'claude-opus-4-8', costUsd: null }],
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

  it('computes a tool_call activity event duration from its matched tool_result timestamp', () => {
    const items = buildStreamModel([
      ev({ event_id: 'c1', kind: 'tool_call', tool_use_id: 'u1', observed_at: '2026-05-28T00:00:00.000Z',
        payload: { tool_name: 'Bash', input: { command: 'ls' } } }),
      ev({ event_id: 'r1', kind: 'tool_result', tool_use_id: 'u1', observed_at: '2026-05-28T00:00:00.500Z',
        payload: { tool_result: { is_error: false } } }),
    ]);
    const run = items.find((i: any) => i.type === 'activity-run') as any;
    expect(run.events[0].durationMs).toBe(500);
  });

  it('leaves durationMs null for a tool_call with no matched result', () => {
    const items = buildStreamModel([
      ev({ event_id: 'c1', kind: 'tool_call', tool_use_id: 'u1', observed_at: '2026-05-28T00:00:00.000Z',
        payload: { tool_name: 'Bash', input: { command: 'ls' } } }),
    ]);
    const run = items.find((i: any) => i.type === 'activity-run') as any;
    expect(run.events[0].durationMs).toBeNull();
  });

  it('drops hook_additional_context (a byte-identical duplicate of its hook_success sibling) but keeps hook_success', () => {
    // Verified against real data: a SessionStart hook emits both hook_success
    // (exitCode/durationMs/command + stdout) and hook_additional_context, whose
    // `content` is identical to hook_success.stdout.hookSpecificOutput
    // .additionalContext (5632 == 5632 chars). The additional_context event adds
    // no information → drop it from the message view.
    const items = buildStreamModel([
      ev({ event_id: 'hs', kind: 'hook_event', actor: 'hook', subkind: 'hook_success',
        payload: { type: 'hook_success', hookName: 'SessionStart:startup', exitCode: 0, durationMs: 314 } }),
      ev({ event_id: 'hac', kind: 'hook_event', actor: 'hook', subkind: 'hook_additional_context',
        payload: { type: 'hook_additional_context', hookEvent: 'SessionStart', content: ['…'] } }),
    ]);
    const acts = items.filter((i) => i.type === 'activity-run');
    const evIds = acts.flatMap((a: any) => a.events.map((e: any) => e.event.event_id));
    expect(evIds).toContain('hs');      // execution record kept
    expect(evIds).not.toContain('hac'); // duplicate injected-context dropped
  });
});

describe('buildStreamModel — signal-less meta dropped, system_summary surfaced', () => {
  it('drops attachment_meta and session_state — they carry no display signal', () => {
    const items = buildStreamModel([
      ev({ event_id: 'att', kind: 'attachment_meta', actor: 'system', payload: { file: '/x', deferred_tools_delta: 2 } }),
      ev({ event_id: 'ses', kind: 'session_state', actor: 'system', subkind: 'permission_mode', payload: { leafUuid: 'u', permissionMode: 'default' } }),
    ]);
    expect(items).toHaveLength(0);
  });

  // The system_summary subkinds + payload shapes below are anchored to real CC
  // transcripts (session 01fe9550 for away_summary/turn_duration/
  // stop_hook_summary/local_command; compact_boundary observed in other
  // sessions). Real shapes: away_summary carries a `content` recap; the
  // compact_boundary record is `{content:"Conversation compacted",
  // compactMetadata:{trigger,preTokens,…}}`; turn_duration carries `durationMs`
  // (no content); stop_hook_summary carries hook fields (no content). Freezing
  // these here means a CC-side subtype rename would break this test.
  it('surfaces a system_summary away_summary as a visible item carrying its content text', () => {
    const recap = '목표는 Episode 분류 개선. 현재 telemetry facet 통합 수정 완료. 다음 액션은 PR 게이트 후 push.';
    const items = buildStreamModel([
      ev({ event_id: 'ss', kind: 'system_summary', actor: 'system', subkind: 'away_summary', payload: { content: recap, isMeta: true } }),
    ]);
    // Not dropped — the CC work recap must be visible in the message view.
    expect(items).toHaveLength(1);
    // The recap text is carried on the produced StreamItem so it can render.
    const item: any = items[0];
    expect(item.type).toBe('message');
    expect(item.role).toBe('system');
    expect(item.text).toBe(recap);
  });

  it('surfaces a compact_boundary system_summary as a visible marker', () => {
    const items = buildStreamModel([
      ev({ event_id: 'cb', kind: 'system_summary', actor: 'system', subkind: 'compact_boundary',
        payload: { content: 'Conversation compacted', compactMetadata: { trigger: 'manual', preTokens: 474104 } } }),
    ]);
    expect(items).toHaveLength(1);
    const item: any = items[0];
    expect(item.type).toBe('message');
    expect(item.role).toBe('system');
    expect(item.text).toBe('Conversation compacted');
  });

  it('drops a shown subkind whose content is empty (no placeholder card — mirrors empty user/assistant drop)', () => {
    const items = buildStreamModel([
      ev({ event_id: 'e1', kind: 'system_summary', actor: 'system', subkind: 'away_summary', payload: { content: '   ' } }),
      ev({ event_id: 'e2', kind: 'system_summary', actor: 'system', subkind: 'compact_boundary', payload: {} }),
    ]);
    expect(items).toHaveLength(0);
  });

  it('drops thin/telemetry-ish system_summary subkinds (only away_summary/compact_boundary are card-worthy)', () => {
    const items = buildStreamModel([
      ev({ event_id: 'td', kind: 'system_summary', actor: 'system', subkind: 'turn_duration', payload: { durationMs: 1200, messageCount: 3 } }),
      ev({ event_id: 'sh', kind: 'system_summary', actor: 'system', subkind: 'stop_hook_summary', payload: { hookCount: 1, hasOutput: false } }),
      ev({ event_id: 'lc', kind: 'system_summary', actor: 'system', subkind: 'local_command', payload: { content: '/clear' } }),
      ev({ event_id: 'no', kind: 'system_summary', actor: 'system', subkind: null, payload: { content: 'x' } }),
    ]);
    // Conservative default: every system_summary except away_summary/compact_boundary drops.
    expect(items).toHaveLength(0);
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

// ---------------------------------------------------------------------------
// REGRESSION GUARD: thinking metrics survive the telemetry display-drop.
//
// Invariant locked: telemetry dropped from the VISIBLE stream must remain a
// METRIC SOURCE; thinking metrics come ONLY from the request_id ↔
// claude_code.llm_request span join (the thinking event itself stores no
// plaintext metrics — empty `thinking` text + an opaque signature).
//
// The danger this protects against: a future change that filters
// claude_code.llm_request spans out of the event window/source entirely
// (not just from display) would silently kill every thinking beat's metrics.
// We build ONE window holding both a redacted thinking event and its matching
// span, then assert the full chain end-to-end on real-shaped data.
// ---------------------------------------------------------------------------
describe('buildStreamModel — thinking metrics survive telemetry display-drop (#regression)', () => {
  const window: ObservedEventDto[] = [
    ev({
      event_id: 'th-1',
      kind: 'thinking',
      actor: 'assistant',
      request_id: 'req_X',
      // Redacted thinking: empty plaintext, only an opaque signature.
      payload: { thinking: '', signature: 'opaque-sig-bytes' },
    }),
    llmRequestSpan('sp-1', {
      request_id: 'req_X',
      output_tokens: 1540,
      duration_ms: 11869,
    }),
  ];

  it('(a) buildLlmRequestMetrics extracts req_X from the llm_request span attributes', () => {
    // Locks request_id extraction + the `claude_code.llm_request` name filter.
    const metrics = buildLlmRequestMetrics(window);
    const got = metrics.get('req_X');
    expect(got).toBeDefined();
    expect(got!.outputTokens).toBe(1540);
    expect(got!.durationMs).toBe(11869);
  });

  it('(b) the otel_span produces NO visible stream item (telemetry not shown as a card)', () => {
    const items = buildStreamModel(window, buildLlmRequestMetrics(window));
    // No item (message/activity-run/sidechain-group/thinking) is sourced from
    // the span event — telemetry is dropped from the displayed stream.
    const fromSpan = items.filter((i: any) => {
      if (i.type === 'message') return i.eventId === 'sp-1';
      if (i.type === 'thinking') return i.events.some((e: any) => e.eventId === 'sp-1');
      if (i.type === 'activity-run')
        return i.events.some((e: any) => e.event.event_id === 'sp-1');
      if (i.type === 'sidechain-group') return false;
      return false;
    });
    expect(fromSpan).toHaveLength(0);
    // The only visible item is the thinking marker.
    expect(items).toHaveLength(1);
    expect((items[0] as any).type).toBe('thinking');
  });

  it('(c) the thinking beat CARRIES req_X metrics from the same window (dropped span still feeds it)', () => {
    // Core guard: the span is dropped from DISPLAY (b) yet still feeds the
    // thinking beat's metrics via the request_id join. Build metrics from the
    // same window so the only metric source is the in-window span.
    const items = buildStreamModel(window, buildLlmRequestMetrics(window));
    const marker: any = items.find((i: any) => i.type === 'thinking');
    expect(marker).toBeDefined();
    const beat = marker.events[0];
    expect(beat.requestId).toBe('req_X');
    expect(beat.metrics).not.toBeNull();
    expect(beat.metrics.outputTokens).toBe(1540);
    expect(beat.metrics.durationMs).toBe(11869);
  });
});
