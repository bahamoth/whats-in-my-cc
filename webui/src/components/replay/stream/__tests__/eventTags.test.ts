import { describe, it, expect } from 'vitest';
import { tagForEvent, collectUntagged, meaningfulCommand, tagVerb, BASH_FIRST_TOKEN_TAGS, TOOL_SUBCOMMAND_TAGS } from '../eventTags';
import type { ObservedEventDto } from '../../../../api/types';

const bash = (command: string): ObservedEventDto =>
  ({ event_id: command, kind: 'tool_call', tool_name: 'Bash', observed_at: '2026-05-31T00:00:00Z', payload: { input: { command } } } as unknown as ObservedEventDto);
const read = (file_path: string): ObservedEventDto =>
  ({ event_id: file_path, kind: 'tool_call', tool_name: 'Read', observed_at: '2026-05-31T00:00:00Z', payload: { input: { file_path } } } as unknown as ObservedEventDto);

describe('tagForEvent — verb.object taxonomy', () => {
  it('read.file — search/inspect files', () => {
    expect(tagForEvent(bash('grep -n foo src')).tag).toBe('read.file');
    expect(tagForEvent(bash('find . -name "*.rs"')).tag).toBe('read.file');
    expect(tagForEvent(bash('ls -la')).tag).toBe('read.file');
    expect(tagForEvent(bash('cat Cargo.toml')).tag).toBe('read.file');
    expect(tagForEvent(bash("sed -n '1,5p' x")).tag).toBe('read.file');
  });
  it('read.proc / read.db / read.web', () => {
    expect(tagForEvent(bash('ps -p 1')).tag).toBe('read.proc');
    expect(tagForEvent(bash('lsof -ti :5175')).tag).toBe('read.proc');
    expect(tagForEvent(bash('sqlite3 db .tables')).tag).toBe('read.db');
    expect(tagForEvent(bash('curl -s http://x')).tag).toBe('read.web');
  });
  it('git: read vs write by subcommand', () => {
    expect(tagForEvent(bash('git status')).tag).toBe('read.vcs');
    expect(tagForEvent(bash('git diff HEAD')).tag).toBe('read.vcs');
    expect(tagForEvent(bash('git fetch')).tag).toBe('read.vcs'); // fetch is read (download only)
    expect(tagForEvent(bash('git commit -m x')).tag).toBe('write.vcs');
    expect(tagForEvent(bash('git push')).tag).toBe('write.vcs');
    expect(tagForEvent(bash('git mv a b')).tag).toBe('write.vcs');
    expect(tagForEvent(bash('gh pr create')).tag).toBe('write.vcs');
  });
  it('cargo/npm/go: subcommand decides build/test/run/lint/deps', () => {
    expect(tagForEvent(bash('cargo build --release')).tag).toBe('build.code');
    expect(tagForEvent(bash('cargo test --all')).tag).toBe('test.code');
    expect(tagForEvent(bash('cargo run -- serve')).tag).toBe('run.code');
    expect(tagForEvent(bash('cargo clippy')).tag).toBe('lint.code');
    expect(tagForEvent(bash('cargo add serde')).tag).toBe('write.deps');
    expect(tagForEvent(bash('npm test')).tag).toBe('test.code');
    expect(tagForEvent(bash('npm run dev')).tag).toBe('run.code'); // any npm run → run.code
    expect(tagForEvent(bash('npm install')).tag).toBe('write.deps');
    expect(tagForEvent(bash('go build ./...')).tag).toBe('build.code');
  });
  it('single-purpose dev tools + tsc --noEmit flips to lint', () => {
    expect(tagForEvent(bash('make')).tag).toBe('build.code');
    expect(tagForEvent(bash('vitest run')).tag).toBe('test.code');
    expect(tagForEvent(bash('eslint .')).tag).toBe('lint.code');
    expect(tagForEvent(bash('tsc -p .')).tag).toBe('build.code');
    expect(tagForEvent(bash('tsc --noEmit')).tag).toBe('lint.code');
  });
  it('run.code — interpreters, scripts, and bare path execution', () => {
    expect(tagForEvent(bash('python3 script.py')).tag).toBe('run.code');
    expect(tagForEvent(bash('node x.js')).tag).toBe('run.code');
    expect(tagForEvent(bash('bash tests/x.sh')).tag).toBe('run.code');
    expect(tagForEvent(bash('./target/release/witmcc serve')).tag).toBe('run.code');
    expect(tagForEvent(bash('/usr/local/bin/foo')).tag).toBe('run.code');
    expect(tagForEvent(bash('tests/structural/x.sh ch-prod')).tag).toBe('run.code'); // *.sh path
  });
  it('write.file / delete.file / write.deps', () => {
    expect(tagForEvent(bash('mkdir -p a/b')).tag).toBe('write.file');
    expect(tagForEvent(bash('cp a b')).tag).toBe('write.file');
    expect(tagForEvent(bash('chmod +x x')).tag).toBe('write.file');
    expect(tagForEvent(bash('rm -rf target')).tag).toBe('delete.file');
    expect(tagForEvent(bash('mv a b')).tag).toBe('delete.file');
    expect(tagForEvent(bash('pip install cairosvg')).tag).toBe('write.deps');
  });
  it('classifies compounds by the first MEANINGFUL command', () => {
    expect(tagForEvent(bash('cd /repo && git add -A && git status')).tag).toBe('write.vcs');
    expect(tagForEvent(bash('cd x && grep y')).tag).toBe('read.file');
    expect(tagForEvent(bash('grep y > out.txt')).tag).toBe('read.file');
    expect(tagForEvent(bash('grep a | grep b | wc -l')).tag).toBe('read.file');
    expect(tagForEvent(bash('cd x && rm -rf y')).tag).toBe('delete.file');
  });
  it('resolves the multiplexer subcommand past global flags (git -C/-c, cargo +toolchain)', () => {
    // git global options precede the subcommand: `git -C <dir> diff`, `git -c k=v commit`.
    expect(tagForEvent(bash('git -C .. diff --stat')).tag).toBe('read.vcs');
    expect(tagForEvent(bash('git -C /repo status --short')).tag).toBe('read.vcs');
    expect(tagForEvent(bash('git -c user.name=x commit -m y')).tag).toBe('write.vcs');
    expect(tagForEvent(bash('git --no-pager log')).tag).toBe('read.vcs');
    // cargo toolchain selector `+toolchain` precedes the subcommand.
    expect(tagForEvent(bash('cargo +1.86.0 build 2>&1')).tag).toBe('build.code');
  });
  it('unwraps a timeout wrapper to classify the inner command', () => {
    expect(tagForEvent(bash('timeout 180 npm run dev')).tag).toBe('run.code');
    expect(tagForEvent(bash('timeout 60 cargo test')).tag).toBe('test.code');
    expect(tagForEvent(bash('timeout 5s git status')).tag).toBe('read.vcs');
    // arg-consuming flags (-s SIGNAL, -k DURATION) must skip their value token too
    expect(tagForEvent(bash('timeout -s SIGTERM 5 cargo test')).tag).toBe('test.code');
    expect(tagForEvent(bash('timeout -k 10 30 npm test')).tag).toBe('test.code');
    expect(tagForEvent(bash('timeout --signal=KILL 10 npm test')).tag).toBe('test.code');
  });
  it('tags bare relative-path execution (no ./ prefix) as run.code', () => {
    expect(tagForEvent(bash('target/debug/witmcc --help')).tag).toBe('run.code');
    expect(tagForEvent(bash('.claude/skills/ch/scripts/ch ch-prod')).tag).toBe('run.code');
  });
  it('just / shasum single-purpose tools', () => {
    expect(tagForEvent(bash('just webui-build')).tag).toBe('run.code');
    expect(tagForEvent(bash('shasum -a 256 file.pdf')).tag).toBe('read.file');
  });
  it('control vs unmatched', () => {
    expect(tagForEvent(bash('cd /tmp && echo done')).disposition).toBe('control');
    expect(tagForEvent(bash('cd /tmp')).disposition).toBe('control');
    expect(tagForEvent(bash('frobnicate x')).disposition).toBe('unmatched');
    expect(tagForEvent(bash('git frobnicate')).disposition).toBe('unmatched'); // unknown git sub
    expect(tagForEvent(bash('npm frob')).disposition).toBe('unmatched'); // unknown npm sub
  });

  // ── classifier hardening (tokenizer noise) ──
  it('strips leading whole-line comments before classifying', () => {
    expect(tagForEvent(bash('# explore\ngrep -r x src')).tag).toBe('read.file');
  });
  it('splits compounds on NEWLINES', () => {
    expect(tagForEvent(bash('cd /x\ngrep y')).tag).toBe('read.file');
    expect(tagForEvent(bash('cargo build\ncargo test')).tag).toBe('build.code');
  });
  it('does NOT mis-split a 2>&1 redirect', () => {
    expect(tagForEvent(bash('grep x src 2>&1 | head')).tag).toBe('read.file');
    expect(tagForEvent(bash('cargo test 2>&1')).tag).toBe('test.code');
  });
  it('skips leading VAR=value assignment prefixes', () => {
    expect(tagForEvent(bash('VAULT=/x grep y')).tag).toBe('read.file');
    expect(tagForEvent(bash('FOO=/x\ncat f')).tag).toBe('read.file');
    expect(tagForEvent(bash('FOO=bar')).disposition).toBe('control');
  });
  it('treats loop/conditional keywords as control', () => {
    expect(tagForEvent(bash('for f in *; do grep x "$f"; done')).tag).toBe('read.file');
    expect(tagForEvent(bash('[ -f x ] && cat y')).tag).toBe('read.file');
  });
});

