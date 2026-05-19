import { describe, expect, it } from 'vitest';
import { laneForNodeKind } from '../laneMapping';

describe('laneForNodeKind', () => {
  it('maps tool_call to Action', () => {
    expect(laneForNodeKind('tool_call')).toBe('Action');
  });

  it('maps otel_span to OTel', () => {
    expect(laneForNodeKind('otel_span')).toBe('OTel');
  });

  it('returns null for unknown kinds', () => {
    expect(laneForNodeKind('unknown_kind')).toBeNull();
  });
});
