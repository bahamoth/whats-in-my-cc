import type { SidechainGroup } from './streamModel';
import { agentColor } from '../../../lib/colorHash';

/** 한 에이전트 그룹의 [최초, 최후] 관측 타임스탬프 폭(ms). */
export function groupSpanMs(group: SidechainGroup): number {
  let min = Infinity, max = -Infinity;
  const see = (iso: string) => { const t = new Date(iso).getTime(); if (!Number.isNaN(t)) { min = Math.min(min, t); max = Math.max(max, t); } };
  for (const it of group.items) {
    if (it.type === 'message') see(it.timestamp);
    else if (it.type === 'activity-run') for (const ae of it.events) see(ae.event.observed_at);
    else if (it.type === 'thinking') for (const e of it.events) see(e.timestamp);
  }
  return max > min ? max - min : 0;
}

function bounds(group: SidechainGroup): { start: number; end: number } {
  let min = Infinity, max = -Infinity;
  const see = (iso: string) => { const t = new Date(iso).getTime(); if (!Number.isNaN(t)) { min = Math.min(min, t); max = Math.max(max, t); } };
  for (const it of group.items) {
    if (it.type === 'message') see(it.timestamp);
    else if (it.type === 'activity-run') for (const ae of it.events) see(ae.event.observed_at);
    else if (it.type === 'thinking') for (const e of it.events) see(e.timestamp);
  }
  return { start: min === Infinity ? 0 : min, end: max === -Infinity ? 0 : max };
}

export interface WorkflowStat { agentCount: number; maxConcurrency: number; longestMs: number; medianMs: number; incomplete: number; }

export function workflowStats(groups: SidechainGroup[]): WorkflowStat {
  const durs = groups.map(groupSpanMs).sort((a, b) => a - b);
  const bnds = groups.map(bounds);
  // 동시성: 시작 +1 / 종료 -1 스윕
  const pts: [number, number][] = [];
  for (const b of bnds) { pts.push([b.start, 1]); pts.push([b.end, -1]); }
  pts.sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  let cur = 0, max = 0;
  for (const [, d] of pts) { cur += d; if (cur > max) max = cur; }
  const median = durs.length ? durs[Math.floor((durs.length - 1) / 2)] : 0;
  return {
    agentCount: groups.length,
    maxConcurrency: max,
    longestMs: durs.length ? durs[durs.length - 1] : 0,
    medianMs: median,
    incomplete: groups.filter((g) => g.conclusion == null).length,
  };
}

export interface GanttLane { id: string; label: string; startMs: number; durMs: number; color: string; }
export interface WorkflowTimeline { spanMs: number; lanes: GanttLane[]; }

/** 첫 시작을 0으로 한 상대 타임라인. 라벨 = description ?? prompt 첫 줄 ?? agentType ?? id. */
export function workflowTimeline(groups: SidechainGroup[]): WorkflowTimeline {
  const bnds = groups.map((g) => ({ g, ...bounds(g) }));
  const origin = Math.min(...bnds.map((b) => b.start).filter((n) => n > 0), Infinity);
  const base = Number.isFinite(origin) ? origin : 0;
  let spanMs = 0;
  const lanes = bnds.map(({ g, start, end }, i) => {
    const startMs = Math.max(0, start - base);
    const durMs = Math.max(0, end - start);
    spanMs = Math.max(spanMs, startMs + durMs);
    // 언어중립 구조 라벨: Workflow 에이전트 프롬프트는 영어 템플릿 prefix를 공유해
    // 판단 라벨로 쓸 수 없다. agentType(예: Explore)이 있으면 `타입 N`, 없으면
    // `에이전트 N`. 영어 원문(프롬프트·결론)은 펼침 상세에만 둔다(원본 보존).
    const label = g.agentType ? `${g.agentType} ${i + 1}` : `에이전트 ${i + 1}`;
    // per-agent color (hash) so the mini-gantt matches the stream block + gutter
    // rail for the same agent (was a uniform violet).
    return { id: g.id, label, startMs, durMs, color: agentColor(g.agentId) };
  });
  return { spanMs, lanes };
}

/** Workflow 에이전트(분 단위) 소요 heat — tool-exec용 durationHeat(10s/60s)와 별개. */
export function agentDurationHeat(ms: number): '' | 'warn' | 'hot' {
  if (ms >= 20 * 60000) return 'hot';
  if (ms >= 5 * 60000) return 'warn';
  return '';
}