describe('tagForEvent — Read by extension', () => {
  it('classifies code/docs/config/data', () => {
    expect(tagForEvent(read('src/a.rs')).tag).toBe('read.code');
    expect(tagForEvent(read('webui/x.tsx')).tag).toBe('read.code');
    expect(tagForEvent(read('README.md')).tag).toBe('read.docs');
    expect(tagForEvent(read('Cargo.toml')).tag).toBe('read.config');
    expect(tagForEvent(read('data.json')).tag).toBe('read.data');
  });
});

describe('classifier invariants', () => {
  it('no map key contains a slash (isPathExec runs FIRST and would shadow it)', () => {
    // isPathExec tags any unquoted slash-containing first token as run.code BEFORE
    // the tsc/multiplexer/BASH lookups — so a slash in a key would be unreachable.
    for (const k of Object.keys(BASH_FIRST_TOKEN_TAGS)) expect(k).not.toContain('/');
    for (const k of Object.keys(TOOL_SUBCOMMAND_TAGS)) expect(k).not.toContain('/');
  });
});

describe('tagVerb', () => {
  it('extracts the verb component for chip colouring', () => {
    expect(tagVerb('read.file')).toBe('read');
    expect(tagVerb('write.vcs')).toBe('write');
    expect(tagVerb('delete.file')).toBe('delete');
    expect(tagVerb('build.code')).toBe('build');
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
    expect(meaningfulCommand('cd /tmp')).toBe('cd /tmp');
  });
});

