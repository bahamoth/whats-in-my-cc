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
    expect(L('tool_call', { tool_name: 'Skill', input: { skill: 'corp-pptx-style' } }))
      .toEqual({ kind: 'tool', primary: 'Skill', secondary: 'corp-pptx-style' });
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
});
