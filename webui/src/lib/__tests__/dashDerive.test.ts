/**
 * 대시보드 파생 SSOT — 일별 버킷·헤드라인·delta·관측된 변화·코호트 비교·레인
 * 배치를 잠근다. 모든 함수는 순수: 같은 rows → 같은 결과.
 */
import { describe, expect, it } from 'vitest';
import type { SessionSeriesRowDto } from '../../api/types';
import {
  buildDaily,
  cohortCompare,
  headline,
  headlineDelta,
  laneLayout,
  observedChanges,
} from '../dashDerive';
import { displayModel } from '../seriesView';

function row(
  id: string,
  date: string, // 'MM-DD'
  models: string[],
  over: Partial<{
    passed: number; failed: number; unknown: number; cost: number;
    input: number; creation: number; read: number; output: number;
    toolCalls: number; toolFails: number; events: number; cc: string[];
  }> = {},
): SessionSeriesRowDto {
  const o = {
    passed: 10, failed: 2, unknown: 3, cost: 50,
    input: 100_000, creation: 400_000, read: 9_500_000, output: 500_000,
    toolCalls: 100, toolFails: 2, events: 1000, cc: ['2.1.200'],
    ...over,
  };
  return {
    session_id: id,
    first_observed_at: `2026-${date}T04:00:00+00:00`,
    last_observed_at: `2026-${date}T08:00:00+00:00`,
    event_count: o.events,
    metrics: {
      session_id: id,
      tool_call_total: o.toolCalls,
      tool_failure_count: o.toolFails,
      verification_total: o.passed + o.failed + o.unknown,
      verification_passed: o.passed,
      verification_failed: o.failed,
      verification_unknown: o.unknown,
      verification_not_executed: 0,
      context_bloat_count: 1,
      tool_user_rejected: 0,
      tool_policy_denied: 0,
      tool_cancelled: 0,
      tool_backgrounded: 0,
      turn_duration_ms_total: 0,
      turn_duration_count: 0,
      api_error_count: 0,
      api_rate_limit_count: 0,
      input_tokens: o.input,
      output_tokens: o.output,
      cache_read_input_tokens: o.read,
      cache_creation_input_tokens: o.creation,
      estimated_cost_usd: o.cost,
      compact_boundary_count: 0,
      tool_result_truncated_count: 0,
      user_interruption_count: 1,
      detector_firing: {},
    },
    fingerprint: {
      session_id: id,
      models,
      cc_versions: o.cc,
      git_branches: [],
      cwds: [],
      entrypoints: [],
    },
  };
}

describe('displayModel', () => {
  it('전체 표시명으로 변환한다', () => {
    expect(displayModel('claude-fable-5')).toBe('Fable 5');
    expect(displayModel('claude-opus-4-8')).toBe('Opus 4.8');
    expect(displayModel('haiku-4-5-20251001')).toBe('Haiku 4.5');
    expect(displayModel('claude-sonnet-4-6')).toBe('Sonnet 4.6');
  });
  it('미인식 이름은 claude- 접두사만 벗겨 원문 유지', () => {
    expect(displayModel('claude-something_odd')).toBe('something_odd');
    expect(displayModel('<synthetic>')).toBe('<synthetic>');
  });
});

describe('buildDaily', () => {
  it('첫 날부터 마지막 날까지 연속 일자 버킷 + 같은 날 합산 + sessionsOf 역참조', () => {
    const rows = [
      row('a', '06-05', ['claude-opus-4-8'], { cost: 100 }),
      row('b', '06-05', ['claude-opus-4-8'], { cost: 30 }),
      row('c', '06-08', ['claude-opus-4-8'], { cost: 7 }),
    ];
    const d = buildDaily(rows);
    expect(d.dates).toEqual(['06-05', '06-06', '06-07', '06-08']);
    expect(d.cost).toEqual([130, 0, 0, 7]);
    expect(d.passed).toEqual([20, 0, 0, 10]);
    expect(d.sessionsOf[0]).toEqual([0, 1]);
    expect(d.sessionsOf[3]).toEqual([2]);
    // 신호 = 6종 합: toolFails 2 + bloat 1 + interruption 1 = 4 (세션당)
    expect(d.signals).toEqual([8, 0, 0, 4]);
  });
});

