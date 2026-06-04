import { describe, it, expect } from 'vitest';
import { tagForEvent, collectUntagged, meaningfulCommand } from '../eventTags';
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
  it('classifies compound commands by the first MEANINGFUL command, skipping control prefixes', () => {
    // `cd … && git add … && git status` is a git-add (vcs-write), not an
    // untaggable compound — split on separators, skip the `cd`, read `git add`.
    const r = tagForEvent(bash('cd /repo && git add -A && git status'));
    expect(r.tag).toBe('vcs-write');
    expect(r.disposition).toBe('tagged');
    // cd skipped → grep is the work
    expect(tagForEvent(bash('cd x && grep y')).tag).toBe('search·read');
    // redirects / pipes do not block tagging — classify by the first command
    expect(tagForEvent(bash('grep y > out.txt')).tag).toBe('search·read');
    expect(tagForEvent(bash('grep a | grep b | wc -l')).tag).toBe('search·read');
    // destructive after a control prefix is still caught
    expect(tagForEvent(bash('cd x && rm -rf y')).tag).toBe('destructive');
  });
  it('compound of only control tokens → control; first meaningful unknown → unmatched', () => {
    expect(tagForEvent(bash('cd /tmp && echo done')).disposition).toBe('control');
    expect(tagForEvent(bash('cd x && gh pr view')).disposition).toBe('unmatched');
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

describe('meaningfulCommand — strip leading control prefixes for display', () => {
  it('drops a leading cd so the command leads with the real work', () => {
    expect(meaningfulCommand('cd /repo && git add -A && git status')).toBe('git add -A && git status');
    expect(meaningfulCommand('cd x && grep y')).toBe('grep y');
  });
  it('leaves a command that already leads with work unchanged', () => {
    expect(meaningfulCommand('grep a | grep b')).toBe('grep a | grep b');
    expect(meaningfulCommand('rm -f x && ls')).toBe('rm -f x && ls');
    expect(meaningfulCommand('cd /tmp')).toBe('cd /tmp'); // all control → as-is
  });
});

describe('collectUntagged', () => {
  it('aggregates unmatched Bash by the first MEANINGFUL token (skips control prefixes), with count + sample', () => {
    const events = [
      bash('gh pr view 1'), bash('gh pr view 2'),
      bash('cd /tmp'),                 // control → excluded
      bash('cd x && grep y'),          // now tagged search·read → excluded
      bash('grep z'),                  // tagged → excluded
      bash('cd /repo && gh pr merge'), // unmatched; meaningful token is gh, not cd
    ];
    const rows = collectUntagged(events);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ token: 'gh', count: 3 });
    expect(rows[0].hint).toContain('BASH_FIRST_TOKEN_TAGS');
  });

  it('carries the FIRST occurrence event_id so the panel can link to its card', () => {
    const events = [
      bash('gh pr view 1'), // first 'gh' → its event_id is the jump target
      bash('gh pr view 2'),
    ];
    const rows = collectUntagged(events);
    expect(rows[0].eventId).toBe('gh pr view 1');
  });
});
