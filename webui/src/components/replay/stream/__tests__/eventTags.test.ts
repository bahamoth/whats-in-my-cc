import { describe, it, expect } from 'vitest';
import { tagForEvent, collectUntagged } from '../eventTags';
import type { ObservedEventDto } from '../../../../api/types';

const bash = (command: string): ObservedEventDto =>
  ({ event_id: command, kind: 'tool_call', tool_name: 'Bash', observed_at: '2026-05-31T00:00:00Z', payload: { input: { command } } } as unknown as ObservedEventDto);
const read = (file_path: string): ObservedEventDto =>
  ({ event_id: file_path, kind: 'tool_call', tool_name: 'Read', observed_at: '2026-05-31T00:00:00Z', payload: { input: { file_path } } } as unknown as ObservedEventDto);

describe('tagForEvent — Bash (real-data anchored tokens)', () => {
  it('tags read/search tools', () => {
    expect(tagForEvent(bash('grep -n foo src')).tag).toBe('search·read');
    expect(tagForEvent(bash('find . -name "*.rs"')).tag).toBe('search·read');
    expect(tagForEvent(bash('ls -la')).tag).toBe('search·read');
    expect(tagForEvent(bash('cat Cargo.toml')).tag).toBe('search·read');
  });
  it('splits git into read vs write by subcommand', () => {
    expect(tagForEvent(bash('git status')).tag).toBe('vcs-read');
    expect(tagForEvent(bash('git diff HEAD')).tag).toBe('vcs-read');
    expect(tagForEvent(bash('git commit -m x')).tag).toBe('vcs-write');
    expect(tagForEvent(bash('git push')).tag).toBe('vcs-write');
  });
  it('tags build/test and query/script', () => {
    expect(tagForEvent(bash('cargo test --all')).tag).toBe('build·test');
    expect(tagForEvent(bash('npm run dev')).tag).toBe('build·test');
    expect(tagForEvent(bash('sqlite3 db .tables')).tag).toBe('query·script');
    expect(tagForEvent(bash('python3 script.py')).tag).toBe('query·script');
  });
  it('marks rm/mv as destructive', () => {
    expect(tagForEvent(bash('rm -rf target')).tag).toBe('destructive');
    expect(tagForEvent(bash('mv a b')).tag).toBe('destructive');
  });
  it('treats compound/redirect commands as ambiguous (no tag, show command)', () => {
    expect(tagForEvent(bash('cd x && grep y')).disposition).toBe('ambiguous');
    expect(tagForEvent(bash('grep y > out.txt')).disposition).toBe('ambiguous');
    expect(tagForEvent(bash('grep a | grep b | wc -l')).disposition).toBe('ambiguous');
    expect(tagForEvent(bash('cd x && grep y')).tag).toBeNull();
  });
  it('treats shell-control tokens as control (no chip, not untagged)', () => {
    expect(tagForEvent(bash('cd /tmp')).disposition).toBe('control');
    expect(tagForEvent(bash('echo hi')).disposition).toBe('control');
  });
  it('marks unrecognized SIMPLE first-tokens as unmatched (panel candidates)', () => {
    expect(tagForEvent(bash('gh pr view')).disposition).toBe('unmatched');
    expect(tagForEvent(bash('frobnicate x')).disposition).toBe('unmatched');
  });
});

describe('tagForEvent — Read by extension', () => {
  it('classifies code/docs/config/data', () => {
    expect(tagForEvent(read('src/a.rs')).tag).toBe('code');
    expect(tagForEvent(read('webui/x.tsx')).tag).toBe('code');
    expect(tagForEvent(read('README.md')).tag).toBe('docs');
    expect(tagForEvent(read('Cargo.toml')).tag).toBe('config');
    expect(tagForEvent(read('data.json')).tag).toBe('data');
  });
});

describe('tagForEvent — other tools get no chip', () => {
  it('Edit/Agent → control disposition (no chip)', () => {
    const edit = { event_id: 'e', kind: 'tool_call', tool_name: 'Edit', observed_at: '2026-05-31T00:00:00Z', payload: {} } as unknown as ObservedEventDto;
    expect(tagForEvent(edit).disposition).toBe('control');
    expect(tagForEvent(edit).tag).toBeNull();
  });
});

describe('collectUntagged', () => {
  it('aggregates only unmatched simple Bash by first token with count + sample, excludes control/ambiguous, and drops a token once a rule is added', () => {
    const events = [bash('gh pr view 1'), bash('gh pr view 2'), bash('cd /tmp'), bash('cd x && grep y'), bash('grep z')];
    const rows = collectUntagged(events);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ token: 'gh', count: 2 });
    expect(rows[0].sample).toContain('gh pr view');
    expect(rows[0].hint).toContain("BASH_FIRST_TOKEN_TAGS");
  });
});
