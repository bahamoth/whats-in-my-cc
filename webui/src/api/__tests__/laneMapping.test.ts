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

  it('maps diff_hunk to Files (slice-10a — transcript-only file lineage)', () => {
    expect(laneForNodeKind('diff_hunk')).toBe('Files');
  });

  it('rejects file_event / git_commit since slice-10a removed those pipelines', () => {
    // Negative lock: future contributors shouldn't accidentally re-route the
    // deleted filesystem-source kinds — the Files lane is diff_hunk-only now.
    expect(laneForNodeKind('file_event')).toBeNull();
    expect(laneForNodeKind('git_commit')).toBeNull();
  });

  it('maps metric_sample / log_record to OTel (slice-6)', () => {
    expect(laneForNodeKind('metric_sample')).toBe('OTel');
    expect(laneForNodeKind('log_record')).toBe('OTel');
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
