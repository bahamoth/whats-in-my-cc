import { describe, it, expect } from 'vitest';
import { nodeLabel, formatModel } from '../nodeLabel';

const L = (node_kind: string, payload: unknown) => nodeLabel({ node_kind, payload });

describe('formatModel', () => {
  it('shortens known model ids', () => {
    expect(formatModel('claude-opus-4-8')).toBe('Opus 4.8');
    expect(formatModel('claude-sonnet-4-6')).toBe('Sonnet 4.6');
    expect(formatModel('claude-haiku-4-5-20251001')).toBe('Haiku 4.5');
  });
  it('falls back for synthetic/unknown', () => {
    expect(formatModel('<synthetic>')).toBe('Claude');
    expect(formatModel(null)).toBe('Claude');
  });
});

describe('nodeLabel', () => {
  it('tool_call: tool name + key arg', () => {
    expect(L('tool_call', { tool_name: 'Read', input: { file_path: '/a/slide_logo-17.jpg' } }))
      .toEqual({ kind: 'tool', primary: 'Read', secondary: 'slide_logo-17.jpg' });
    expect(L('tool_call', { tool_name: 'Bash', input: { command: 'rm -f x.jpg && ls' } }))
      .toEqual({ kind: 'tool', primary: 'Bash', secondary: 'rm -f x.jpg && ls' });
    // a leading `cd …` is stripped so the shown command leads with the work
    expect(L('tool_call', { tool_name: 'Bash', input: { command: 'cd /repo && git add -A && git status' } }).secondary)
      .toBe('git add -A && git status');
    expect(L('tool_call', { tool_name: 'Skill', input: { skill: 'corp-pptx-style' } }))
      .toEqual({ kind: 'tool', primary: 'Skill', secondary: 'corp-pptx-style' });
  });
  it('tool_call: action-style (browser/computer) tools show what they did', () => {
    // mcp computer: action + coordinate → "what work was done"
    expect(L('tool_call', { tool_name: 'mcp__claude-in-chrome__computer', input: { action: 'left_click', coordinate: [638, 220], tabId: 1 } }).secondary)
      .toBe('left_click (638, 220)');
    // action + text
    expect(L('tool_call', { tool_name: 'mcp__claude-in-chrome__computer', input: { action: 'type', text: 'hello' } }).secondary)
      .toBe('type "hello"');
    // navigate: url
    expect(L('tool_call', { tool_name: 'mcp__claude-in-chrome__navigate', input: { url: 'http://localhost:5173', tabId: 1 } }).secondary)
      .toBe('http://localhost:5173');
  });
  it('tool_call: prefers a human description over the raw command/args', () => {
    // Bash carries both — show the intent, not the gnarly command.
    expect(L('tool_call', { tool_name: 'Bash', input: { command: 'perl -0pi -e "s/a/b/" f1 f2 f3 f4 f5', description: 'Add costUsd:null to the 5 fixtures via perl' } }).secondary)
      .toBe('Add costUsd:null to the 5 fixtures via perl');
    // no description → falls back to the (cd-stripped) command
    expect(L('tool_call', { tool_name: 'Bash', input: { command: 'cd /x && grep y' } }).secondary).toBe('grep y');
  });
  it('tool_call: Task shows its description; unknown shapes fall back to first field', () => {
    expect(L('tool_call', { tool_name: 'Task', input: { description: 'find flaky tests', subagent_type: 'general-purpose' } }).secondary)
      .toBe('find flaky tests');
    // no known key → labelled first string value, so something always shows
    expect(L('tool_call', { tool_name: 'mcp__x__y', input: { foo: 'bar', n: 3 } }).secondary)
      .toBe('foo: bar');
    expect(L('tool_call', { tool_name: 'Weird', input: {} }).secondary).toBe('');
  });
  it('assistant_message: model + message head', () => {
    expect(L('assistant_message', { model: 'claude-opus-4-8', text: 'NC 브랜드 스타일로 다시 만들겠습니다.' }))
      .toEqual({ kind: 'assistant', primary: 'Opus 4.8', secondary: 'NC 브랜드 스타일로 다시 만들겠습니다.' });
  });
  it('user_message: real text', () => {
    expect(L('user_message', { content: '왜 폰트를 썼지?' }))
      .toEqual({ kind: 'user', primary: 'You', secondary: '왜 폰트를 썼지?' });
  });
  it('user_message: scaffolding becomes command label', () => {
    expect(L('user_message', { content: '<command-name>/plugin</command-name>' }))
      .toEqual({ kind: 'user', primary: 'command', secondary: '/plugin' });
  });
  it('hook_event: hookName from either shape', () => {
    expect(L('hook_event', { hookName: 'PreToolUse:Agent' }).secondary).toBe('PreToolUse:Agent');
    expect(L('hook_event', { hook: { hook_event_name: 'PreToolUse' } }).secondary).toBe('PreToolUse');
  });
  it('otel_span: span name', () => {
    expect(L('otel_span', { raw_span: { name: 'claude_code.interaction' } }).secondary).toBe('claude_code.interaction');
  });
  it('log_record: friendly label by event_name + salient detail (state-change beats)', () => {
    expect(L('log_record', { event_name: 'subagent_completed', attributes: { agent_type: 'Explore' } }))
      .toEqual({ kind: 'other', primary: 'subagent', secondary: 'Explore' });
    expect(L('log_record', { event_name: 'mcp_server_connection', attributes: { status: 'connected' } }))
      .toMatchObject({ primary: 'mcp', secondary: 'connected' });
    expect(L('log_record', { event_name: 'compaction' }).primary).toBe('compaction');
    // unknown event_name falls back to the raw name (not a bare "log_record")
    expect(L('log_record', { event_name: 'something_new' }).primary).toBe('something_new');
  });
});
