import { describe, it, expect } from 'vitest';
import { buildToolMetricsFromEvents, buildLlmMetricsFromEvents } from '../eventMetrics';
import type { ObservedEventDto } from '../../../../api/types';

function ev(p: Partial<ObservedEventDto> & { event_id: string; kind: string }): ObservedEventDto {
  return {
    raw_event_id: '', session_id: 's', event_uuid: null, parent_uuid: null,
    observed_at: '2026-05-28T00:00:00Z', actor: 'system', subkind: null, tool_use_id: null,
    tool_name: null, turn_id: null, is_sidechain: false, is_meta: false, payload: {}, ...p,
  } as ObservedEventDto;
}

// Detail metrics derived DIRECTLY from ObservedEvents by correlation key — no
// graph node / facet fold. Real shapes:
//   tool_result log_record payload = { event_name, attributes:{ tool_use_id,
//     success, duration_ms, tool_input_size_bytes, ... } }  (DB-verified keys)
//   llm_request span payload = { raw_span:{ name, attributes:[{key,value}] } }
//   api_request log payload = { event_name:'api_request', attributes:{ request_id, cost_usd } }
describe('buildToolMetricsFromEvents', () => {
  it('builds tool metrics from the tool_result log_record matching tool_use_id', () => {
    const events = [
      ev({ event_id: 'call', kind: 'tool_call', tool_use_id: 'u1' }),
      ev({ event_id: 'log1', kind: 'log_record', payload: { event_name: 'tool_result',
        attributes: { tool_use_id: 'u1', success: 'true', duration_ms: '57',
          tool_input_size_bytes: '362', tool_result_size_bytes: '302' } } }),
      // unrelated tool_result (different tool_use_id) must be ignored
      ev({ event_id: 'log2', kind: 'log_record', payload: { event_name: 'tool_result',
        attributes: { tool_use_id: 'OTHER', success: 'false', duration_ms: '999' } } }),
    ];
    const m = buildToolMetricsFromEvents(events, 'u1');
    expect(m.durationMs).toBe(57);
    expect(m.success).toBe(true);
    expect(m.inputBytes).toBe(362);
    expect(m.resultBytes).toBe(302);
  });

  it('returns an all-null ToolMetrics when no telemetry matches', () => {
    const m = buildToolMetricsFromEvents([ev({ event_id: 'call', kind: 'tool_call', tool_use_id: 'u1' })], 'u1');
    expect(m.durationMs).toBeNull();
    expect(m.success).toBeNull();
  });

  // REGRESSION GUARD: a tool_decision log_record (NOT a tool_result) must be
  // routed to the 'tool_decision_log' facet kind so its decision_source /
  // decision_type reach ToolMetrics. The two event_names share a code path in
  // buildToolMetricsFromEvents; a regression that only handled 'tool_result'
  // would silently drop every permission-decision metric.
  it('folds a tool_decision log_record into decision_source / decision_type by tool_use_id', () => {
    const events = [
      ev({ event_id: 'call', kind: 'tool_call', tool_use_id: 'u1' }),
      ev({ event_id: 'dec', kind: 'log_record', payload: { event_name: 'tool_decision',
        attributes: { tool_use_id: 'u1', decision_source: 'config', decision_type: 'allow' } } }),
    ];
    const m = buildToolMetricsFromEvents(events, 'u1');
    expect(m.decisionSource).toBe('config');
    expect(m.decisionType).toBe('allow');
  });
});

describe('buildLlmMetricsFromEvents', () => {
  it('merges the llm_request span (by request_id) with the api_request log cost', () => {
    const span = ev({ event_id: 's1', kind: 'otel_span', payload: { raw_span: {
      name: 'claude_code.llm_request',
      attributes: [
        { key: 'request_id', value: { stringValue: 'r1' } },
        { key: 'gen_ai.usage.output_tokens', value: { intValue: '2300' } },
      ],
    } } });
    const apiLog = ev({ event_id: 'a1', kind: 'log_record', payload: {
      event_name: 'api_request', attributes: { request_id: 'r1', cost_usd: 0.0123, query_source: 'repl_main_thread' } } });
    const m = buildLlmMetricsFromEvents([span, apiLog], 'r1');
    expect(m).not.toBeNull();
    expect(m!.requestId).toBe('r1');
    expect(m!.costUsd).toBe(0.0123);
  });

  it('returns null when no span matches the request_id', () => {
    expect(buildLlmMetricsFromEvents([], 'r1')).toBeNull();
  });

  // REGRESSION GUARD: cost is OPTIONAL. The llm_request span is the base metric
  // source; the api_request cost log is a separate, sometimes-absent record.
  // A span with no matching cost log must still yield the base metrics with a
  // null costUsd — NOT collapse the whole result to null. (Guards against a
  // future change that requires both records to be present.)
  it('returns the span base metrics with null cost when no api_request log is present', () => {
    const span = ev({ event_id: 's1', kind: 'otel_span', payload: { raw_span: {
      name: 'claude_code.llm_request',
      attributes: [
        { key: 'request_id', value: { stringValue: 'r1' } },
        { key: 'output_tokens', value: { intValue: '900' } },
      ],
    } } });
    const m = buildLlmMetricsFromEvents([span], 'r1');
    expect(m).not.toBeNull();
    expect(m!.requestId).toBe('r1');
    expect(m!.outputTokens).toBe(900);
    expect(m!.costUsd).toBeNull();
  });
});
