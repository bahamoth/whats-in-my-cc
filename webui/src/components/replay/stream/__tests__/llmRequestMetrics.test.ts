import { describe, it, expect } from 'vitest';
import {
  buildLlmRequestMetrics,
  formatDuration,
  formatQuerySource,
  formatThroughput,
  formatTokens,
  formatUsd,
  parseApiRequestLog,
} from '../llmRequestMetrics';
import type { ObservedEventDto } from '../../../../api/types';

function span(attrs: Record<string, string | number | boolean>, name = 'claude_code.llm_request'): ObservedEventDto {
  const attributes = Object.entries(attrs).map(([key, v]) => {
    const value =
      typeof v === 'number'
        ? Number.isInteger(v) ? { intValue: String(v) } : { doubleValue: v }
        : typeof v === 'boolean'
        ? { boolValue: v }
        : { stringValue: v };
    return { key, value };
  });
  return {
    event_id: `sp-${attrs.request_id ?? name}`,
    raw_event_id: '', session_id: 's', event_uuid: null, parent_uuid: null,
    observed_at: '2026-05-30T00:00:00Z', actor: 'system', kind: 'otel_span',
    subkind: null, tool_use_id: null, tool_name: null, turn_id: null,
    is_sidechain: false, is_meta: false,
    payload: { raw_span: { name, attributes } },
  } as ObservedEventDto;
}

describe('buildLlmRequestMetrics', () => {
  it('maps request_id → metrics from a claude_code.llm_request span', () => {
    const m = buildLlmRequestMetrics([
      span({
        request_id: 'req-1', duration_ms: 11869, ttft_ms: 4887, input_tokens: 2,
        output_tokens: 398, cache_read_tokens: 204707, cache_creation_tokens: 4577,
        stop_reason: 'tool_use', attempt: 1, success: true, model: 'claude-opus-4-8',
      }),
    ]);
    const got = m.get('req-1');
    expect(got).toBeDefined();
    expect(got!.durationMs).toBe(11869);
    expect(got!.ttftMs).toBe(4887);
    expect(got!.outputTokens).toBe(398);
    expect(got!.cacheReadTokens).toBe(204707);
    expect(got!.stopReason).toBe('tool_use');
    expect(got!.attempt).toBe(1);
    expect(got!.success).toBe(true);
    expect(got!.model).toBe('claude-opus-4-8');
  });

  it('ignores non-llm_request spans', () => {
    const m = buildLlmRequestMetrics([
      span({ request_id: 'req-x', duration_ms: 5 }, 'claude_code.tool'),
    ]);
    expect(m.size).toBe(0);
  });

  it('falls back to gen_ai.response.id when request_id attr is absent', () => {
    const m = buildLlmRequestMetrics([
      span({ 'gen_ai.response.id': 'req-2', duration_ms: 100 }),
    ]);
    expect(m.get('req-2')?.durationMs).toBe(100);
  });
});

describe('formatDuration / formatTokens', () => {
  it('formats sub-second as ms and seconds with one decimal', () => {
    expect(formatDuration(850)).toBe('850ms');
    expect(formatDuration(11900)).toBe('11.9s');
    expect(formatDuration(null)).toBeNull();
  });
  it('formats tokens with k suffix above 1000', () => {
    expect(formatTokens(398)).toBe('398');
    expect(formatTokens(1540)).toBe('1.5k');
    expect(formatTokens(204707)).toBe('205k');
    expect(formatTokens(null)).toBeNull();
  });
});

describe('parseApiRequestLog — measured per-request cost from the api_request_log facet', () => {
  it('extracts cost_usd (the measured cost, NOT the token×rate estimate)', () => {
    const got = parseApiRequestLog({
      event_name: 'api_request',
      attributes: { cost_usd: 0.2360595, model: 'claude-opus-4-8', query_source: 'repl_main_thread' },
    });
    expect(got).not.toBeNull();
    expect(got!.costUsd).toBeCloseTo(0.2360595, 6);
  });
  it('returns null cost when absent / data is not an api_request_log', () => {
    expect(parseApiRequestLog({ attributes: {} })!.costUsd).toBeNull();
    expect(parseApiRequestLog(null)).toBeNull();
    expect(parseApiRequestLog({})).toBeNull();
  });
});

describe('formatUsd', () => {
  it('uses 4 decimals under $1 (sub-cent precision) and 2 at/above', () => {
    expect(formatUsd(0.2360595)).toBe('$0.2361');
    expect(formatUsd(0.0098)).toBe('$0.0098');
    expect(formatUsd(3.4239)).toBe('$3.42');
    expect(formatUsd(null)).toBeNull();
  });
});

describe('formatQuerySource', () => {
  it('labels the main thread and subagents (builtin + custom)', () => {
    expect(formatQuerySource('repl_main_thread')).toBe('메인 스레드');
    expect(formatQuerySource('agent:builtin:general-purpose')).toBe('서브에이전트 · general-purpose');
    expect(formatQuerySource('agent:builtin:Explore')).toBe('서브에이전트 · Explore');
    expect(formatQuerySource('agent:custom')).toBe('서브에이전트 · custom');
    expect(formatQuerySource(null)).toBeNull();
    expect(formatQuerySource(undefined)).toBeNull();
  });
});

describe('formatThroughput', () => {
  it('computes output tokens / seconds; null when missing or zero duration', () => {
    expect(formatThroughput(608, 9594)).toBe('63 tok/s');
    expect(formatThroughput(null, 1000)).toBeNull();
    expect(formatThroughput(100, 0)).toBeNull();
    expect(formatThroughput(100, null)).toBeNull();
  });
});
