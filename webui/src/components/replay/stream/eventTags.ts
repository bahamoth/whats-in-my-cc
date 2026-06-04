// webui/src/components/replay/stream/eventTags.ts
import type { ObservedEventDto } from '../../../api/types';

// ── Taxonomy: every tag is `verb.object` ──────────────────────────────────
//   verbs   : read · write · delete · build · test · run · lint
//   objects : code · docs · config · data · file · proc · vcs · db · web · deps
// Principle: I/O operations are read/write/delete on an object (file/vcs/db/web/
// proc); the rest are execution actions on the codebase (build/test/run/lint).
// The chip is coloured by the VERB (the part before the dot).
export type Tag =
  | 'read.code' | 'read.docs' | 'read.config' | 'read.data'
  | 'read.file' | 'read.proc' | 'read.vcs' | 'read.db' | 'read.web'
  | 'write.file' | 'write.vcs' | 'write.deps'
  | 'delete.file'
  | 'build.code' | 'test.code' | 'run.code' | 'lint.code';

/** The verb (action) component of a tag — used for chip colouring/grouping. */
export type TagVerb = 'read' | 'write' | 'delete' | 'build' | 'test' | 'run' | 'lint';
export function tagVerb(tag: Tag): TagVerb {
  return tag.slice(0, tag.indexOf('.')) as TagVerb;
}

export type Disposition = 'tagged' | 'control' | 'unmatched';
export interface TagResult { tag: Tag | null; disposition: Disposition; }

// ── single source of truth — add a key to extend ──────────────────────────
// Read tool: file content type by extension.
export const READ_EXT_TAGS: Record<string, Tag> = {
  rs: 'read.code', ts: 'read.code', tsx: 'read.code', js: 'read.code', jsx: 'read.code', css: 'read.code',
  md: 'read.docs', html: 'read.docs', txt: 'read.docs',
  toml: 'read.config', yaml: 'read.config', yml: 'read.config', ini: 'read.config',
  json: 'read.data', sql: 'read.data', jsonl: 'read.data', log: 'read.data', csv: 'read.data',
};

// Bash SINGLE-PURPOSE first tokens → tag. Multiplexers (git/cargo/npm/…) whose
// subcommand decides the verb live in TOOL_SUBCOMMAND_TAGS; bare path execution
// (`./x`, `/abs`, `*.sh`) is detected as run.code.
export const BASH_FIRST_TOKEN_TAGS: Record<string, Tag> = {
  // read.file — search / inspect files & dirs
  grep: 'read.file', rg: 'read.file', egrep: 'read.file', fgrep: 'read.file', find: 'read.file',
  ls: 'read.file', cat: 'read.file', head: 'read.file', tail: 'read.file', wc: 'read.file',
  jq: 'read.file', tree: 'read.file', which: 'read.file', file: 'read.file', stat: 'read.file',
  du: 'read.file', df: 'read.file', sed: 'read.file', awk: 'read.file', pwd: 'read.file', realpath: 'read.file',
  // read.proc — process / port inspection
  ps: 'read.proc', lsof: 'read.proc',
  // read.db
  sqlite3: 'read.db', psql: 'read.db', mysql: 'read.db',
  // read.web
  curl: 'read.web', wget: 'read.web',
  // write.file — create / modify (non-destructive)
  mkdir: 'write.file', touch: 'write.file', cp: 'write.file', chmod: 'write.file', chown: 'write.file', ln: 'write.file',
  // write.deps — dependency management
  pip: 'write.deps', pip3: 'write.deps',
  // delete.file — destructive
  rm: 'delete.file', mv: 'delete.file', rmdir: 'delete.file',
  // run.code — execute interpreters / scripts / package binaries
  python3: 'run.code', python: 'run.code', node: 'run.code', ruby: 'run.code', osascript: 'run.code',
  bash: 'run.code', sh: 'run.code', zsh: 'run.code', npx: 'run.code', markitdown: 'run.code',
  // read.file — hash / inspect a file's content
  shasum: 'read.file', sha256sum: 'read.file', md5: 'read.file', md5sum: 'read.file',
  // run.code — task runners (just/make-like) executing project scripts
  just: 'run.code',
  // build / test / lint — single-purpose dev tools
  make: 'build.code',
  vitest: 'test.code', jest: 'test.code', pytest: 'test.code',
  eslint: 'lint.code', ruff: 'lint.code', prettier: 'lint.code',
  // vcs (non-git)
  gh: 'write.vcs',
};

