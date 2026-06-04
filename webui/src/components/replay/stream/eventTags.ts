// webui/src/components/replay/stream/eventTags.ts
import type { ObservedEventDto } from '../../../api/types';

export type ReadTag = 'code' | 'docs' | 'config' | 'data';
export type BashTag = 'search·read' | 'vcs-read' | 'vcs-write' | 'build·test' | 'query·script' | 'destructive';
export type Tag = ReadTag | BashTag;
export type Disposition = 'tagged' | 'control' | 'ambiguous' | 'unmatched';
export interface TagResult { tag: Tag | null; disposition: Disposition; }

// ── single source of truth — add a key to extend ──────────────────────────
export const READ_EXT_TAGS: Record<string, ReadTag> = {
  rs: 'code', ts: 'code', tsx: 'code', js: 'code', jsx: 'code', css: 'code',
  md: 'docs', html: 'docs', txt: 'docs',
  toml: 'config', yaml: 'config', yml: 'config', ini: 'config',
  json: 'data', sql: 'data', jsonl: 'data', log: 'data', csv: 'data',
};
export const BASH_FIRST_TOKEN_TAGS: Record<string, BashTag> = {
  grep: 'search·read', rg: 'search·read', egrep: 'search·read', fgrep: 'search·read',
  find: 'search·read', ls: 'search·read', cat: 'search·read', head: 'search·read',
  tail: 'search·read', wc: 'search·read', jq: 'search·read', tree: 'search·read',
  which: 'search·read', file: 'search·read', stat: 'search·read', du: 'search·read', df: 'search·read',
  cargo: 'build·test', npm: 'build·test', npx: 'build·test', pnpm: 'build·test',
  yarn: 'build·test', make: 'build·test', tsc: 'build·test', vitest: 'build·test', go: 'build·test',
  sqlite3: 'query·script', python3: 'query·script', python: 'query·script',
  node: 'query·script', osascript: 'query·script', psql: 'query·script', ruby: 'query·script',
};
export const GIT_SUBCOMMAND_TAGS: Record<string, BashTag> = {
  status: 'vcs-read', log: 'vcs-read', diff: 'vcs-read', show: 'vcs-read',
  branch: 'vcs-read', blame: 'vcs-read', 'rev-parse': 'vcs-read', describe: 'vcs-read',
  add: 'vcs-write', commit: 'vcs-write', push: 'vcs-write', checkout: 'vcs-write',
  switch: 'vcs-write', stash: 'vcs-write', rm: 'vcs-write', reset: 'vcs-write',
  merge: 'vcs-write', rebase: 'vcs-write', fetch: 'vcs-write', pull: 'vcs-write', tag: 'vcs-write', clone: 'vcs-write',
};
export const DESTRUCTIVE_FIRST_TOKENS = new Set(['rm', 'mv', 'rmdir']);
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
    // segments (cd) and assignment/keyword prefixes (`VAR=x`, `do`), tag by the
    // first real command. Meaningful-but-unknown → `unmatched`; all-control →
    // `control`.
    for (const seg of segmentCommand(cmd)) {
      const cmdStr = commandOf(seg);
      const tok = firstToken(cmdStr);
      if (!tok || CONTROL_TOKENS.has(tok)) continue; // empty/assignment-only or control → next segment
      if (DESTRUCTIVE_FIRST_TOKENS.has(tok)) return { tag: 'destructive', disposition: 'tagged' };
      if (tok === 'git') {
        const sub = firstToken(cmdStr.slice(3).trim());
        const t = GIT_SUBCOMMAND_TAGS[sub];
        return t ? { tag: t, disposition: 'tagged' } : { tag: null, disposition: 'unmatched' };
      }
      const t = BASH_FIRST_TOKEN_TAGS[tok];
      if (t) return { tag: t, disposition: 'tagged' };
      return { tag: null, disposition: 'unmatched' }; // first meaningful but unknown
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

export function collectUntagged(events: ObservedEventDto[]): UntaggedRow[] {
  const byToken = new Map<string, { count: number; sample: string; eventId: string }>();
  for (const e of events) {
    if (tagForEvent(e).disposition !== 'unmatched') continue;
    const input = ((e.payload as Record<string, unknown>)?.input ?? {}) as Record<string, unknown>;
    const isCmd = typeof input.command === 'string';
    const cmd = isCmd
      ? stripCommentLines(input.command as string)
      : typeof input.file_path === 'string' ? input.file_path : '';
    // Aggregate a compound under its first MEANINGFUL command token (e.g. `gh`),
    // with comment lines / control prefixes / `VAR=` assignments stripped — so
    // the panel hint points at the real command worth tagging, not noise.
    const meaningful = isCmd ? commandOf(firstMeaningfulSegment(segmentCommand(cmd)) ?? cmd) : cmd;
    const tok = firstToken(meaningful);
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
      hint: `add '${token}': '<tag>' to BASH_FIRST_TOKEN_TAGS in eventTags.ts`,
    }))
    .sort((a, b) => b.count - a.count);
}
