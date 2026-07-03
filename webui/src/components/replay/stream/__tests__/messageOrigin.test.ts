import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { describe, it, expect } from 'vitest';
import { messageOrigin } from '../messageOrigin';

// Payload shapes below are faithful copies of REAL `type:"user"` transcript
// records observed in this remote session (CC 2.1.176, entrypoint
// remote_mobile) and frozen in
// tests/fixtures/transcripts/real/message_origin_v01.jsonl — sample 1 session.
// The invariant: human input carries no command marker and isMeta=false (a
// harness `<system-reminder>` wrapper does NOT make it non-human); every
// command/skill/output/caveat record carries either a marker or isMeta=true.

describe('messageOrigin — deterministic caller classification of user_message', () => {
  it('plain text (isMeta false, no marker) is human input', () => {
    expect(messageOrigin({ payload: { content: '이 PR 리뷰해줘' }, is_meta: false }).origin).toBe('human');
  });

  it('a <system-reminder>-wrapped turn is still human (the wrapper is harness-injected, not a command)', () => {
    const real = '<system-reminder>Message sent at Sat 2026-06-13 03:54:24 UTC.</system-reminder>\n현재 진행중인 PR을 리뷰하고';
    expect(messageOrigin({ payload: { content: real }, is_meta: false }).origin).toBe('human');
  });

  it('<command-name> marker is a command invocation and exposes the command name', () => {
    const real = '<command-name>/model</command-name>\n  <command-message>model</command-message>\n  <command-args>claude-opus-4-8</command-args>';
    const r = messageOrigin({ payload: { content: real }, is_meta: false });
    expect(r.origin).toBe('command');
    expect(r.commandName).toBe('/model');
  });

  it('<local-command-stdout> / <local-command-caveat> are command output (caveat is also isMeta:true in real data)', () => {
    expect(messageOrigin({ payload: { content: '<local-command-stdout>Set model to x</local-command-stdout>' }, is_meta: false }).origin).toBe('command-output');
    expect(messageOrigin({ payload: { content: '<local-command-caveat>Caveat: ...</local-command-caveat>' }, is_meta: true }).origin).toBe('command-output');
  });

  it('[Request interrupted is a system beat', () => {
    expect(messageOrigin({ payload: { content: '[Request interrupted by user]' }, is_meta: false }).origin).toBe('system');
  });

  it('isMeta:true plain text (no marker) is a skill/command injection — NOT human', () => {
    // The real leak: a /review skill body injected as type:"user" + isMeta:true.
    expect(messageOrigin({ payload: { content: 'Review the pull request thoroughly...' }, is_meta: true }).origin).toBe('skill');
  });

  it('"Base directory for this skill:" is a skill injection even without isMeta', () => {
    expect(messageOrigin({ payload: { content: 'Base directory for this skill: /x' }, is_meta: false }).origin).toBe('skill');
  });

  it('reads text from either content or text, and a number is_meta (NULL TEXT row mapping) is truthy', () => {
    expect(messageOrigin({ payload: { text: 'human typed this' }, is_meta: 0 }).origin).toBe('human');
    expect(messageOrigin({ payload: { text: 'injected' }, is_meta: 1 }).origin).toBe('skill');
  });

  it('<task-notification> leading marker is a harness notification, NOT human "You"', () => {
    // anchored: 전 DB 55건 user_message가 <task-notification> 선행, isMeta 없음.
    // The harness injects background-task completion notices as a user-role
    // record beginning with <task-notification>; without this marker it falls
    // through to the human default and posts as "You" (a gap the user did not type).
    const real = '<task-notification>Background task "build" completed (exit 0).</task-notification>';
    const r = messageOrigin({ payload: { content: real }, is_meta: false });
    expect(r.origin).toBe('notification');
    expect(r.commandName).toBeNull();
  });
});

