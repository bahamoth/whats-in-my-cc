import { describe, it, expect } from 'vitest';
import { groupSpanMs, workflowStats, workflowTimeline, agentDurationHeat } from '../workflowStats';
import { translate } from '../../../../i18n/t';
import { en } from '../../../../i18n/catalog/en';
import { ko } from '../../../../i18n/catalog/ko';
import type { TFunction } from '../../../../i18n';
import type { SidechainGroup } from '../streamModel';

// Assert against the Korean (source) fallback label by binding t to ko.
const koT: TFunction = (key, arg) => translate(ko, en, key, arg);

const g = (id: string, startIso: string, endIso: string, concl: string | null): SidechainGroup => ({
  type: 'sidechain-group', id, agentId: id, agentType: 'Explore', description: null, taskEventId: null, conclusion: concl,
  items: [
    { type: 'message', id: id + 'u', eventId: id + 'u', role: 'user', model: null, text: 'p', timestamp: startIso, sidechain: true },
    { type: 'message', id: id + 'a', eventId: id + 'a', role: 'assistant', model: null, text: concl ?? '', timestamp: endIso, sidechain: true },
  ],
});

describe('workflow helpers', () => {
  const groups = [
    g('A', '2026-06-14T00:00:00Z', '2026-06-14T00:38:00Z', 'a'), // 38m
    g('B', '2026-06-14T00:00:00Z', '2026-06-14T00:24:00Z', 'b'), // 24m, overlaps A
    g('C', '2026-06-14T00:00:00Z', '2026-06-14T00:02:00Z', 'c'), // 2m
  ];
  it('groupSpanMs = 자식 min~max', () => { expect(groupSpanMs(groups[0])).toBe(38 * 60000); });
  it('maxConcurrency / longest / median', () => {
    const s = workflowStats(groups);
    expect(s.agentCount).toBe(3);
    expect(s.maxConcurrency).toBe(3);   // 0~2m 구간 셋 다 실행
    expect(s.longestMs).toBe(38 * 60000);
    expect(s.medianMs).toBe(24 * 60000);
    expect(s.incomplete).toBe(0);
  });
  it('timeline lanes: 상대 시작·소요', () => {
    const t = workflowTimeline(groups, koT);
    expect(t.spanMs).toBe(38 * 60000);
    expect(t.lanes[0]).toMatchObject({ startMs: 0, durMs: 38 * 60000 });
  });
  it('레인 라벨은 언어중립 구조 라벨(영어 프롬프트 원문 금지)', () => {
    // 워크플로우 에이전트 프롬프트는 영어 템플릿 prefix를 공유 → 판단 불가.
    // agentType 있으면 `타입 N`, 없으면 `에이전트 N`.
    const t = workflowTimeline(groups, koT);
    expect(t.lanes.map((l) => l.label)).toEqual(['Explore 1', 'Explore 2', 'Explore 3']);
    const noType = workflowTimeline([{ ...groups[0], agentType: null }], koT);
    expect(noType.lanes[0].label).toBe('에이전트 1');
  });
  it('agentDurationHeat: ≥5m warn, ≥20m hot', () => {
    expect(agentDurationHeat(60000)).toBe('');
    expect(agentDurationHeat(6 * 60000)).toBe('warn');
    expect(agentDurationHeat(25 * 60000)).toBe('hot');
  });
});
