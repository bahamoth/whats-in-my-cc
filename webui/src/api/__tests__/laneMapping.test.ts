import { describe, expect, it } from 'vitest';
import { LANES, laneForNodeKind } from '../laneMapping';

describe('laneForNodeKind', () => {
  it('maps tool_call to Action', () => {
    expect(laneForNodeKind('tool_call')).toBe('Action');
  });

  it('maps otel_span to OTel', () => {
    expect(laneForNodeKind('otel_span')).toBe('OTel');
  });

  it('maps hook_event to Hook', () => {
    expect(laneForNodeKind('hook_event')).toBe('Hook');
  });

  it('returns null for unknown kinds', () => {
    expect(laneForNodeKind('unknown_kind')).toBeNull();
  });
});

describe('LANES constant', () => {
  it('exposes Hook as the 5th lane (after State, before OTel)', () => {
    expect(LANES).toContain('Hook');
    expect(LANES.length).toBe(7);
    expect(LANES.indexOf('Hook')).toBe(LANES.indexOf('State') + 1);
  });
});
