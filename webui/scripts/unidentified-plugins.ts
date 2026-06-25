#!/usr/bin/env node
/**
 * unidentified-plugins — surface MCP tools that are NOT yet verb.object-tagged,
 * joined with their plugin provenance, as JSON for an LLM/human to act on. The
 * sibling of `untagged-bash` / `unknown-verification`: a read-only loop that
 * closes the MCP-tagging gap WITHOUT a backend change.
 *
 * Classification is the server's (core `src/insight/event_tags.rs`,
 * `MCP_SERVER_TOOL_TAGS`): an MCP tool with `tag.disposition='unmatched'` has no
 * verb.object yet. This script groups those by `server:tool` (the tag token) and
 * resolves each server's provenance via the read-only `/v1/plugins` registry
 * (`claude plugins list --json`, see `src/plugins.rs`), so the agent can decide:
 *
 *   - provenance official|public → add the tool to `MCP_SERVER_TOOL_TAGS`
 *     (researching its verb online if needed), with a locking test in
 *     `tests/event_tags.rs`; rebuild + restart serve; re-run until the list shrinks.
 *   - provenance personal|configured → EXCLUDE from tagging (a directory-source
 *     marketplace plugin or a directly-configured MCP server — not community).
 *     Leave it unmatched; the loop must not chase it.
 *
 * The internet lookup for a community tool's purpose is the AGENT's job during
 * the loop (it has WebFetch) — like untagged-bash, the script only SURFACES
 * candidates + a `hint`. The wimcc service itself never goes online.
 *
 * Usage (from webui/) — emits CLEAN JSON on stdout (no npm banner):
 *   node scripts/unidentified-plugins.ts <sessionId>    # one session
 *   node scripts/unidentified-plugins.ts --all          # aggregate across all sessions
 *   node scripts/unidentified-plugins.ts <sessionId> --base http://127.0.0.1:7878
 * Runs on Node 22+ (native TS type-stripping); no vite-node/npm wrapper needed.
 *
 * Output: JSON array sorted by count desc:
 *   [{ token, server, tool, count, provenance, plugin, sessionId, hint }]
 */
import type { ObservedEventDto, PluginDto } from '../src/api/types.ts';

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
const limitArg = Number(arg('--limit'));
const PAGE = Number.isFinite(limitArg) && limitArg > 0 ? limitArg : 500;

async function pull<T>(path: string): Promise<T> {
  const res = await fetch(BASE + path);
  if (!res.ok) throw new Error(`GET ${path} → ${res.status}`);
  const body = (await res.json()) as { data: T };
  return body.data;
}

async function fetchAllEvents(sessionId: string): Promise<ObservedEventDto[]> {
  const all: ObservedEventDto[] = [];
  let page = await pull<EventsPage>(
    `/v1/sessions/${encodeURIComponent(sessionId)}/events?limit=${PAGE}`,
  );
  all.push(...page.events);
  let cursor = page.prev_cursor;
  while (cursor) {
    page = await pull<EventsPage>(
      `/v1/sessions/${encodeURIComponent(sessionId)}/events?before=${encodeURIComponent(cursor)}&limit=${PAGE}`,
    );
    if (page.events.length === 0) break;
    all.push(...page.events);
    cursor = page.prev_cursor;
  }
  return all;
}

async function listSessionIds(): Promise<string[]> {
  const sessions = await pull<{ session_id: string }[]>('/v1/sessions');
  return sessions.map((s) => s.session_id);
}

/** server name → its owning plugin (provenance/id), from /v1/plugins. */
function serverIndex(plugins: PluginDto[]): Map<string, PluginDto> {
  const m = new Map<string, PluginDto>();
  for (const p of plugins) for (const s of p.mcp_servers) m.set(s, p);
  return m;
}

function hintFor(provenance: string): string {
  switch (provenance) {
    case 'official':
    case 'public':
      return 'add to MCP_SERVER_TOOL_TAGS (community plugin — research verb if needed)';
    case 'personal':
      return 'exclude — personal (directory-source marketplace), not tagged';
    default:
      return 'exclude — directly-configured MCP server (not a marketplace plugin)';
  }
}

interface Row {
  token: string;
  server: string;
  tool: string;
  count: number;
  provenance: string;
  plugin: string | null;
  sessionId: string;
  hint: string;
}

async function main() {
  const positional = process.argv.slice(2).find((a) => !a.startsWith('--'));
  const all = hasFlag('--all');
  if (!positional && !all) {
    console.error('usage: node scripts/unidentified-plugins.ts <sessionId> | --all [--base URL]');
    process.exit(2);
  }

  const index = serverIndex(await pull<PluginDto[]>('/v1/plugins'));
  const sessionIds = all ? await listSessionIds() : [positional as string];

  // Group unmatched MCP tools by `server:tool` (the tag token).
  const byToken = new Map<string, Row>();
  for (const sid of sessionIds) {
    const events = await fetchAllEvents(sid);
    for (const e of events) {
      if (e.kind !== 'tool_call') continue;
      if (!e.tool_name?.startsWith('mcp__')) continue;
      const tag = e.tag;
      if (!tag || tag.disposition !== 'unmatched' || !tag.token) continue;
      const token = tag.token; // "server:tool"
      const cur = byToken.get(token);
      if (cur) {
        cur.count += 1;
        continue;
      }
      const sep = token.indexOf(':');
      const server = sep >= 0 ? token.slice(0, sep) : token;
      const tool = sep >= 0 ? token.slice(sep + 1) : '';
      const owner = index.get(server);
      const provenance = owner?.provenance ?? 'configured';
      byToken.set(token, {
        token,
        server,
        tool,
        count: 1,
        provenance,
        plugin: owner?.id ?? null,
        sessionId: sid,
        hint: hintFor(provenance),
      });
    }
  }

  const rows = [...byToken.values()].sort((a, b) => b.count - a.count);
  console.log(JSON.stringify(rows, null, 2));
}

main().catch((e) => {
  console.error(String(e));
  process.exit(1);
});
