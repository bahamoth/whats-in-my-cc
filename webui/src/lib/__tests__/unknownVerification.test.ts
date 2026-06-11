import { describe, expect, it } from 'vitest';
import {
  hintForUnknown,
  collectUnknownVerification,
  type UnknownVerificationInput,
} from '../unknownVerification';
import type { VerificationRunDto } from '../../api/types';

function run(
  command: string,
  command_kind: string,
  status_basis: string,
  trigger_event_id: string,
): VerificationRunDto {
  return {
    verification_run_id: `vr_${trigger_event_id}`,
    schema_version: 'verification_run.v1',
    session_id: 's1',
    source: 'bash',
    command,
    command_kind,
    trigger_event_id,
    trigger_tool_use_id: null,
    status: 'unknown',
    status_provenance: 'unknown',
    detection_basis: 'known_tool',
    status_basis,
    started_at: '',
    ended_at: null,
    exit_code: null,
    failure_summary: null,
    covered_diff_hunk_ids: [],
  };
}

describe('hintForUnknown', () => {
  it('piped + summary-like text present → suggests extending the Rust heuristic', () => {
    const h = hintForUnknown('piped', 'RUN\n Tests  3 passed (3)');
    expect(h).toMatch(/looks_like_success|looks_like_failure/);
  });

  it('piped + no summary in output → flags the downstream filter cut it (not recoverable)', () => {
    const h = hintForUnknown('piped', '0\n');
    expect(h).toMatch(/요약 없음|복구 불가/);
  });

  it('disposition basis → expected unknown, no action', () => {
    expect(hintForUnknown('policy_denied', 'whatever')).toMatch(/disposition|정상 unknown/);
    expect(hintForUnknown('background', 'Command running in background')).toMatch(
      /disposition|정상 unknown/,
    );
  });

  it('exit basis but still unknown → asks to inspect content for a new pattern', () => {
    expect(hintForUnknown('exit', 'some opaque output')).toMatch(/exit|미인식|content/);
  });
});

describe('collectUnknownVerification', () => {
  it('keeps only unknown runs, groups by (command_kind, status_basis), counts desc', () => {
    const input: UnknownVerificationInput = {
      runs: [
        run('vitest 2>&1 | grep x', 'test_suite_js', 'piped', 'ev1'),
        run('npx vitest run | grep y', 'test_suite_js', 'piped', 'ev2'),
        run('pytest | tee log', 'test_suite_py', 'piped', 'ev3'),
        // a non-unknown run must be excluded
        { ...run('cargo test', 'test_suite_rust', 'exit', 'ev4'), status: 'passed' },
      ],
      contentTailByEventId: {
        ev1: 'Tests  3 passed (3)',
        ev2: '',
        ev3: '===== 5 passed in 1s =====',
      },
    };
    const rows = collectUnknownVerification(input);
    // test_suite_js/piped has 2, test_suite_py/piped has 1; passed run excluded.
    expect(rows.length).toBe(2);
    expect(rows[0].count).toBe(2); // js group first (count desc)
    expect(rows[0].commandKind).toBe('test_suite_js');
    expect(rows[0].statusBasis).toBe('piped');
    expect(rows[0].sampleContentTail).toBe('Tests  3 passed (3)');
    expect(rows[0].hint).toMatch(/looks_like_success|looks_like_failure/);
    // no unknown runs left out
    expect(rows.every((r) => r.count > 0)).toBe(true);
  });
});