describe('headline / headlineDelta', () => {
  it('측정 기반 요약 — passRate = passed/(passed+failed), 단가 = cost/billed×1M', () => {
    const h = headline([row('a', '06-05', [], { cost: 50 })]);
    expect(h.sessions).toBe(1);
    expect(h.passRatePct).toBeCloseTo(83.3, 1);
    // billed = 0.1M+0.4M+0.5M = 1M → $50/1M
    expect(h.unitRatePerM).toBeCloseTo(50, 5);
    expect(h.cacheHitPct).toBeCloseTo(95, 1);
    expect(h.toolFailPct).toBeCloseTo(2, 5);
  });
  it('측정이 없으면 null — 0으로 위장하지 않는다', () => {
    const h = headline([
      row('a', '06-05', [], {
        passed: 0, failed: 0, unknown: 0, cost: 0,
        input: 0, creation: 0, read: 0, output: 0, toolCalls: 0, toolFails: 0,
      }),
    ]);
    expect(h.passRatePct).toBeNull();
    expect(h.unitRatePerM).toBeNull();
    expect(h.cacheHitPct).toBeNull();
    expect(h.toolFailPct).toBeNull();
  });
  it('prev 없으면 delta 전부 null', () => {
    const d = headlineDelta(headline([row('a', '06-05', [])]), null);
    expect(d.passRatePp).toBeNull();
    expect(d.cost).toBeNull();
  });
  it('delta = cur − prev', () => {
    const cur = headline([row('a', '06-06', [], { cost: 80 })]);
    const prev = headline([row('p', '06-01', [], { cost: 50 })]);
    expect(headlineDelta(cur, prev).cost).toBeCloseTo(30, 5);
  });
});

describe('observedChanges', () => {
  it('창 내 첫 관측 모델·CC 전환·신호 최다 세션을 결정론 문구로', () => {
    const rows = [
      row('a', '06-05', ['claude-opus-4-8'], { cc: ['2.1.198'] }),
      row('b', '06-12', ['claude-opus-4-8', 'claude-fable-5'], { cc: ['2.1.198'] }),
      row('c', '07-02', ['claude-fable-5'], { cc: ['2.1.200'], toolFails: 30 }),
    ];
    const out = observedChanges(rows);
    expect(out).toContainEqual({ kind: 'model_first', date: '06-12', model: 'Fable 5' });
    expect(out).toContainEqual({ kind: 'cc_change', date: '07-02', from: '2.1.198', to: '2.1.200' });
    expect(out).toContainEqual({ kind: 'top_signals', sessionId: 'c', n: 32 });
  });
});

describe('cohortCompare', () => {
  const rows = [
    row('a', '06-05', ['claude-opus-4-8'], { cost: 10 }),
    row('b', '06-06', ['claude-opus-4-8'], { cost: 10 }),
    row('c', '06-07', ['claude-opus-4-8'], { cost: 10 }),
    row('d', '06-12', ['claude-opus-4-8', 'claude-fable-5'], { cost: 90 }),
    row('e', '06-13', ['claude-opus-4-8', 'claude-fable-5'], { cost: 90 }),
    row('f', '06-14', ['claude-opus-4-8', 'claude-fable-5'], { cost: 90 }),
  ];
  it('최신 경계의 인접 세그먼트를 전/후로 집계, 라벨은 diff에서 파생', () => {
    const c = cohortCompare(rows)!;
    expect(c.label).toBe('Fable 5 유입');
    expect(c.boundaryIdx).toBe(3);
    expect(c.before.n).toBe(3);
    expect(c.after.n).toBe(3);
    expect(c.after.unitRatePerM! > c.before.unitRatePerM!).toBe(true);
    expect(c.lowSample).toBe(false);
    expect(c.alsoCcChanged).toBe(false);
  });
  it('경계가 없으면 null', () => {
    expect(cohortCompare(rows.slice(0, 3))).toBeNull();
  });
  it('한쪽 표본 n<3 → lowSample, 동시 CC 변경 → alsoCcChanged', () => {
    const r2 = [
      row('a', '06-05', ['claude-opus-4-8'], { cc: ['2.1.198'] }),
      row('b', '06-06', ['claude-fable-5'], { cc: ['2.1.200'] }),
    ];
    const c = cohortCompare(r2)!;
    expect(c.lowSample).toBe(true);
    expect(c.alsoCcChanged).toBe(true);
    expect(c.label).toBe('Opus 4.8 → Fable 5');
  });
});

describe('laneLayout', () => {
  it('겹치면 다음 레인, 자리가 나면 재사용', () => {
    expect(laneLayout([{ x: 0 }, { x: 5 }, { x: 40 }], 20)).toEqual([0, 1, 0]);
  });
});