// Teammate 세션 (CC 2.1.198, 2026-07-03 실측 — teammate_v01 fixture, 표본 1):
// named Agent 스폰의 응답·유휴 알림은 리드 transcript에 type:"user"·isMeta 없음
// 으로 접히고, 본문이 "Another Claude session sent a message:\n<teammate-message
// teammate_id=…>"로 시작한다. 마커 미등록 시 사람 "You" 버블로 새는 갭 —
// message_origin_v01 라운드(주입 텍스트 누수)와 동형.
describe('messageOrigin — teammate messages (teammate_v01, frozen real records)', () => {
  const leadFixture = resolve(
    dirname(fileURLToPath(import.meta.url)),
    '../../../../../../tests/fixtures/transcripts/real/teammate_v01/lead_teammate_messages.jsonl',
  );
  const leadRecords = readFileSync(leadFixture, 'utf8')
    .split('\n')
    .filter((l) => l.trim())
    .map((l) => JSON.parse(l) as { isMeta?: boolean; message: { content: string } });

  it('lead-side teammate reply is origin teammate with the sender id, never human', () => {
    expect(leadRecords.length).toBeGreaterThan(0);
    for (const r of leadRecords) {
      const res = messageOrigin({ payload: { content: r.message.content }, is_meta: r.isMeta });
      expect(res.origin).toBe('teammate');
      expect(res.teammateId).toBe('explore-ingest');
    }
  });

  it('teammate-side inbound prompt (direct <teammate-message …> lead) classifies too', () => {
    // teammate transcript의 첫 user 레코드는 접두문 없이 마커로 바로 시작한다.
    const headFixture = resolve(
      dirname(fileURLToPath(import.meta.url)),
      '../../../../../../tests/fixtures/transcripts/real/teammate_v01/teammate_session_head.jsonl',
    );
    const user = readFileSync(headFixture, 'utf8')
      .split('\n')
      .filter((l) => l.trim())
      .map((l) => JSON.parse(l) as { type: string; message?: { content?: unknown } })
      .find((r) => r.type === 'user');
    expect(user).toBeDefined();
    const content = user!.message!.content as string;
    const res = messageOrigin({ payload: { content }, is_meta: false });
    expect(res.origin).toBe('teammate');
    expect(res.teammateId).toBe('team-lead');
  });
});

// Real-data anchor (CLAUDE.md "Real-data anchoring"): invariants asserted
// against FROZEN real transcript records, not hand-written shapes.
describe('messageOrigin — frozen real records (message_origin_v01)', () => {
  const fixture = resolve(
    dirname(fileURLToPath(import.meta.url)),
    '../../../../../../tests/fixtures/transcripts/real/message_origin_v01.jsonl',
  );
  const records = readFileSync(fixture, 'utf8')
    .split('\n')
    .filter((l) => l.trim())
    .map((l) => JSON.parse(l) as { isMeta?: boolean; message: { content: unknown } });

  // Map content (string OR [{type:'text',text}]) to the payload shape the DTO
  // carries, then assert the deterministic origin the real record must yield.
  const textOf = (content: unknown): string =>
    typeof content === 'string'
      ? content
      : Array.isArray(content)
      ? content.map((b) => (b && typeof b === 'object' ? ((b as Record<string, unknown>).text as string) ?? '' : '')).join('')
      : '';
  const expected = (text: string, isMeta: boolean): string => {
    const s = text.trimStart();
    if (s.startsWith('<command-name>')) return 'command';
    if (s.startsWith('<local-command-stdout>')) return 'command-output';
    if (s.startsWith('<local-command-caveat>')) return 'command-output';
    if (isMeta) return 'skill';
    return 'human';
  };

  it('classifies every frozen real user record deterministically (no human/scaffold confusion)', () => {
    expect(records.length).toBeGreaterThan(0);
    for (const r of records) {
      const text = textOf(r.message.content);
      expect(messageOrigin({ payload: { content: text }, is_meta: r.isMeta }).origin).toBe(
        expected(text, !!r.isMeta),
      );
    }
    // The <system-reminder>-wrapped record is the human one — confirm it exists
    // and is NOT misread as scaffolding.
    const human = records.find((r) => textOf(r.message.content).includes('<system-reminder>'));
    expect(human).toBeDefined();
    expect(messageOrigin({ payload: { content: textOf(human!.message.content) }, is_meta: human!.isMeta }).origin).toBe('human');
  });
});
