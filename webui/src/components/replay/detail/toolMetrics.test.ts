import { describe, expect, it } from 'vitest';
import { buildToolMetrics } from './toolMetrics';
import type { FacetEntry } from '../facets/entityFacets';

const facet = (kind: string, attrs: Record<string, unknown>): FacetEntry => ({
  facet_kind: kind,
  basis: 'tool_use_id',
  source_event_id: `ev-${kind}`,
  data: { attributes: attrs },
});

describe('buildToolMetrics', () => {
  it('merges tool_result + tool_decision facet attributes', () => {
    const facets = [
      facet('tool_result_log', {
        duration_ms: '57',
        success: 'true',
        tool_input_size_bytes: '362',
        tool_result_size_bytes: '302',
        'event.sequence': 763,
      }),
      facet('tool_decision_log', { decision_source: 'config', decision_type: 'accept' }),
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
  it('ignores non-tool facet kinds', () => {
    const m = buildToolMetrics([facet('llm_request_span', { duration_ms: '99' })]);
    expect(m.durationMs).toBeNull();
  });
  it('degrades to all-null when data has no attributes', () => {
    const f: FacetEntry = {
      facet_kind: 'tool_result_log',
      basis: 'tool_use_id',
      source_event_id: 'e',
      data: {},
    };
    let m!: ReturnType<typeof buildToolMetrics>;
    expect(() => {
      m = buildToolMetrics([f]);
    }).not.toThrow();
    expect(m.durationMs).toBeNull();
    expect(m.success).toBeNull();
    expect(m.decisionSource).toBeNull();
    expect(m.decisionType).toBeNull();
    expect(m.inputBytes).toBeNull();
    expect(m.resultBytes).toBeNull();
    expect(m.sequence).toBeNull();
  });
  it('degrades to all-null when data.attributes is a non-object truthy value', () => {
    // `data.attributes` is `unknown` at the boundary — a string (or any
    // non-object) must not throw and must yield all-null.
    const f = {
      facet_kind: 'tool_result_log',
      basis: 'tool_use_id',
      source_event_id: 'e',
      data: { attributes: 'oops' },
    } as unknown as FacetEntry;
    let m!: ReturnType<typeof buildToolMetrics>;
    expect(() => {
      m = buildToolMetrics([f]);
    }).not.toThrow();
    expect(m.durationMs).toBeNull();
    expect(m.success).toBeNull();
    expect(m.decisionSource).toBeNull();
    expect(m.decisionType).toBeNull();
    expect(m.inputBytes).toBeNull();
    expect(m.resultBytes).toBeNull();
    expect(m.sequence).toBeNull();
  });
});