// Multiplexer tools: the SUBCOMMAND decides the verb. An unknown subcommand is
// `unmatched` (no default) so the tagging loop surfaces `tool sub` to be added
// here — new subcommands are never silently mis-tagged.
export const TOOL_SUBCOMMAND_TAGS: Record<string, Record<string, Tag>> = {
  git: {
    status: 'read.vcs', log: 'read.vcs', diff: 'read.vcs', show: 'read.vcs', branch: 'read.vcs',
    blame: 'read.vcs', 'rev-parse': 'read.vcs', describe: 'read.vcs', fetch: 'read.vcs',
    remote: 'read.vcs', config: 'read.vcs', 'ls-files': 'read.vcs', shortlog: 'read.vcs',
    add: 'write.vcs', commit: 'write.vcs', push: 'write.vcs', checkout: 'write.vcs', switch: 'write.vcs',
    stash: 'write.vcs', rm: 'write.vcs', mv: 'write.vcs', reset: 'write.vcs', merge: 'write.vcs',
    rebase: 'write.vcs', pull: 'write.vcs', tag: 'write.vcs', clone: 'write.vcs', init: 'write.vcs',
    restore: 'write.vcs', 'cherry-pick': 'write.vcs', revert: 'write.vcs', apply: 'write.vcs', worktree: 'write.vcs',
  },
  cargo: {
    build: 'build.code', b: 'build.code', test: 'test.code', t: 'test.code', nextest: 'test.code',
    run: 'run.code', r: 'run.code', check: 'lint.code', clippy: 'lint.code', fmt: 'lint.code',
    add: 'write.deps', update: 'write.deps', remove: 'write.deps',
  },
  npm: { install: 'write.deps', i: 'write.deps', ci: 'write.deps', add: 'write.deps', test: 'test.code', t: 'test.code', start: 'run.code', run: 'run.code' },
  pnpm: { install: 'write.deps', i: 'write.deps', add: 'write.deps', test: 'test.code', start: 'run.code', run: 'run.code' },
  yarn: { install: 'write.deps', add: 'write.deps', test: 'test.code', start: 'run.code', run: 'run.code' },
  go: { build: 'build.code', test: 'test.code', run: 'run.code', vet: 'lint.code', get: 'write.deps', install: 'write.deps' },
};

// Global options that PRECEDE a multiplexer's subcommand and would otherwise be
// mis-read AS the subcommand: `git -C <dir> diff`, `git -c k=v commit`,
// `git --no-pager log`, `cargo +1.86.0 build`. Per tool, the flags that consume
// a following argument (so we skip the arg too). Anchored to real corpus:
// `git -C`/`-c` dominated the untagged set (240 occurrences).
const SUBCOMMAND_ARG_FLAGS: Record<string, Set<string>> = {
  git: new Set(['-C', '-c']),
};
/** Resolve a multiplexer's real subcommand from the text after the tool token,
 *  skipping leading global options (`-x`, `--x`, arg-consuming `-C <dir>`) and a
 *  `+toolchain` selector (cargo). Returns '' when no subcommand remains. */
function resolveSubcommand(tool: string, rest: string): string {
  const toks = rest.split(/\s+/).filter(Boolean);
  const argFlags = SUBCOMMAND_ARG_FLAGS[tool];
  let i = 0;
  while (i < toks.length) {
    const t = toks[i];
    if (t.startsWith('+')) { i += 1; continue; }                 // cargo +toolchain
    if (t.startsWith('-')) { i += argFlags?.has(t) ? 2 : 1; continue; } // global flag (+ its arg)
    break;
  }
  return (toks[i] ?? '').toLowerCase();
}

export const CONTROL_TOKENS = new Set([
  'cd', 'echo', 'sleep', 'for', 'export', 'source', 'set', 'pgrep', 'kill', 'pkill', 'wait', 'true', ':',
  // shell loop/conditional keywords + the test builtin: a segment that IS one of
  // these is control (no work to classify here).
  'while', 'until', 'if', 'case', 'esac', 'done', 'fi', '[', '[[', 'test',
]);
// Control-flow keywords that PRECEDE a command on the same segment (`do grep …`,
// `then cat …`). Unlike CONTROL_TOKENS they are stripped as a prefix so the real
// command after them is classified.
const PREFIX_KEYWORDS = new Set(['do', 'then', 'else', 'elif']);
// A leading `NAME=value` shell variable assignment (env prefix). Stripped like a
// prefix so `VAR=x grep …` classifies as the grep, not as an unknown `var=x`.
const ASSIGNMENT = /^[A-Za-z_][A-Za-z0-9_]*=/;
// Command SEQUENCERS — split a compound into independent commands. We split on
// these + NEWLINES (multi-line scripts), but NOT on a bare `&` (it would shred
// `2>&1` / `&>` redirects into bogus tokens) nor redirects `>`/`<` / subshells.
const COMMAND_SEPARATORS = /(?:&&|\|\||;|\||\n)/;

