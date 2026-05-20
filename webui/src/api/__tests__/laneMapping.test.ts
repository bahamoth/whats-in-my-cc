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

  it('maps file_event / git_commit / diff_hunk to Files (slice-5)', () => {
    expect(laneForNodeKind('file_event')).toBe('Files');
    expect(laneForNodeKind('git_commit')).toBe('Files');
    expect(laneForNodeKind('diff_hunk')).toBe('Files');
  });

  it('returns null for unknown kinds', () => {
    expect(laneForNodeKind('unknown_kind')).toBeNull();
  });
});

describe('LANES constant', () => {
  it('exposes 8 lanes with Files between State and Hook (slice-5)', () => {
    expect(LANES.length).toBe(8);
    expect(LANES).toEqual([
      'Intent',
      'Context',
      'Action',
      'State',
      'Files',
      'Hook',
      'OTel',
      'Quality',
    ]);
  });
});
