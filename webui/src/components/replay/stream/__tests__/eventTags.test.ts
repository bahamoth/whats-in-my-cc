import { describe, it, expect } from 'vitest';
import { tagVerb, collectUntagged } from '../eventTags';
import type { EventTagDto, ObservedEventDto } from '../../../../api/types';

// 분류(taxonomy·셸 파싱) 테스트는 Rust로 이전됐다 — tests/event_tags.rs가
// 구 eventTags.test.ts의 케이스를 1:1 잠근다 (loop-foundations 2026-06-12).
// 여기 남는 것은 표현(tagVerb)과 서버 tag 필드 기반 집계(collectUntagged)뿐.

const ev = (
  id: string,
  tag: EventTagDto | null,
  tool_name: string | null = 'Bash',
): ObservedEventDto =>
  ({
    event_id: id,
    kind: 'tool_call',
    tool_name,
    observed_at: '2026-06-12T00:00:00Z',
    tag,
    payload: {},
  }) as unknown as ObservedEventDto;

const unmatched = (token: string, display = ''): EventTagDto => ({
  value: null,
  disposition: 'unmatched',
  token,
  display,
});

describe('tagVerb', () => {
  it('extracts the verb component for chip colouring', () => {
    expect(tagVerb('read.file')).toBe('read');
    expect(tagVerb('write.vcs')).toBe('write');
    expect(tagVerb('delete.file')).toBe('delete');
    expect(tagVerb('build.code')).toBe('build');
  });
});

describe('collectUntagged — 서버 tag 필드 기반 집계', () => {
  it('aggregates unmatched events by server token with count + first eventId', () => {
    const rows = collectUntagged([
      ev('e1', unmatched('frobnicate', 'frobnicate a')),
      ev('e2', unmatched('frobnicate', 'frobnicate b')),
      ev('e3', { value: 'read.file', disposition: 'tagged', token: 'grep', display: 'grep z' }),
      ev('e4', { value: null, disposition: 'control', token: null, display: 'cd /tmp' }),
      ev('e5', null), // 태그 미계산(비 tool_call 등) → 제외
    ]);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ token: 'frobnicate', count: 2, eventId: 'e1' });
    expect(rows[0].sample).toBe('frobnicate a');
    expect(rows[0].hint).toContain('BASH_FIRST_TOKEN_TAGS');
    expect(rows[0].hint).toContain('src/insight/event_tags.rs');
  });

  it('multiplexer `tool sub` token hints at TOOL_SUBCOMMAND_TAGS', () => {
    const rows = collectUntagged([
      ev('e1', unmatched('git frobnicate', 'git frobnicate')),
      ev('e2', unmatched('git frobnicate', 'git frobnicate --x')),
    ]);
    expect(rows[0].token).toBe('git frobnicate');
    expect(rows[0].count).toBe(2);
    expect(rows[0].hint).toContain("TOOL_SUBCOMMAND_TAGS['git']");
  });

  it('unmatched Read/Edit points the loop at EXT_OBJECT, not BASH maps', () => {
    const rows = collectUntagged([
      ev('e1', unmatched('diff', '/tmp/pr41.diff'), 'Read'),
      ev('e2', unmatched('diff', '/other/x.diff'), 'Read'),
      ev('e3', unmatched('Makefile', 'Makefile'), 'Edit'),
    ]);
    const byToken = Object.fromEntries(rows.map((r) => [r.token, r]));
    expect(byToken['diff'].count).toBe(2);
    expect(byToken['diff'].hint).toContain('EXT_OBJECT');
    expect(byToken['diff'].hint).not.toContain('BASH_FIRST_TOKEN_TAGS');
    expect(byToken['Makefile'].hint).toContain('EXT_OBJECT');
  });

  it('excludes MCP tools (owned by the unidentified-plugins loop, not untagged-bash)', () => {
    const rows = collectUntagged([
      ev('e1', unmatched('frobnicate', 'frobnicate'), 'Bash'),
      // unmatched MCP tools (e.g. directly-configured servers) must NOT pollute
      // untagged-bash — they belong to the unidentified-plugins loop.
      ev('m1', unmatched('claude-in-chrome:computer'), 'mcp__claude-in-chrome__computer'),
      ev('m2', unmatched('serena:activate_project'), 'mcp__plugin_serena_serena__activate_project'),
    ]);
    expect(rows.map((r) => r.token)).toEqual(['frobnicate']);
  });

  it('rows sort by count desc and empty tokens are skipped', () => {
    const rows = collectUntagged([
      ev('a1', unmatched('aa')),
      ev('b1', unmatched('bb')),
      ev('b2', unmatched('bb')),
      ev('x1', { value: null, disposition: 'unmatched', token: null, display: '' }),
    ]);
    expect(rows.map((r) => r.token)).toEqual(['bb', 'aa']);
  });
});
