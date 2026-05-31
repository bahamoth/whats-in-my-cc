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
export const BASH_COMPOUND_MARKERS = ['&&', '||', ';', '|', '>', '>>', '<', '$(', '`'];

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
    if (BASH_COMPOUND_MARKERS.some((m) => cmd.includes(m))) return { tag: null, disposition: 'ambiguous' };
    const tok = firstToken(cmd);
    if (DESTRUCTIVE_FIRST_TOKENS.has(tok)) return { tag: 'destructive', disposition: 'tagged' };
    if (tok === 'git') {
      const sub = firstToken(cmd.slice(3).trim());
      const t = GIT_SUBCOMMAND_TAGS[sub];
      return t ? { tag: t, disposition: 'tagged' } : { tag: null, disposition: 'unmatched' };
    }
    const t = BASH_FIRST_TOKEN_TAGS[tok];
    if (t) return { tag: t, disposition: 'tagged' };
    if (CONTROL_TOKENS.has(tok)) return { tag: null, disposition: 'control' };
    return { tag: null, disposition: 'unmatched' };
  }
  // every other tool: tool name is the label → no chip
  return { tag: null, disposition: 'control' };
}

export interface UntaggedRow { token: string; count: number; sample: string; hint: string; }

export function collectUntagged(events: ObservedEventDto[]): UntaggedRow[] {
  const byToken = new Map<string, { count: number; sample: string }>();
  for (const e of events) {
    if (tagForEvent(e).disposition !== 'unmatched') continue;
    const input = ((e.payload as Record<string, unknown>)?.input ?? {}) as Record<string, unknown>;
    const cmd = typeof input.command === 'string' ? input.command.trim() : (typeof input.file_path === 'string' ? input.file_path : '');
    const tok = firstToken(cmd);
    const cur = byToken.get(tok);
    if (cur) cur.count++;
    else byToken.set(tok, { count: 1, sample: cmd.slice(0, 80) });
  }
  return [...byToken.entries()]
    .map(([token, v]) => ({ token, count: v.count, sample: v.sample, hint: `add '${token}': '<tag>' to BASH_FIRST_TOKEN_TAGS in eventTags.ts` }))
    .sort((a, b) => b.count - a.count);
}