describe('collectUntagged', () => {
  it('aggregates unmatched commands by the first MEANINGFUL token, with count + sample', () => {
    const events = [
      bash('frobnicate a'), bash('frobnicate b'),
      bash('cd /tmp'),                  // control → excluded
      bash('cd x && grep y'),           // tagged read.file → excluded
      bash('grep z'),                   // tagged → excluded
      bash('cd /repo && frobnicate c'), // unmatched; meaningful token = frobnicate
    ];
    const rows = collectUntagged(events);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ token: 'frobnicate', count: 3 });
    expect(rows[0].hint).toContain('BASH_FIRST_TOKEN_TAGS');
  });

  it('aggregates an unknown MULTIPLEXER subcommand as `tool sub` (so the loop knows which sub to add)', () => {
    const rows = collectUntagged([bash('git frobnicate'), bash('git frobnicate --x')]);
    expect(rows[0].token).toBe('git frobnicate');
    expect(rows[0].count).toBe(2);
    expect(rows[0].hint).toContain("TOOL_SUBCOMMAND_TAGS['git']");
  });

  it('aggregates an unknown subcommand past global flags under the same `tool sub`', () => {
    // `git -C .. frobnicate` and `git frobnicate` are the SAME unknown sub.
    const rows = collectUntagged([bash('git -C .. frobnicate'), bash('git frobnicate')]);
    expect(rows[0].token).toBe('git frobnicate');
    expect(rows[0].count).toBe(2);
  });

  it('does not surface comment / assignment / redirect noise as untagged tokens', () => {
    const events = [
      bash('# explore\nfrobnicate view'),  // → frobnicate (comment stripped)
      bash('VAULT=/x\nfrobnicate list'),    // → frobnicate (assignment skipped, newline split)
      bash('grep x 2>&1 | head'),           // tagged read.file → excluded
    ];
    const rows = collectUntagged(events);
    expect(rows.map((r) => r.token)).toEqual(['frobnicate']);
    expect(rows[0].count).toBe(2);
  });

  it('carries the FIRST occurrence event_id so the panel can link to its card', () => {
    const rows = collectUntagged([bash('frobnicate one'), bash('frobnicate two')]);
    expect(rows[0].eventId).toBe('frobnicate one');
  });
});
