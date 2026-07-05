/** 세션 분포 스캐터 옵션 빌더 — log x(토큰 0 제외)·주 모델별 계열(최초
 *  관측순 색)·중앙값 markLine·이상점만 라벨을 잠근다. */
import { describe, expect, it } from 'vitest';
import { buildScatterOption } from '../scatterOption';
import type { SessionSeriesRowDto } from '../../../api/types';

function row(id: string, models: string[], over: Partial<{
  billedM: number; cost: number; signals: number; events: number;
}> = {}): SessionSeriesRowDto {
  const o = { billedM: 10, cost: 100, signals: 5, events: 1000, ...over };
  return {
    session_id: id,
    first_observed_at: '2026-06-05T00:00:00+00:00',
    last_observed_at: '2026-06-05T01:00:00+00:00',
    event_count: o.events,
    metrics: {
      session_id: id,
      tool_call_total: 10,
      tool_failure_count: o.signals,
      verification_total: 0,
      verification_passed: 0,
      verification_failed: 0,
      verification_unknown: 0,
      verification_not_executed: 0,
      context_bloat_count: 0,
      tool_user_rejected: 0,
      tool_policy_denied: 0,
      tool_cancelled: 0,
      tool_backgrounded: 0,
      turn_duration_ms_total: 0,
      turn_duration_count: 0,
      api_error_count: 0,
      api_rate_limit_count: 0,
      input_tokens: o.billedM * 1_000_000,
      output_tokens: 0,
      cache_read_input_tokens: 0,
      cache_creation_input_tokens: 0,
      estimated_cost_usd: o.cost,
      compact_boundary_count: 0,
      tool_result_truncated_count: 0,
      user_interruption_count: 0,
      detector_firing: {},
      llm_request_p50: {
        ttft_ms: { p50: null, n: 0 },
        duration_ms: { p50: null, n: 0 },
        output_tokens: { p50: null, n: 0 },
        cost_usd: { p50: null, n: 0 },
      },
    },
    fingerprint: {
      session_id: id,
      models,
      cc_versions: [],
      git_branches: [],
      cwds: [],
      entrypoints: [],
    },
  };
}

const labels = { x: 'billed tokens (M)', y: 'signals / 100 events', unassigned: 'not observed', click: 'click → replay' };

describe('buildScatterOption', () => {
  it('과금 토큰 0 세션 제외, 주 모델별 계열, 최초 관측순 색', () => {
    const { option, points } = buildScatterOption({
      rows: [
        row('a', ['claude-opus-4-8']),
        row('b', ['claude-fable-5'], { billedM: 3 }),
        row('z', ['claude-opus-4-8'], { billedM: 0 }),
      ],
      nameOf: (sid) => sid,
      labels,
    });
    expect(points).toBe(2);
    const o = option as Record<string, any>;
    const names = o.series.map((s: any) => s.name);
    expect(names).toEqual(['Opus 4.8', 'Fable 5']);
    expect(o.series[0].itemStyle.color).toBe('#7da7ff');
    expect(o.series[1].itemStyle.color).toBe('#f0b429');
  });
  it('이상점(비용 상위 2 ∪ 신호밀도 상위 2)만 라벨 formatter가 이름을 낸다', () => {
    const rows = [
      row('cheap', ['claude-opus-4-8'], { cost: 1, signals: 0 }),
      row('mid', ['claude-opus-4-8'], { cost: 2, signals: 1 }),
      row('pricey', ['claude-opus-4-8'], { cost: 900, signals: 1 }),
      row('noisy', ['claude-opus-4-8'], { cost: 3, signals: 60 }),
    ];
    const { option } = buildScatterOption({ rows, nameOf: (sid) => sid, labels });
    const s0 = (option as Record<string, any>).series[0];
    const fmt = s0.label.formatter;
    const byName = Object.fromEntries(
      s0.data.map((d: { name: string }) => [d.name, fmt({ data: d })]),
    );
    expect(byName['pricey']).toBe('pricey');
    expect(byName['noisy']).toBe('noisy');
    expect(byName['cheap']).toBe('');
  });
  it('점 크기 하한 10·상한 52, 밝은 테두리·탄성 마운트 애니메이션', () => {
    const { option } = buildScatterOption({
      rows: [row('a', ['claude-opus-4-8'], { cost: 0 }), row('b', ['claude-opus-4-8'], { cost: 10000 })],
      nameOf: (sid) => sid,
      labels,
    });
    const s0 = (option as Record<string, any>).series[0];
    expect(s0.symbolSize([1, 1, 0])).toBe(10);
    expect(s0.symbolSize([1, 1, 1e9])).toBe(52);
    expect(s0.itemStyle.borderWidth).toBeGreaterThanOrEqual(1.5);
    expect((option as Record<string, any>).animationEasing).toBe('elasticOut');
  });
  it('중앙값 점선 markLine 존재', () => {
    const { option } = buildScatterOption({
      rows: [row('a', ['claude-opus-4-8']), row('b', ['claude-opus-4-8'], { billedM: 4 })],
      nameOf: (sid) => sid,
      labels,
    });
    const s0 = (option as Record<string, any>).series[0];
    expect(s0.markLine.data.length).toBe(2);
  });
});
