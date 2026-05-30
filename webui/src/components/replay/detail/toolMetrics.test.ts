import { describe, expect, it } from 'vitest';
import { buildToolMetrics } from './toolMetrics';
import type { GraphNodeDto } from '../../../api/types';

const logNode = (eventName: string, attrs: Record<string, unknown>): GraphNodeDto => ({
  node_id: 'log-' + eventName, schema_version: '1', session_id: 's', node_kind: 'log_record',
  started_at: '', ended_at: null, merge_keys: {}, source_event_ids: [], source_uris: [],
  payload: { event_name: eventName, attributes: attrs },
});

describe('buildToolMetrics', () => {
  it('merges tool_result + tool_decision log attributes', () => {
    const facets = [
      logNode('tool_result', { duration_ms: '57', success: 'true', tool_input_size_bytes: '362', tool_result_size_bytes: '302', 'event.sequence': 763 }),
      logNode('tool_decision', { decision_source: 'config', decision_type: 'accept' }),
    ];
    const m = buildToolMetrics(facets);
    expect(m.durationMs).toBe(57);
    expect(m.success).toBe(true);
    expect(m.inputBytes).toBe(362);
    expect(m.resultBytes).toBe(302);
    expect(m.decisionSource).toBe('config');
    expect(m.decisionType).toBe('accept');
    expect(m.sequence).toBe(763);
  });
  it('returns nulls when facets absent', () => {
    const m = buildToolMetrics([]);
    expect(m.durationMs).toBeNull();
    expect(m.success).toBeNull();
  });
  it('ignores non-log_record nodes', () => {
    const span = { ...logNode('x', {}), node_kind: 'otel_span' } as GraphNodeDto;
    const m = buildToolMetrics([span]);
    expect(m.durationMs).toBeNull();
  });
});
