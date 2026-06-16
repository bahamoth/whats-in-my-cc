import { describe, it, expect } from 'vitest';
import { nodeLabel, formatModel } from '../nodeLabel';
import { translate } from '../../../../i18n/t';
import { en } from '../../../../i18n/catalog/en';
import { ko } from '../../../../i18n/catalog/ko';
import type { TFunction } from '../../../../i18n';

// Assert against the Korean (source) labels by binding t to the ko catalog.
const koT: TFunction = (key, arg) => translate(ko, en, key, arg);

const L = (
  node_kind: string,
  payload: unknown,
  telemetry?: unknown,
  tag?: { display?: string | null } | null,
  is_meta?: boolean,
) => nodeLabel({ node_kind, payload, telemetry, tag, is_meta }, koT);

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
    // 선행 `cd …` 제거는 서버 tag.display 책임 (core 분류기, tests/event_tags.rs
    // 에서 잠금) — display가 오면 그것을 쓰고, 없으면 원문 그대로.
    expect(
      L(
        'tool_call',
        { tool_name: 'Bash', input: { command: 'cd /repo && git add -A && git status' } },
        undefined,
        { display: 'git add -A && git status' },
      ).secondary,
    ).toBe('git add -A && git status');
    expect(L('tool_call', { tool_name: 'Bash', input: { command: 'cd /repo && git add -A && git status' } }).secondary)
      .toBe('cd /repo && git add -A && git status');
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
    // no description → falls back to the command (서버 display가 있으면 cd 제거판)
    expect(
      L('tool_call', { tool_name: 'Bash', input: { command: 'cd /x && grep y' } }, undefined, {
        display: 'grep y',
      }).secondary,
    ).toBe('grep y');
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
  it('user_message: isMeta injection labels as skill (not "You"), so it never reads as human input', () => {
    expect(L('user_message', { content: '스킬 본문 주입' }, undefined, undefined, true).primary).toBe('skill');
  });
  it('user_message: local-command output and interrupt get their own origins', () => {
    expect(L('user_message', { content: '<local-command-stdout>x</local-command-stdout>' }).primary).toBe('command');
    expect(L('user_message', { content: '[Request interrupted by user]' }).primary).toBe('system');
  });
  it('user_message: <task-notification> labels as "알림" (NOT "You"), matching the MessageCard label', () => {
    // anchored: 전 DB 55건 user_message가 <task-notification> 선행, isMeta 없음.
    // The detail panel (InsightTab) + activity stack go through nodeLabel; without
    // this case a task-notification fell through to the "You" default, so opening
    // one via ?selected= showed "© You" in the detail header (the original gap).
    const real = '<task-notification>Background task "build" completed (exit 0).</task-notification>';
    const r = L('user_message', { content: real });
    expect(r.primary).toBe('알림');
    expect(r.kind).toBe('user');
  });
  it('hook_event: hookName from either shape', () => {
    expect(L('hook_event', { hookName: 'PreToolUse:Agent' }).secondary).toBe('PreToolUse:Agent');
    expect(L('hook_event', { hook: { hook_event_name: 'PreToolUse' } }).secondary).toBe('PreToolUse');
  });
  it('otel_span: span name from the telemetry facet (C4: not payload.raw_span)', () => {
    expect(L('otel_span', {}, { span_name: 'claude_code.interaction' }).secondary).toBe('claude_code.interaction');
    // graceful fallback when the facet is absent
    expect(L('otel_span', {}, undefined).secondary).toBe('');
    expect(L('otel_span', {}).secondary).toBe('');
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
