import { describe, it, expect } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { ResponseMetricsPanel } from '../ResponseMetricsPanel';
import type { LlmRequestMetrics } from '../../stream/llmRequestMetrics';

const m: LlmRequestMetrics = {
  requestId: 'req-1', durationMs: 28900, ttftMs: 3100, inputTokens: 2,
  outputTokens: 2300, cacheReadTokens: 290000, cacheCreationTokens: 2200,
  stopReason: 'tool_use', attempt: 1, success: true, model: 'claude-opus-4-8', costUsd: null,
};

function rowFor(label: string): HTMLElement {
  return screen.getByText(label).closest('div') as HTMLElement;
}

describe('ResponseMetricsPanel', () => {
  it('renders the full metric set with values', () => {
    render(<ResponseMetricsPanel metrics={m} />);
    expect(within(rowFor('소요 시간')).getByText('28.9s')).toBeInTheDocument();
    expect(within(rowFor('첫 토큰까지(ttft)')).getByText('3.1s')).toBeInTheDocument();
    expect(within(rowFor('출력 토큰')).getByText('2.3k')).toBeInTheDocument();
    expect(within(rowFor('종료 사유')).getByText('tool_use')).toBeInTheDocument();
    expect(within(rowFor('모델')).getByText('claude-opus-4-8')).toBeInTheDocument();
  });

  it('attaches explanatory tooltips to the token / cache rows', () => {
    render(<ResponseMetricsPanel metrics={m} />);
    for (const label of ['출력 토큰', '입력 토큰', '캐시 읽기', '캐시 생성']) {
      expect(within(rowFor(label)).getByRole('button', { name: `${label} 설명` })).toBeInTheDocument();
    }
    // Non-token rows have no tooltip trigger.
    expect(within(rowFor('소요 시간')).queryByRole('button')).toBeNull();
  });

  it('states that the reasoning content is not recorded', () => {
    render(<ResponseMetricsPanel metrics={m} />);
    expect(screen.getByText(/transcript에 기록되지 않습니다/)).toBeInTheDocument();
  });

  it('handles null metrics gracefully (span outside window)', () => {
    render(<ResponseMetricsPanel metrics={null} />);
    expect(screen.getByText(/지표를 현재 윈도우에서 찾지 못했습니다/)).toBeInTheDocument();
  });

  it('flags abnormal responses (max_tokens)', () => {
    render(<ResponseMetricsPanel metrics={{ ...m, stopReason: 'max_tokens' }} />);
    expect(screen.getByText(/잘림\/재시도\/실패 신호/)).toBeInTheDocument();
  });
});
