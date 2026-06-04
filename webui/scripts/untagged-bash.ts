/**
 * untagged-bash — emit the untagged-Bash analysis as JSON for an LLM to consume.
 *
 * Closes the tagging loop WITHOUT a backend: it reuses the frontend's single
 * source of truth (`collectUntagged` in src/components/replay/stream/eventTags.ts)
 * — the SAME classification the WebUI panel shows — applied to events pulled from
 * the read-only Pull API. An agent runs this each turn, reads the untagged
 * tokens, adds rules to BASH_FIRST_TOKEN_TAGS in eventTags.ts, and re-runs until
 * the list shrinks. No rule logic is duplicated.
 *
 * Usage (from webui/):
 *   npm run untagged -- <sessionId>          # one session
 *   npm run untagged -- --all                # aggregate across all sessions
 *   npm run untagged -- <sessionId> --base http://127.0.0.1:7878
 *
 * Output: JSON array sorted by count desc:
 *   [{ token, count, sample, eventId, sessionId, hint }]
 */
import { collectUntagged, type UntaggedRow } from '../src/components/replay/stream/eventTags';
import type { ObservedEventDto } from '../src/api/types';

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

const BASE = arg('--base') ?? process.env.WITMCC_BASE ?? 'http://127.0.0.1:7878';
const PAGE = Number(arg('--limit') ?? 500);

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
