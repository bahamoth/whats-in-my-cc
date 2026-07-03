import { describe, expect, it } from 'vitest';
import {
  sortSeriesAscending,
  cohortModels,
  cohortSegments,
  cohortBoundaries,
  type CohortSegment,
} from '../seriesView';
import type { SessionSeriesRowDto } from '../../api/types';

// 대시보드(B-1) 코호트 로직의 SSOT 테스트. 원칙:
// - 경계는 "관측된 비어있지 않은 값이 달라질 때"만 — fingerprint가 빈 세션은
//   변화의 증거가 아니므로 직전 코호트를 이어간다(가짜 경계 금지).
// - '<synthetic>' 모델 항목은 CC가 주입하는 합성 레코드라 코호트 라벨에서
//   제외한다(모든 세션에 고르게 나타나 경계 판정에 기여하지 않음).

function row(
  id: string,
  first: string,
  models: string[] = [],
  ccVersions: string[] = [],
): SessionSeriesRowDto {
  return {
    session_id: id,
    first_observed_at: first,
    last_observed_at: first,
    event_count: 1,
    metrics: {
      session_id: id,
      tool_call_total: 0,
      tool_failure_count: 0,
      verification_total: 0,
      verification_passed: 0,
      verification_failed: 0,
      verification_unknown: 0,
      verification_not_executed: 0,
      context_bloat_count: 0,
      tool_user_rejected: 0,
      tool_policy_denied: 0,
      tool_cancelled: 0,
      tool_backgrounded: 0,
      turn_duration_ms_total: 0,
      turn_duration_count: 0,
      api_error_count: 0,
      compact_boundary_count: 0,
      tool_result_truncated_count: 0,
      user_interruption_count: 0,
      detector_firing: {},
    },
    fingerprint: {
      session_id: id,
      models,
      cc_versions: ccVersions,
      git_branches: [],
      cwds: [],
      entrypoints: [],
    },
  };
}

describe('sortSeriesAscending', () => {
  it('sorts by first_observed_at ascending (API returns latest-first)', () => {
    const rows = [
      row('c', '2026-07-03T10:00:00+00:00'),
      row('a', '2026-07-01T10:00:00+00:00'),
      row('b', '2026-07-02T10:00:00+00:00'),
    ];
    expect(sortSeriesAscending(rows).map((r) => r.session_id)).toEqual(['a', 'b', 'c']);
  });

  it('breaks timestamp ties by session_id for a stable axis', () => {
    const rows = [row('b', '2026-07-01T10:00:00+00:00'), row('a', '2026-07-01T10:00:00+00:00')];
    expect(sortSeriesAscending(rows).map((r) => r.session_id)).toEqual(['a', 'b']);
  });
});

describe('cohortModels', () => {
  it("drops the CC-injected '<synthetic>' pseudo-model from labels", () => {
    expect(cohortModels({ session_id: 's', models: ['<synthetic>', 'claude-fable-5'], cc_versions: [], git_branches: [], cwds: [], entrypoints: [] })).toEqual(['claude-fable-5']);
  });
});

describe('cohortSegments', () => {
  const pickModels = (r: SessionSeriesRowDto) => cohortModels(r.fingerprint);

  it('splits when the observed value set changes', () => {
    const rows = [
      row('a', '2026-07-01T00:00:00+00:00', ['opus-4-7']),
      row('b', '2026-07-02T00:00:00+00:00', ['opus-4-7']),
      row('c', '2026-07-03T00:00:00+00:00', ['fable-5']),
    ];
    expect(cohortSegments(rows, pickModels)).toEqual<CohortSegment[]>([
      { start: 0, end: 1, label: 'opus-4-7', known: true },
      { start: 2, end: 2, label: 'fable-5', known: true },
    ]);
  });

  it('carries the previous cohort across sessions with no fingerprint (no fake boundary)', () => {
    const rows = [
      row('a', '2026-07-01T00:00:00+00:00', ['fable-5']),
      row('b', '2026-07-02T00:00:00+00:00', []),
      row('c', '2026-07-03T00:00:00+00:00', ['fable-5']),
    ];
    expect(cohortSegments(rows, pickModels)).toEqual<CohortSegment[]>([
      { start: 0, end: 2, label: 'fable-5', known: true },
    ]);
  });

  it('marks a leading run without any observation as unknown', () => {
    const rows = [
      row('a', '2026-07-01T00:00:00+00:00', []),
      row('b', '2026-07-02T00:00:00+00:00', ['fable-5']),
    ];
    expect(cohortSegments(rows, pickModels)).toEqual<CohortSegment[]>([
      { start: 0, end: 0, label: '', known: false },
      { start: 1, end: 1, label: 'fable-5', known: true },
    ]);
  });

  it('joins multi-value sets deterministically', () => {
    const rows = [row('a', '2026-07-01T00:00:00+00:00', ['b-model', 'a-model'])];
    expect(cohortSegments(rows, pickModels)[0].label).toBe('a-model + b-model');
  });

  it('returns no segments for an empty series', () => {
    expect(cohortSegments([], pickModels)).toEqual([]);
  });
});

describe('cohortBoundaries', () => {
  it('yields one boundary per known→known segment change', () => {
    const segments: CohortSegment[] = [
      { start: 0, end: 1, label: '2.1.197', known: true },
      { start: 2, end: 4, label: '2.1.198', known: true },
      { start: 5, end: 5, label: '2.1.200', known: true },
    ];
    expect(cohortBoundaries(segments)).toEqual([
      { index: 2, from: '2.1.197', to: '2.1.198' },
      { index: 5, from: '2.1.198', to: '2.1.200' },
    ]);
  });

  it('does not fabricate a boundary out of an unknown leading segment', () => {
    const segments: CohortSegment[] = [
      { start: 0, end: 0, label: '', known: false },
      { start: 1, end: 2, label: 'fable-5', known: true },
    ];
    expect(cohortBoundaries(segments)).toEqual([]);
  });
});
