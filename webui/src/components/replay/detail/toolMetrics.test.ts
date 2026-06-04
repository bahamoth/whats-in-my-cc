import { describe, it, expect } from 'vitest';
import { buildToolMetrics } from './toolMetrics';

// buildToolMetrics now takes the flat `attributes` maps of the matching
// tool_result / tool_decision log_records (no FacetEntry / graph facet).
describe('buildToolMetrics', () => {
  it('folds duration/success/sizes/decision/sequence from attribute maps', () => {
    const m = buildToolMetrics([
      { duration_ms: '57', success: 'true', tool_input_size_bytes: '362', tool_result_size_bytes: '302', 'event.sequence': 763 },
      { decision_source: 'config', decision_type: 'accept' },
    ]);
    expect(m.durationMs).toBe(57);
    expect(m.success).toBe(true);
    expect(m.inputBytes).toBe(362);
    expect(m.resultBytes).toBe(302);
    expect(m.sequence).toBe(763);
    expect(m.decisionSource).toBe('config');
    expect(m.decisionType).toBe('accept');
  });

  it('returns all-null for no attribute maps', () => {
    const m = buildToolMetrics([]);
    expect(m.durationMs).toBeNull();
    expect(m.success).toBeNull();
  });
});
