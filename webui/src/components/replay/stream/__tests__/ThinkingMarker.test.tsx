import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { ThinkingMarker } from '../ThinkingMarker';
import type { LlmRequestMetrics } from '../llmRequestMetrics';
import type { ThinkingMarker as ThinkingMarkerData } from '../streamModel';

function metrics(over: Partial<LlmRequestMetrics> = {}): LlmRequestMetrics {
  return {
    requestId: 'req-1', durationMs: 11900, ttftMs: 3100, inputTokens: 2,
    outputTokens: 1540, cacheReadTokens: 290000, cacheCreationTokens: 2200,
    stopReason: 'tool_use', attempt: 1, success: true, model: 'claude-opus-4-8', costUsd: null, ...over,
  };
}
function marker(over: Partial<ThinkingMarkerData['events'][number]> = {}): ThinkingMarkerData {
  return {
    type: 'thinking', id: 'th-e1',
    events: [{ eventId: 'e1', timestamp: '2026-05-30T13:47:41Z', sigLen: 2000, requestId: 'req-1', metrics: metrics(), ...over }],
  };
}

describe('ThinkingMarker', () => {
  it('renders the 추론 label with duration and output tokens', () => {
    render(<ThinkingMarker marker={marker()} selectedEventId={null} onSelect={() => {}} />);
    expect(screen.getByText('추론')).toBeInTheDocument();
    expect(screen.getByText('11.9s')).toBeInTheDocument();
    expect(screen.getByText('1.5k tok')).toBeInTheDocument();
  });

  it('calls onSelect with the event id on click', () => {
    const onSelect = vi.fn();
    render(<ThinkingMarker marker={marker()} selectedEventId={null} onSelect={onSelect} />);
    fireEvent.click(screen.getByTestId('thinking-marker').querySelector('button')!);
    expect(onSelect).toHaveBeenCalledWith('e1');
  });

  it('degrades gracefully to — when metrics are absent', () => {
    render(<ThinkingMarker marker={marker({ metrics: null })} selectedEventId={null} onSelect={() => {}} />);
    expect(screen.getByText('추론')).toBeInTheDocument();
    expect(screen.getByText('—')).toBeInTheDocument();
    expect(screen.queryByText(/tok$/)).toBeNull();
  });

  it('shows a warning badge for abnormal responses (max_tokens / retry / failure)', () => {
    render(
      <ThinkingMarker
        marker={marker({ metrics: metrics({ stopReason: 'max_tokens' }) })}
        selectedEventId={null}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByLabelText('이상 응답')).toBeInTheDocument();
  });

  it('marks itself selected when its event id matches', () => {
    render(<ThinkingMarker marker={marker()} selectedEventId={'e1'} onSelect={() => {}} />);
    expect(screen.getByTestId('thinking-marker')).toHaveAttribute('data-selected', 'true');
  });
});
