#!/usr/bin/env node
/**
 * untagged-bash — emit the untagged-Bash analysis as JSON for an LLM to consume.
 *
 * 분류는 서버(core `src/insight/event_tags.rs`)가 한다 — events 응답의 `tag`
 * 필드(loop-foundations 2026-06-12). 이 스크립트는 프런트 `collectUntagged`
 * (서버 tag 기반 집계 — WebUI 패널과 동일 SSOT)를 Pull API 이벤트에 적용해
 * untagged token을 JSON으로 내보낸다. 에이전트는 hint가 가리키는 Rust 사전
 * (BASH_FIRST_TOKEN_TAGS · TOOL_SUBCOMMAND_TAGS · EXT_OBJECT)에 규칙을 추가하고
 * 서버 재빌드·재기동 후 재실행해 목록이 줄었는지 확인한다.
 *
 * Usage (from webui/) — emits CLEAN JSON on stdout (no npm banner):
 *   node scripts/untagged-bash.ts <sessionId>    # one session
 *   node scripts/untagged-bash.ts --all          # aggregate across all sessions
 *   node scripts/untagged-bash.ts <sessionId> --base http://127.0.0.1:7878
 * Runs on Node 22+ (native TS type-stripping); no vite-node/npm wrapper needed.
 * `npm run untagged -- <sessionId>` works too but `npm run` prints a banner to
 * stdout — use `npm run -s untagged -- ...` (silent) if piping through npm.
 *
 * Output: JSON array sorted by count desc:
 *   [{ token, count, sample, eventId, sessionId, hint }]
 */
// Explicit .ts extensions so Node (v22+, native type-stripping) resolves these
// when run directly — `node scripts/untagged-bash.ts <sessionId>` — with NO npm
// wrapper, so stdout is clean JSON (no `npm run` banner). tsconfig allows it
// (allowImportingTsExtensions) and this file is outside the build's `include`.
import { collectUntagged, type UntaggedRow } from '../src/components/replay/stream/eventTags.ts';
import type { ObservedEventDto } from '../src/api/types.ts';

interface EventsPage {
  events: ObservedEventDto[];
  prev_cursor: string | null;
  next_cursor: string | null;
}

function arg(name: string): string | undefined {
  const i = process.argv.indexOf(name);
  return i >= 0 ? process.argv[i + 1] : undefined;
}
const hasFlag = (name: string) => process.argv.includes(name);

const BASE = arg('--base') ?? process.env.WIMCC_BASE ?? 'http://127.0.0.1:7878';
// `?? 500` does NOT catch NaN (a value-less `--limit` makes arg() undefined →
// Number(undefined)=NaN), so validate explicitly to avoid `limit=NaN` in the URL.
const limitArg = Number(arg('--limit'));
const PAGE = Number.isFinite(limitArg) && limitArg > 0 ? limitArg : 500;

async function pull<T>(path: string): Promise<T> {
  const res = await fetch(BASE + path);
  if (!res.ok) throw new Error(`GET ${path} → ${res.status}`);
  const body = (await res.json()) as { data: T };
  return body.data;
}

/** Walk the whole session (newest window, then `before=` older pages until the
 *  session start) so the analysis covers every Bash command, not just the
 *  loaded UI window. */
async function fetchAllEvents(sessionId: string): Promise<ObservedEventDto[]> {
  const all: ObservedEventDto[] = [];
  let path = `/v1/sessions/${encodeURIComponent(sessionId)}/events?limit=${PAGE}`;
  let cursor: string | null | undefined;
  // first page (newest window)
  let page = await pull<EventsPage>(path);
  all.push(...page.events);
  cursor = page.prev_cursor;
  // older pages
  while (cursor) {
    path = `/v1/sessions/${encodeURIComponent(sessionId)}/events?before=${encodeURIComponent(cursor)}&limit=${PAGE}`;
    page = await pull<EventsPage>(path);
    if (page.events.length === 0) break;
    all.push(...page.events);
    cursor = page.prev_cursor;
  }
  return all;
}

interface AggRow extends UntaggedRow {
  sessionId: string;
}

async function listSessionIds(): Promise<string[]> {
  // GET /v1/sessions → data is the session array directly.
  const sessions = await pull<{ session_id: string }[]>('/v1/sessions');
  return sessions.map((s) => s.session_id);
}

async function main() {
  const positional = process.argv.slice(2).find((a) => !a.startsWith('--'));
  const all = hasFlag('--all');
  if (!positional && !all) {
    console.error('usage: npm run untagged -- <sessionId> | --all [--base URL]');
    process.exit(2);
  }

  const sessionIds = all ? await listSessionIds() : [positional as string];

  // Merge per-session UntaggedRows by token (sum counts; keep the first sample +
  // a jump reference: eventId within its sessionId).
  const byToken = new Map<string, AggRow>();
  for (const sid of sessionIds) {
    const events = await fetchAllEvents(sid);
    for (const r of collectUntagged(events)) {
      const cur = byToken.get(r.token);
      if (cur) cur.count += r.count;
      else byToken.set(r.token, { ...r, sessionId: sid });
    }
  }

  const rows = [...byToken.values()].sort((a, b) => b.count - a.count);
  console.log(JSON.stringify(rows, null, 2));
}

main().catch((e) => {
  console.error(String(e));
  process.exit(1);
});
