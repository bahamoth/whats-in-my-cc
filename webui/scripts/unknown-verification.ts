#!/usr/bin/env node
/**
 * unknown-verification — emit the still-unknown verification runs as JSON for an
 * LLM/human to act on. The sibling of `untagged-bash`: a read-only loop that
 * closes the verification-outcome gap WITHOUT a backend change.
 *
 * It reuses the pure SSOT `collectUnknownVerification` (src/lib/unknownVerification.ts)
 * — the SAME classification a UI could show — applied to data pulled from the
 * read-only Pull API. The authoritative pass/fail resolution stays in Rust
 * (src/ingest/verification_run.rs); this script only SURFACES candidates + a
 * coarse `hint` about which Rust heuristic could close each gap.
 *
 * Loop: run this → read the hints → extend looks_like_success/looks_like_failure
 * in verification_run.rs (with a locking Rust test) → re-ingest → re-run until the
 * piped-unknown list shrinks. See docs/implementation-notes.html#unknown-verification-loop.
 *
 * Usage (from webui/) — emits CLEAN JSON on stdout (no npm banner):
 *   node scripts/unknown-verification.ts <sessionId>    # one session
 *   node scripts/unknown-verification.ts --all          # aggregate across all sessions
 *   node scripts/unknown-verification.ts <sessionId> --base http://127.0.0.1:7878
 * Runs on Node 22+ (native TS type-stripping); no vite-node/npm wrapper needed.
 *
 * Output: JSON array sorted by count desc:
 *   [{ commandKind, statusBasis, count, sampleCommand, sampleContentTail, hint }]
 */
import {
  collectUnknownVerification,
  type UnknownVerificationRow,
} from '../src/lib/unknownVerification.ts';
import type { VerificationRunDto } from '../src/api/types.ts';

function arg(name: string): string | undefined {
  const i = process.argv.indexOf(name);
  return i >= 0 ? process.argv[i + 1] : undefined;
}
const hasFlag = (name: string) => process.argv.includes(name);

const BASE = arg('--base') ?? process.env.WIMCC_BASE ?? 'http://127.0.0.1:7878';
const TAIL_BYTES = 600;

async function pull<T>(path: string): Promise<T> {
  const res = await fetch(BASE + path);
  if (!res.ok) throw new Error(`GET ${path} → ${res.status}`);
  const body = (await res.json()) as { data: T };
  return body.data;
}

async function listSessionIds(): Promise<string[]> {
  const sessions = await pull<{ session_id: string }[]>('/v1/sessions');
  return sessions.map((s) => s.session_id);
}

/** tool_result content tail for an event (the only tail we need is per group rep). */
async function fetchContentTail(eventId: string): Promise<string> {
  try {
    const data = await pull<{ record?: unknown }>(
      `/v1/events/${encodeURIComponent(eventId)}/raw`,
    );
    const record = data.record as
      | { message?: { content?: Array<{ type?: string; content?: unknown }> } }
      | undefined;
    const blocks = record?.message?.content ?? [];
    const tr = blocks.find((b) => b?.type === 'tool_result');
    const content =
      typeof tr?.content === 'string' ? tr.content : tr?.content ? JSON.stringify(tr.content) : '';
    return content.slice(-TAIL_BYTES);
  } catch {
    return '';
  }
}

async function main() {
  const positional = process.argv.slice(2).find((a) => !a.startsWith('--'));
  const all = hasFlag('--all');
  if (!positional && !all) {
    console.error('usage: node scripts/unknown-verification.ts <sessionId> | --all [--base URL]');
    process.exit(2);
  }

  const sessionIds = all ? await listSessionIds() : [positional as string];

  // Accumulate ALL unknown runs across sessions + a content tail for each group's
  // representative event, then classify once (groups by command_kind+status_basis
  // across sessions). trigger_event_id is unique across sessions.
  const allRuns: VerificationRunDto[] = [];
  const contentTailByEventId: Record<string, string> = {};
  const fetchedKeys = new Set<string>();

  for (const sid of sessionIds) {
    const runs = await pull<VerificationRunDto[]>(
      `/v1/sessions/${encodeURIComponent(sid)}/verification-runs`,
    );
    for (const r of runs) {
      if (r.status !== 'unknown') continue;
      allRuns.push(r);
      // Only the first run of each (kind, basis) group becomes the sample, so we
      // fetch its content tail and skip the rest (keeps the script to a few GETs).
      const key = `${r.command_kind} ${r.status_basis}`;
      if (!fetchedKeys.has(key)) {
        fetchedKeys.add(key);
        contentTailByEventId[r.trigger_event_id] = await fetchContentTail(r.trigger_event_id);
      }
    }
  }

  const rows: UnknownVerificationRow[] = collectUnknownVerification({
    runs: allRuns,
    contentTailByEventId,
  });
  console.log(JSON.stringify(rows, null, 2));
}

main().catch((e) => {
  console.error(String(e));
  process.exit(1);
});
