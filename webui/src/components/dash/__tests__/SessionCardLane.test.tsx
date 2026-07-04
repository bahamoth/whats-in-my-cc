/** 세션 카드 레인 — 실제 날짜 위치·레인 스택·전체 모델명·신호 밀도 앰버·
 *  클릭 진입·미측정 '—' 표기를 잠근다. */
import { render, screen, cleanup, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi, afterEach } from 'vitest';
import { SessionCardLane } from '../SessionCardLane';
import { I18nProvider } from '../../../i18n';
import type { SessionSeriesRowDto } from '../../../api/types';

afterEach(cleanup);

function row(id: string, date: string, models: string[], over: Partial<{
  cost: number; signals: number; events: number; billed: boolean; guards: number;
}> = {}): SessionSeriesRowDto {
  const o = { cost: 50, signals: 2, events: 1000, billed: true, guards: 12, ...over };
  return {
    session_id: id,
    first_observed_at: `2026-${date}T04:00:00+00:00`,
    last_observed_at: `2026-${date}T08:00:00+00:00`,
    event_count: o.events,
    metrics: {
      session_id: id,
      tool_call_total: 10,
      tool_failure_count: o.signals,
      verification_total: o.guards,
      verification_passed: o.guards > 0 ? 10 : 0,
      verification_failed: o.guards > 0 ? 2 : 0,
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
      input_tokens: o.billed ? 100_000 : 0,
      output_tokens: o.billed ? 500_000 : 0,
      cache_read_input_tokens: o.billed ? 9_000_000 : 0,
      cache_creation_input_tokens: o.billed ? 400_000 : 0,
      estimated_cost_usd: o.cost,
      compact_boundary_count: 0,
      tool_result_truncated_count: 0,
      user_interruption_count: 0,
      detector_firing: {},
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

function mount(rows: SessionSeriesRowDto[], onOpen = vi.fn()) {
  render(
    <I18nProvider initialLocale="en">
      <SessionCardLane rows={rows} nameOf={(sid) => `name-${sid}`} onOpen={onOpen} />
    </I18nProvider>,
  );
  return onOpen;
}

describe('SessionCardLane', () => {
  it('세션마다 카드 1장 — 전체 모델명·이름 렌더', () => {
    mount([row('a', '06-05', ['claude-opus-4-8']), row('b', '06-12', ['claude-fable-5'])]);
    expect(screen.getByText('name-a')).toBeInTheDocument();
    expect(screen.getByText('Opus 4.8')).toBeInTheDocument();
    expect(screen.getByText('Fable 5')).toBeInTheDocument();
  });
  it('같은 날 두 세션은 다른 레인(top)으로 스택', () => {
    mount([row('a', '06-05', []), row('b', '06-05', []), row('c', '06-20', [])]);
    const cards = screen.getAllByRole('button');
    const tops = cards.map((c) => (c as HTMLElement).style.top);
    expect(tops[0]).not.toBe(tops[1]);
    expect(tops[2]).toBe(tops[0]);
  });
  it('카드 클릭 → onOpen(session_id)', () => {
    const onOpen = mount([row('a', '06-05', [])]);
    fireEvent.click(screen.getByText('name-a'));
    expect(onOpen).toHaveBeenCalledWith('a');
  });
  it('usage 미측정이지만 활동(신호/가드)이 있으면 정상 카드에 — 표기', () => {
    mount([row('a', '06-05', [], { billed: false, cost: 0 })]);
    const line = screen.getByText(/2 signals/).closest('div');
    expect(line?.textContent).toContain('— ·');
  });
  it('무활동 세션(미측정·신호 0·가드 0)은 고스트 칩으로 강등', () => {
    mount([
      row('a', '06-05', ['claude-opus-4-8']),
      row('ghost', '06-06', [], { billed: false, cost: 0, signals: 0, guards: 0 }),
    ]);
    const g = screen.getByText('name-ghost').closest('[role="button"]') as HTMLElement;
    expect(g.dataset.ghost).toBe('true');
    // 고스트에는 지표 줄이 없다 — 0 나열로 고장처럼 보이지 않게.
    expect(g.textContent).not.toContain('signals');
  });
  it('시간축 틱 라벨이 렌더된다', () => {
    mount([row('a', '06-05', []), row('b', '06-20', [])]);
    expect(screen.getByText('06-05')).toBeInTheDocument();
    expect(screen.getAllByText(/06-\d\d/).length).toBeGreaterThan(2);
  });
});
