/**
 * Type-level contract checks. These are compile-time-only checks (the test
 * runtime body is trivial) wired so a regression in `types.ts` surfaces as a
 * vitest failure as well as a `tsc -b` failure.
 *
 * Locked invariants:
 *  - `SignalDto.evidence_refs` is a non-omittable array
 *  - `VerificationRunDto.covered_diff_hunk_ids` is `string[]`
 */
import { describe, expect, it } from 'vitest';
import type {
  SignalDto,
  VerificationRunDto,
  DiffHunkDto,
} from '../types';

describe('types.ts contract', () => {
  it('SignalDto.evidence_refs is a non-omittable array', () => {
    const s: SignalDto = {
      signal_id: 'sig1',
      schema_version: '1',
      session_id: 'S1',
      detector: 'tool_failure',
      subkind: null,
      summary: 'exit code 1',
      evidence_refs: ['01KSQKD5CT8BHH1DAS4YNKJBVB'],
      facts: {},
      provenance: {},
      created_at: '2026-06-07T00:00:00Z',
    };
    expect(Array.isArray(s.evidence_refs)).toBe(true);
  });

  it('VerificationRunDto and DiffHunkDto wire fields', () => {
    const vr: VerificationRunDto = {
      verification_run_id: 'vr1',
      schema_version: 'v1',
      session_id: 'S1',
      source: 'transcript',
      command: 'cargo test',
      command_kind: 'test_suite_rust',
      trigger_event_id: 'e1',
      trigger_tool_use_id: null,
      status: 'passed',
      status_provenance: 'measured',
      detection_basis: 'known_tool',
      status_basis: 'exit',
      started_at: '2026-05-29T00:00:00Z',
      ended_at: null,
      exit_code: 0,
      failure_summary: null,
      covered_diff_hunk_ids: ['dh1', 'dh2'],
    };
    const dh: DiffHunkDto = {
      diff_hunk_id: 'dh1',
      session_id: 'S1',
      file_path: 'a.rs',
      change_type: 'modify',
      line_range_after_start: 1,
      line_range_after_end: 2,
      introduced_by_event_id: 'e1',
      introduced_by_tool_use_id: null,
      patch_preview: '',
      lines_added: 2,
      lines_removed: 1,
      user_modified: false,
    };
    expect(vr.covered_diff_hunk_ids).toEqual(['dh1', 'dh2']);
    expect(vr.detection_basis).toBe('known_tool');
    expect(vr.status_basis).toBe('exit');
    expect(dh.file_path).toBe('a.rs');
  });
});
