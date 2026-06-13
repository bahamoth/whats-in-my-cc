import { describe, it, expect } from 'vitest';
import { buildStreamModel } from '../streamModel';
import { buildLlmRequestMetrics } from '../llmRequestMetrics';
import type { ObservedEventDto } from '../../../../api/types';

function ev(p: Partial<ObservedEventDto> & { event_id: string; kind: string }): ObservedEventDto {
  return { raw_event_id: '', session_id: 's', event_uuid: null, parent_uuid: null,
    observed_at: '2026-05-28T00:00:00Z', actor: 'user', subkind: null, tool_use_id: null,
    tool_name: null, turn_id: null, is_sidechain: false, is_meta: false, payload: {}, ...p } as ObservedEventDto;
}

/** A claude_code.llm_request OTel span event. C4 (Tier 3-1): span name +
 *  metrics live in the telemetry facet (a SIBLING of payload), whose
 *  `attributes` is the FLAT key→value object the backend's `flatten_kv`
 *  produces — NOT the OTLP array, and NOT re-embedded under payload.raw_span. */
function llmRequestSpan(
  eventId: string,
  attrs: Record<string, string | number | boolean>,
): ObservedEventDto {
  return ev({
    event_id: eventId,
    kind: 'otel_span',
    actor: 'system',
    telemetry: { span_name: 'claude_code.llm_request', attributes: attrs },
    payload: {},
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

  it('drops empty user messages but keeps command/skill scaffolding as user-side messages tagged by origin (not activity, not "You")', () => {
    const items = buildStreamModel([
      ev({ event_id: 'a', kind: 'user_message', payload: { text: '' } }),
      ev({ event_id: 'b', kind: 'user_message', payload: { content: '<command-name>/clear</command-name>' } }),
      ev({ event_id: 'c', kind: 'user_message', payload: { content: 'Base directory for this skill: /x' } }),
    ]);
    const msgs = items.filter((i) => i.type === 'message') as any[];
    // empty dropped; command + skill remain as USER-SIDE messages (user-invoked),
    // never relocated to the agent/activity side.
    expect(msgs.map((m) => m.origin)).toEqual(['command', 'skill']);
    expect(msgs.map((m) => m.role)).toEqual(['user', 'user']);
    expect(items.some((i) => i.type === 'activity-run')).toBe(false);
  });

  it('isMeta:true injected text is a user-side message tagged origin=skill, never human "You" (caller gap)', () => {
    // Real leak: a skill/command body injected as type:"user" + isMeta:true has
    // no command marker, so it used to fall through to a "You" human bubble.
    const items = buildStreamModel([
      ev({ event_id: 'h', kind: 'user_message', payload: { content: '진짜 사람 질문' } }),
      ev({ event_id: 'm', kind: 'user_message', is_meta: true, payload: { content: 'Review the PR thoroughly...' } }),
    ]);
    const msgs = items.filter((i) => i.type === 'message') as any[];
    expect(msgs.map((m) => m.origin)).toEqual(['human', 'skill']);
    expect(msgs[0].text).toBe('진짜 사람 질문');
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
      ev({ event_id: '2', kind: 'otel_span', actor: 'system', telemetry: { span_name: 'claude_code.tool' }, payload: {} }),
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

  it('keeps standalone session-beat log_records (subagent_completed) but drops folded/duplicate ones', () => {
    const items = buildStreamModel([
      ev({ event_id: 'sub', kind: 'log_record', actor: 'system', payload: { event_name: 'subagent_completed', attributes: { agent_type: 'Explore' } } }),
      ev({ event_id: 'api', kind: 'log_record', actor: 'system', payload: { event_name: 'api_request', attributes: {} } }),
      ev({ event_id: 'hx', kind: 'log_record', actor: 'system', payload: { event_name: 'hook_execution_complete', attributes: {} } }),
    ]);
    const evIds = items
      .filter((i) => i.type === 'activity-run')
      .flatMap((a: any) => a.events.map((e: any) => e.event.event_id));
    expect(evIds).toContain('sub');     // standalone unique beat → kept
    expect(evIds).not.toContain('api'); // folded into assistant detail → dropped
    expect(evIds).not.toContain('hx');  // redundant w/ transcript hook_event + high volume → dropped
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

describe('buildStreamModel — sidechain agent attribution (agent_id)', () => {
  it('carries the events agent_id on the sidechain-group', () => {
    const items = buildStreamModel([
      ev({ event_id: 's-u', kind: 'user_message', is_sidechain: true, agent_id: 'agentX', payload: { content: 'p' } }),
      ev({ event_id: 's-a', kind: 'assistant_message', is_sidechain: true, agent_id: 'agentX', actor: 'assistant', payload: { text: 'r', model: 'claude-opus-4-8' } }),
    ]);
    const group = items.find((i: any) => i.type === 'sidechain-group') as any;
    expect(group.agentId).toBe('agentX');
  });

  it('de-interleaves parallel subagents: each agent_id collapses to ONE group (no fragments)', () => {
    // 병렬 Task 디스패치: 두 서브에이전트의 이벤트가 시간순으로 교차 도착한다.
    // 예전엔 agent_id 변화가 경계라 한 에이전트가 여러 조각으로 쪼개졌지만(설계 2026-06-13),
    // 이제 agent_id로 전역 수집(de-interleave)하므로 agentX는 교차에도 한 그룹으로 모인다.
    // 디스패치 사이드카가 없어(N 판별 불가) 배치 래핑 없이 first-seen 순서대로 평탄 emit.
    const items = buildStreamModel([
      ev({ event_id: 'x1', kind: 'user_message', is_sidechain: true, agent_id: 'agentX', payload: { content: 'px' } }),
      ev({ event_id: 'y1', kind: 'user_message', is_sidechain: true, agent_id: 'agentY', payload: { content: 'py' } }),
      ev({ event_id: 'x2', kind: 'assistant_message', is_sidechain: true, agent_id: 'agentX', actor: 'assistant', payload: { text: 'rx', model: 'claude-opus-4-8' } }),
    ]);
    expect(items.map((i: any) => [i.type, i.agentId])).toEqual([
      ['sidechain-group', 'agentX'],
      ['sidechain-group', 'agentY'],
    ]);
    const x = items.find((i: any) => i.agentId === 'agentX') as any;
    // agentX의 교차 도착한 두 이벤트가 한 그룹에 직렬로 모인다 (조각 X).
    expect(x.items.map((i: any) => i.id)).toEqual(['x1', 'x2']);
  });

  it('splits a sidechain ACTIVITY run at an agent_id boundary (tools from two agents never share a run)', () => {
    const items = buildStreamModel([
      ev({ event_id: 'cx', kind: 'tool_call', is_sidechain: true, agent_id: 'agentX', actor: 'assistant', tool_name: 'Read', payload: { tool_name: 'Read', input: {} } }),
      ev({ event_id: 'cy', kind: 'tool_call', is_sidechain: true, agent_id: 'agentY', actor: 'assistant', tool_name: 'Grep', payload: { tool_name: 'Grep', input: {} } }),
    ]);
    expect(items.map((i: any) => [i.type, i.agentId])).toEqual([
      ['sidechain-group', 'agentX'],
      ['sidechain-group', 'agentY'],
    ]);
    for (const g of items as any[]) expect(g.items[0].type).toBe('activity-run');
  });

  it('joins subagent_meta sidecar event onto the group: agentType, description, taskEventId', () => {
    // 실 사이드카(subagent_sidecar_v01 fixture)가 ingest되면 attachment_meta/
    // subagent_meta 이벤트가 같은 윈도우에 실린다. 그룹은 agent_id로 메타를,
    // 메타의 toolUseId로 메인 체인 Task tool_call 이벤트를 찾는다(점프 타깃).
    const items = buildStreamModel([
      ev({ event_id: 'task1', kind: 'tool_call', actor: 'assistant', tool_use_id: 'toolu_T', tool_name: 'Task',
        payload: { tool_name: 'Task', input: { description: '조사', prompt: 'p', subagent_type: 'Explore' } } }),
      ev({ event_id: 'meta1', kind: 'attachment_meta', subkind: 'subagent_meta', actor: 'system',
        agent_id: 'agentX', tool_use_id: 'toolu_T', is_sidechain: true,
        payload: { agentType: 'Explore', description: '간단 조사', toolUseId: 'toolu_T' } }),
      ev({ event_id: 's-u', kind: 'user_message', is_sidechain: true, agent_id: 'agentX', payload: { content: 'p' } }),
    ]);
    const group = items.find((i: any) => i.type === 'sidechain-group') as any;
    expect(group.agentId).toBe('agentX');
    expect(group.agentType).toBe('Explore');
    expect(group.description).toBe('간단 조사');
    expect(group.taskEventId).toBe('task1');
    // 사이드카 이벤트 자체는 카드로 렌더되지 않는다 (attachment_meta drop 유지)
    const allIds = items.flatMap((i: any) =>
      i.type === 'sidechain-group' ? i.items.map((x: any) => x.id) : [i.id]);
    expect(allIds).not.toContain('meta1');
  });

  it('leaves taskEventId null when the Task tool_call is outside the loaded window', () => {
    const items = buildStreamModel([
      ev({ event_id: 'meta1', kind: 'attachment_meta', subkind: 'subagent_meta', actor: 'system',
        agent_id: 'agentX', tool_use_id: 'toolu_T', is_sidechain: true,
        payload: { agentType: 'Explore', description: 'd', toolUseId: 'toolu_T' } }),
      ev({ event_id: 's-u', kind: 'user_message', is_sidechain: true, agent_id: 'agentX', payload: { content: 'p' } }),
    ]);
    const group = items.find((i: any) => i.type === 'sidechain-group') as any;
    expect(group.agentType).toBe('Explore');
    expect(group.taskEventId).toBeNull();
  });

  it('falls back to assistant payload.attribution_agent for agentType when no sidecar event', () => {
    const items = buildStreamModel([
      ev({ event_id: 's-u', kind: 'user_message', is_sidechain: true, agent_id: 'agentX', payload: { content: 'p' } }),
      ev({ event_id: 's-a', kind: 'assistant_message', is_sidechain: true, agent_id: 'agentX', actor: 'assistant',
        payload: { text: 'r', model: 'claude-opus-4-8', attribution_agent: 'Explore' } }),
    ]);
    const group = items.find((i: any) => i.type === 'sidechain-group') as any;
    expect(group.agentType).toBe('Explore');
    expect(group.taskEventId).toBeNull();
    expect(group.description).toBeNull();
  });

  it('treats missing agent_id (pre-0023 ingest: null or "") as contiguity-only grouping', () => {
    const items = buildStreamModel([
      ev({ event_id: 's1', kind: 'user_message', is_sidechain: true, agent_id: null, payload: { content: 'p1' } }),
      ev({ event_id: 's2', kind: 'assistant_message', is_sidechain: true, agent_id: '', actor: 'assistant', payload: { text: 'r1', model: 'claude-opus-4-8' } }),
    ]);
    expect(items).toHaveLength(1);
    const group = items[0] as any;
    expect(group.type).toBe('sidechain-group');
    expect(group.agentId).toBeNull();
    expect(group.items.map((i: any) => i.type)).toEqual(['message', 'message']);
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

// ---------------------------------------------------------------------------
// Parallel batch grouping (design 2026-06-13, sample-1 session
// fb6b8e3a-2289-4214-884c-0c721a3e3cf5): main dispatches N Agent calls in ONE
// assistant message; the N subagents run in parallel and their sidechain
// events arrive interleaved by timestamp. We collect per agent_id globally
// (de-interleave) and wrap the same-dispatch siblings in a BatchGroup.
// ---------------------------------------------------------------------------

const base = (over: Partial<ObservedEventDto>): ObservedEventDto =>
  ({
    event_id: 'e',
    session_id: 's',
    kind: 'assistant_message',
    actor: 'assistant',
    observed_at: '2026-06-13T00:00:00.000Z',
    is_sidechain: false,
    agent_id: '',
    message_id: null,
    turn_id: null,
    tool_use_id: null,
    subkind: null,
    payload: {},
    ...over,
  }) as ObservedEventDto;
const asstMain = (mid: string, text: string) =>
  base({ event_id: mid, message_id: mid, kind: 'assistant_message', payload: { text } });
const taskCall = (mid: string, ev: string, tu: string) =>
  base({ event_id: ev, message_id: mid, kind: 'tool_call', tool_name: 'Agent', tool_use_id: tu });
const sidecar = (ag: string, tu: string, atype: string, desc: string) =>
  base({
    event_id: `meta-${ag}`,
    kind: 'attachment_meta',
    subkind: 'subagent_meta',
    is_sidechain: true,
    agent_id: ag,
    tool_use_id: tu,
    payload: { agentType: atype, description: desc, toolUseId: tu },
  });
const scUser = (ag: string, ev: string) =>
  base({ event_id: ev, kind: 'user_message', is_sidechain: true, agent_id: ag, payload: { content: 'prompt' } });
const scAsst = (ag: string, ev: string, text: string) =>
  base({ event_id: ev, kind: 'assistant_message', is_sidechain: true, agent_id: ag, payload: { text } });

/** All sidechain-groups regardless of whether they sit inside a batch-group. */
function collectSidechainGroups(items: any[]): any[] {
  const out: any[] = [];
  for (const it of items) {
    if (it.type === 'sidechain-group') out.push(it);
    if (it.type === 'batch-group') out.push(...it.agentGroups);
  }
  return out;
}

describe('buildStreamModel — parallel batch grouping (#13 design 2026-06-13)', () => {
  it('병렬 형제는 BatchGroup으로 래핑되고 자식은 agent별 SidechainGroup', () => {
    const evs = [
      asstMain('m1', '병렬로 2개'), // main assistant
      taskCall('m1', 'tc-A', 'tu-A'),
      taskCall('m1', 'tc-B', 'tu-B'),
      sidecar('A', 'tu-A', 'Explore', '조사 A'),
      sidecar('B', 'tu-B', 'general', '조사 B'),
      // 교차 도착
      scUser('A', 'pA'),
      scUser('B', 'pB'),
      scAsst('A', 'a1', 'A 중간'),
      scAsst('B', 'b1', 'B 중간'),
      scAsst('A', 'a2', 'A 결론'),
      scAsst('B', 'b2', 'B 결론'),
    ];
    const items = buildStreamModel(evs);
    const batch = items.find((i: any) => i.type === 'batch-group') as any;
    expect(batch).toBeTruthy();
    expect(batch.agentGroups).toHaveLength(2);
    expect(batch.agentGroups.map((g: any) => g.agentId).sort()).toEqual(['A', 'B']);
  });

  it('각 자식 SidechainGroup.conclusion = 그 agent의 마지막 assistant_message', () => {
    const evs = [
      asstMain('m1', '병렬'),
      taskCall('m1', 'tcA', 'tuA'),
      sidecar('A', 'tuA', 'Explore', '조사 A'),
      scUser('A', 'pA'),
      scAsst('A', 'a1', '중간'),
      scAsst('A', 'a2', '최종 결론입니다'),
    ];
    const items = buildStreamModel(evs);
    // N=1 → 배치 래핑 없음, 단일 SidechainGroup
    const g = items.find((i: any) => i.type === 'sidechain-group') as any;
    expect(g.conclusion).toBe('최종 결론입니다');
  });

  it('교차 도착해도 한 agent는 한 SidechainGroup으로(조각 안 남)', () => {
    const evs = [
      asstMain('m1', '병렬'),
      taskCall('m1', 'tcA', 'tuA'),
      taskCall('m1', 'tcB', 'tuB'),
      sidecar('A', 'tuA', 'Explore', 'A'),
      sidecar('B', 'tuB', 'general', 'B'),
      scAsst('A', 'a1', 'A1'),
      scAsst('B', 'b1', 'B1'),
      scAsst('A', 'a2', 'A2'),
      scAsst('B', 'b2', 'B2'),
    ];
    const items = buildStreamModel(evs);
    const groups = collectSidechainGroups(items); // 헬퍼: batch 안/밖 모든 sidechain-group
    const byAgent = groups.filter((g: any) => g.agentId === 'A');
    expect(byAgent).toHaveLength(1); // 조각 X
    expect(byAgent[0].items.filter((i: any) => i.type === 'message')).toHaveLength(2);
  });
});
