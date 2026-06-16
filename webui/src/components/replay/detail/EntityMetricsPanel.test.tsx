import { screen } from '@testing-library/react';
import { renderWithI18n as render } from '../../../test/i18nRender';
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
          costUsd: null,
        }}
      />,
    );
    expect(screen.getByText(/출력 토큰/)).toBeInTheDocument();
  });

  it('renders hook metrics (result, duration, command) when kind=hook_event', () => {
    // hook_event metrics come from the node's own payload (real hook_success
    // shape; see stream/hookFacet.test.ts), not from toolMetrics/llmMetrics.
    render(
      <EntityMetricsPanel
        kind="hook_event"
        toolMetrics={null}
        llmMetrics={null}
        payload={{
          type: 'hook_success',
          hookName: 'PreToolUse:Bash',
          hookEvent: 'PreToolUse',
          exitCode: 0,
          durationMs: 330,
          command: 'python remove_ai_footer.py',
          stdout: '{"continue": true}\n',
          stderr: '',
        }}
      />,
    );
    expect(screen.getByText(/소요 시간/)).toBeInTheDocument();
    expect(screen.getByText('330ms')).toBeInTheDocument();
    expect(screen.getByText(/결과/)).toBeInTheDocument();
    expect(screen.getByText('ok')).toBeInTheDocument();
    expect(screen.getByText(/python remove_ai_footer\.py/)).toBeInTheDocument();
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

  // ---- S7 (UX 재설계) — HOW grouped into subheadings + provenance pills ----

  it('groups assistant response metrics under LLM 동작 / 토큰 / 비용 subheadings with provenance', () => {
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
          costUsd: 0.043,
        }}
      />,
    );
    expect(screen.getByText('LLM 동작')).toBeInTheDocument();
    expect(screen.getByText('토큰')).toBeInTheDocument();
    expect(screen.getByText('비용')).toBeInTheDocument();
    // every group states its trust level (measured OTel span / api_request_log)
    const badges = screen.getAllByTestId('provenance-badge');
    expect(badges.length).toBeGreaterThanOrEqual(3);
    expect(badges.every((b) => b.textContent === '측정')).toBe(true);
    // the grouped rows themselves still render their values
    expect(screen.getByText(/출력 토큰/)).toBeInTheDocument();
    expect(screen.getByText(/첫 토큰까지/)).toBeInTheDocument();
  });

  it('groups tool metrics under a 도구 실행 subheading with a provenance pill', () => {
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
    expect(screen.getByText('도구 실행')).toBeInTheDocument();
    expect(screen.getByTestId('provenance-badge')).toHaveTextContent('측정');
  });
});