function ext(path: string): string {
  const base = path.split('/').pop() ?? '';
  const i = base.lastIndexOf('.');
  return i > 0 ? base.slice(i + 1).toLowerCase() : '';
}
function firstToken(cmd: string): string {
  const t = cmd.trim();
  const sp = t.indexOf(' ');
  return (sp > 0 ? t.slice(0, sp) : t).toLowerCase();
}

/** Drop whole-line `#` comments before any analysis, so a command that leads
 *  with a comment line (`# note\ngrep …`) classifies by the real command, not
 *  by the `#` token. Only lines that START with `#` are removed (a `#` mid-line
 *  may be inside a quoted string). */
function stripCommentLines(cmd: string): string {
  return cmd
    .split('\n')
    .filter((line) => !/^\s*#/.test(line))
    .join('\n')
    .trim();
}

/** The actual command of a segment, with leading `NAME=value` assignments and
 *  control-flow prefix keywords (`do`/`then`/…) stripped. '' when the segment is
 *  only assignments/prefixes (e.g. a bare `do` or `FOO=bar`). */
function commandOf(segment: string): string {
  let s = segment.trim();
  for (let guard = 0; guard < 12; guard++) {
    if (ASSIGNMENT.test(s)) {
      const sp = s.indexOf(' ');
      if (sp < 0) return ''; // pure assignment, no command follows
      s = s.slice(sp + 1).trim();
      continue;
    }
    if (PREFIX_KEYWORDS.has(firstToken(s))) {
      const sp = s.indexOf(' ');
      if (sp < 0) return ''; // bare `do` / `then`
      s = s.slice(sp + 1).trim();
      continue;
    }
    // `timeout [flags] DURATION cmd…` — a wrapper that runs an inner command.
    // Strip `timeout`, any leading `-flags`, and one duration token (180 / 5s)
    // so the INNER command (npm/cargo/…) is what classifies.
    if (firstToken(s) === 'timeout') {
      let rest = s.slice('timeout'.length).trim();
      while (rest.startsWith('-')) {
        const sp = rest.indexOf(' ');
        if (sp < 0) { rest = ''; break; }
        rest = rest.slice(sp + 1).trim();
      }
      if (/^\d+(\.\d+)?[smhd]?$/.test(firstToken(rest))) {
        const sp = rest.indexOf(' ');
        rest = sp < 0 ? '' : rest.slice(sp + 1).trim();
      }
      if (rest === '') return '';
      s = rest;
      continue;
    }
    return s;
  }
  return s;
}

/** Split a compound shell command into its sequenced sub-commands. */
export function segmentCommand(cmd: string): string[] {
  return cmd.split(COMMAND_SEPARATORS).map((s) => s.trim()).filter(Boolean);
}

/** The first segment that carries real work — its command (assignments/prefix
 *  keywords stripped) is non-empty and not a control token. null when every
 *  segment is control/assignment (e.g. `cd x && echo y`, `FOO=bar`). */
function firstMeaningfulSegment(segments: string[]): string | null {
  return (
    segments.find((s) => {
      const c = commandOf(s);
      return c !== '' && !CONTROL_TOKENS.has(firstToken(c));
    }) ?? null
  );
}

/** The command from its first meaningful sub-command onward, for DISPLAY —
 *  leading control prefixes like `cd /path &&` are dropped so the shown command
 *  leads with the actual work. Falls back to the trimmed original when every
 *  sub-command is control (e.g. a bare `cd /tmp`). */
export function meaningfulCommand(cmd: string): string {
  let s = cmd.trim();
  for (let guard = 0; guard < 6; guard++) {
    const m = s.match(/^(.*?)(?:&&|\|\||;|\|)(.*)$/s); // split at the FIRST separator (no bare &)
    if (!m) break;
    const head = commandOf(m[1].trim());
    if (head !== '' && !CONTROL_TOKENS.has(firstToken(head))) break;
    s = m[2].trim();
  }
  return s || cmd.trim();
}

/** A command run directly by path → run.code: `./x`, `../x`, `/abs/x`, `*.sh`,
 *  or a bare relative path with a slash (`target/debug/witmcc`,
 *  `.claude/skills/ch/scripts/ch`). A token CONTAINING a slash in command
 *  position names a file to execute. Quoted tokens are excluded — those are
 *  heredoc/string-body fragments, not commands. */
function isPathExec(tok: string): boolean {
  if (tok.endsWith('.sh')) return true;
  if (tok.startsWith('"') || tok.startsWith("'")) return false;
  return tok.includes('/');
}

/** Classify a single (control-prefix-stripped) command string. */
function classifyCommand(cmdStr: string): TagResult {
  const tok = firstToken(cmdStr);
  if (!tok || CONTROL_TOKENS.has(tok)) return { tag: null, disposition: 'control' };
  if (isPathExec(tok)) return { tag: 'run.code', disposition: 'tagged' };
  // tsc is a single tool whose intent flips with --noEmit (type-check = lint).
  if (tok === 'tsc') {
    return { tag: cmdStr.includes('--noEmit') ? 'lint.code' : 'build.code', disposition: 'tagged' };
  }
  // multiplexer: subcommand decides the verb (unknown → unmatched).
  const subMap = TOOL_SUBCOMMAND_TAGS[tok];
  if (subMap) {
    const sub = resolveSubcommand(tok, cmdStr.slice(tok.length).trim());
    const t = subMap[sub];
    return t ? { tag: t, disposition: 'tagged' } : { tag: null, disposition: 'unmatched' };
  }
  const t = BASH_FIRST_TOKEN_TAGS[tok];
  if (t) return { tag: t, disposition: 'tagged' };
  return { tag: null, disposition: 'unmatched' };
}

export function tagForEvent(e: ObservedEventDto): TagResult {
  const tool = e.tool_name;
  const input = ((e.payload as Record<string, unknown>)?.input ?? {}) as Record<string, unknown>;

  if (tool === 'Read') {
    const fp = typeof input.file_path === 'string' ? input.file_path : '';
    const tag = READ_EXT_TAGS[ext(fp)];
    return tag ? { tag, disposition: 'tagged' } : { tag: null, disposition: 'unmatched' };
  }
  if (tool === 'Bash' || tool === 'bash') {
    const cmd = stripCommentLines(typeof input.command === 'string' ? input.command : '');
    if (!cmd) return { tag: null, disposition: 'control' };
    // Classify a compound (`cd … && git add … && git status`) by its first
    // MEANINGFUL sub-command: split on separators + newlines, skip control
    // segments (cd) and assignment/keyword prefixes (`VAR=x`, `do`).
    for (const seg of segmentCommand(cmd)) {
      const cmdStr = commandOf(seg);
      const tok = firstToken(cmdStr);
      if (!tok || CONTROL_TOKENS.has(tok)) continue; // empty/assignment-only or control → next segment
      return classifyCommand(cmdStr);
    }
    return { tag: null, disposition: 'control' }; // every segment was control
  }
  // every other tool: tool name is the label → no chip
  return { tag: null, disposition: 'control' };
}

export interface UntaggedRow {
  token: string;
  count: number;
  sample: string;
  hint: string;
  /** event_id of the FIRST occurrence — the panel links to this card. */
  eventId: string;
}

/** Aggregation token for an untagged command. Multiplexers aggregate by
 *  `tool sub` (e.g. `git worktree`, `cargo bench`) so the panel/loop points at
 *  the exact subcommand to add; everything else by its first token. */
function untaggedToken(cmdStr: string): string {
  const tok = firstToken(cmdStr);
  if (TOOL_SUBCOMMAND_TAGS[tok]) {
    const sub = resolveSubcommand(tok, cmdStr.slice(tok.length).trim());
    return sub ? `${tok} ${sub}` : tok;
  }
  return tok;
}

function untaggedHint(token: string): string {
  return token.includes(' ')
    ? `add '${token.split(' ')[1]}': '<tag>' to TOOL_SUBCOMMAND_TAGS['${token.split(' ')[0]}'] in eventTags.ts`
    : `add '${token}': '<tag>' to BASH_FIRST_TOKEN_TAGS in eventTags.ts`;
}

export function collectUntagged(events: ObservedEventDto[]): UntaggedRow[] {
  const byToken = new Map<string, { count: number; sample: string; eventId: string }>();
  for (const e of events) {
    if (tagForEvent(e).disposition !== 'unmatched') continue;
    const input = ((e.payload as Record<string, unknown>)?.input ?? {}) as Record<string, unknown>;
    const isCmd = typeof input.command === 'string';
    const cmd = isCmd
      ? stripCommentLines(input.command as string)
      : typeof input.file_path === 'string' ? input.file_path : '';
    // Aggregate under the first MEANINGFUL command token (or `tool sub` for a
    // multiplexer), with comments / control prefixes / `VAR=` assignments
    // stripped — so the hint points at the real command worth tagging.
    const meaningful = isCmd ? commandOf(firstMeaningfulSegment(segmentCommand(cmd)) ?? cmd) : cmd;
    const tok = isCmd ? untaggedToken(meaningful) : meaningful;
    const cur = byToken.get(tok);
    if (cur) cur.count++;
    else byToken.set(tok, { count: 1, sample: (meaningful || cmd).slice(0, 80), eventId: e.event_id });
  }
  return [...byToken.entries()]
    .map(([token, v]) => ({
      token,
      count: v.count,
      sample: v.sample,
      eventId: v.eventId,
      hint: untaggedHint(token),
    }))
    .sort((a, b) => b.count - a.count);
}
