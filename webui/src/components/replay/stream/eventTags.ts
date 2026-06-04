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
export const CONTROL_TOKENS = new Set(['cd', 'echo', 'sleep', 'for', 'export', 'source', 'set', 'pgrep', 'kill', 'wait', 'true', ':']);
// Command SEQUENCERS — split a compound into independent commands. We split on
// these only (NOT redirects `>`/`<` or subshells `$(`/backtick, which live
// inside a single command), then classify by the first meaningful command.
const COMMAND_SEPARATORS = /(?:&&|\|\||;|&|\|)/;

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

/** Split a compound shell command into its sequenced sub-commands. */
export function segmentCommand(cmd: string): string[] {
  return cmd.split(COMMAND_SEPARATORS).map((s) => s.trim()).filter(Boolean);
}

/** The first segment whose first token is NOT a control token (the "work"), or
 *  null when every segment is control (e.g. `cd x && echo y`). */
function firstMeaningfulSegment(segments: string[]): string | null {
  return segments.find((s) => !CONTROL_TOKENS.has(firstToken(s))) ?? null;
}

/** The command from its first meaningful sub-command onward, for DISPLAY —
 *  leading control prefixes like `cd /path &&` are dropped so the shown command
 *  leads with the actual work. Falls back to the trimmed original when every
 *  sub-command is control (e.g. a bare `cd /tmp`). */
export function meaningfulCommand(cmd: string): string {
  let s = cmd.trim();
  for (let guard = 0; guard < 6; guard++) {
    const m = s.match(/^(.*?)(?:&&|\|\||;|&|\|)(.*)$/s); // split at the FIRST separator
    if (!m) break;
    const head = m[1].trim();
    if (!head || !CONTROL_TOKENS.has(firstToken(head))) break;
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
    const cmd = typeof input.command === 'string' ? input.command.trim() : '';
    if (!cmd) return { tag: null, disposition: 'control' };
    // Classify a compound (`cd … && git add … && git status`) by its first
    // MEANINGFUL sub-command: split on separators, skip control prefixes (cd),
    // and tag by the first real command (here: `git add` → vcs-write). A
    // meaningful-but-unknown first command is `unmatched` (panel candidate);
    // all-control is `control`.
    for (const seg of segmentCommand(cmd)) {
      const tok = firstToken(seg);
      if (!tok) continue;
      if (DESTRUCTIVE_FIRST_TOKENS.has(tok)) return { tag: 'destructive', disposition: 'tagged' };
      if (tok === 'git') {
        const sub = firstToken(seg.slice(3).trim());
        const t = GIT_SUBCOMMAND_TAGS[sub];
        return t ? { tag: t, disposition: 'tagged' } : { tag: null, disposition: 'unmatched' };
      }
      const t = BASH_FIRST_TOKEN_TAGS[tok];
      if (t) return { tag: t, disposition: 'tagged' };
      if (CONTROL_TOKENS.has(tok)) continue; // skip control, look at the next segment
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
    const cmd = isCmd ? (input.command as string).trim() : (typeof input.file_path === 'string' ? input.file_path : '');
    // Aggregate a compound under its first MEANINGFUL token (e.g. `gh`), not a
    // `cd` prefix — so the panel hint points at the command worth tagging.
    const tok = firstToken(isCmd ? (firstMeaningfulSegment(segmentCommand(cmd)) ?? cmd) : cmd);
    const cur = byToken.get(tok);
    if (cur) cur.count++;
    else byToken.set(tok, { count: 1, sample: cmd.slice(0, 80), eventId: e.event_id });
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
