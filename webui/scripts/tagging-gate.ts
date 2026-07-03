#!/usr/bin/env node
/**
 * tagging-gate (B-7d, 2026-07-04) — PR-전 게이트: "보편 후보 잔존 시 차단".
 *
 * untagged-bash·unidentified-plugins와 같은 read-only Pull API 수집을 돌린 뒤
 * 순수 판정(`src/lib/taggingGate.ts`, vitest 잠금)으로 pass/fail을 낸다:
 *   - untagged 토큰 count>=2 이고 baseline(보류 목록)에 없으면 FAIL
 *   - official/public plugin의 unmatched MCP 도구가 있으면 FAIL
 *     (기결정 intentionally-unmatched 목록은 제외 — B-7c)
 *
 * 보류는 `scripts/tagging-gate-baseline.json`에 토큰→사유로 커밋한다 —
 * 보류가 PR 리뷰에 보이는 편집이 되도록. "이 PR과 무관"은 보류 사유가
 * 아니다(CLAUDE.md 개선 루프).
 *
 * Usage (from webui/):  node scripts/tagging-gate.ts [--base URL]
 * Exit 0 = pass, 1 = fail (실패 목록은 stdout JSON).
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import type { ObservedEventDto, PluginDto } from '../src/api/types.ts';
import { collectUntagged } from '../src/components/replay/stream/eventTags.ts';
import { gateVerdict, type GateInputs } from '../src/lib/taggingGate.ts';

interface EventsPage {
  events: ObservedEventDto[];
  prev_cursor: string | null;
  next_cursor: string | null;
}

function arg(name: string): string | undefined {
  const i = process.argv.indexOf(name);
  return i >= 0 ? process.argv[i + 1] : undefined;
}
const BASE = arg('--base') ?? process.env.WIMCC_BASE ?? 'http://127.0.0.1:7878';
const PAGE = 500;

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

function serverIndex(plugins: PluginDto[]): Map<string, PluginDto> {
  const m = new Map<string, PluginDto>();
  for (const p of plugins) for (const s of p.mcp_servers) m.set(s, p);
  return m;
}

async function main() {
  const baselinePath = join(dirname(fileURLToPath(import.meta.url)), 'tagging-gate-baseline.json');
  const baselineDoc = JSON.parse(readFileSync(baselinePath, 'utf8')) as {
    tokens: Record<string, string>;
  };
  const baseline = new Set(Object.keys(baselineDoc.tokens));

  const sessions = await pull<{ session_id: string }[]>('/v1/sessions');
  const index = serverIndex(await pull<PluginDto[]>('/v1/plugins'));

  const untaggedByToken = new Map<string, number>();
  const mcpByToken = new Map<string, string>();
  for (const s of sessions) {
    const events = await fetchAllEvents(s.session_id);
    for (const row of collectUntagged(events)) {
      untaggedByToken.set(row.token, (untaggedByToken.get(row.token) ?? 0) + row.count);
    }
    for (const e of events) {
      if (e.kind !== 'tool_call' || !e.tool_name?.startsWith('mcp__')) continue;
      const t = e.tag;
      if (!t || t.disposition !== 'unmatched' || !t.token) continue;
      const server = t.token.slice(0, t.token.indexOf(':'));
      const provenance = index.get(server)?.provenance ?? 'configured';
      mcpByToken.set(t.token, provenance);
    }
  }

  const inputs: GateInputs = {
    untagged: [...untaggedByToken].map(([token, count]) => ({ token, count })),
    unidentified: [...mcpByToken].map(([token, provenance]) => ({ token, provenance })),
  };
  const verdict = gateVerdict(inputs, baseline);
  console.log(JSON.stringify(verdict, null, 2));
  process.exit(verdict.pass ? 0 : 1);
}

main().catch((e) => {
  console.error(String(e));
  process.exit(2);
});
