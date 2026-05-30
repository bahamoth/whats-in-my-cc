import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { EntityMetricsPanel } from './EntityMetricsPanel';

describe('EntityMetricsPanel', () => {
  it('renders tool metrics with meaning when kind=tool_call', () => {
    render(
      <EntityMetricsPanel
        kind="tool_call"
        toolMetrics={{
          durationMs: 57,
          success: true,
          decisionSource: 'config',
          decisionType: 'accept',
          inputBytes: 362,
          resultBytes: 302,
          sequence: 763,
        }}
        llmMetrics={null}
      />,
    );
    expect(screen.getByText(/결정 출처/)).toBeInTheDocument();
    expect(screen.getByText(/accept/)).toBeInTheDocument();
  });

  it('renders response metrics when kind=assistant_message', () => {
    render(
      <EntityMetricsPanel
        kind="assistant_message"
        toolMetrics={null}
        llmMetrics={{
          requestId: 'r',
          durationMs: 28900,
          ttftMs: 3100,
          inputTokens: 2,
          outputTokens: 2300,
          cacheReadTokens: 290000,
          cacheCreationTokens: 2200,
          stopReason: 'tool_use',
          attempt: 1,
          success: true,
          model: 'claude-opus-4-8',
        }}
      />,
    );
    expect(screen.getByText(/출력 토큰/)).toBeInTheDocument();
  });

  it('shows uncollected when tool metrics all null', () => {
    render(
      <EntityMetricsPanel
        kind="tool_call"
        toolMetrics={{
          durationMs: null,
          success: null,
          decisionSource: null,
          decisionType: null,
          inputBytes: null,
          resultBytes: null,
          sequence: null,
        }}
        llmMetrics={null}
      />,
    );
    expect(screen.getByText(/미수집/)).toBeInTheDocument();
  });
});
