// Normalized view over a `hook_event` ObservedEvent payload.
//
// Field meaning is real-data-anchored (see hookFacet.test.ts +
// `__tests__/fixtures/hook_success.real.json`): a `hook_success` record carries
// `exitCode` / `durationMs` / `command` / `hookEvent` / `hookName` (100% present
// across PreToolUse, PostToolUse, Stop, SessionStart in the live dev DB). A
// `hook_additional_context` record has none of the exec fields → all null.
export interface HookFacet {
  hookEvent: string | null;
  command: string | null;
  exitCode: number | null;
  durationMs: number | null;
  /** exitCode === 0 → true; non-zero → false; absent → null (unknown). */
  success: boolean | null;
  stdout: string;
  stderr: string;
}

function asObj(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' ? (v as Record<string, unknown>) : {};
}

function numOrNull(v: unknown): number | null {
  return typeof v === 'number' ? v : null;
}

function strOrEmpty(v: unknown): string {
  return typeof v === 'string' ? v : '';
}

export function hookFacet(payload: unknown): HookFacet {
  const p = asObj(payload);
  const exitCode = numOrNull(p.exitCode);
  return {
    hookEvent: typeof p.hookEvent === 'string' ? p.hookEvent : null,
    command: typeof p.command === 'string' ? p.command : null,
    exitCode,
    durationMs: numOrNull(p.durationMs),
    success: exitCode == null ? null : exitCode === 0,
    stdout: strOrEmpty(p.stdout),
    stderr: strOrEmpty(p.stderr),
  };
}
