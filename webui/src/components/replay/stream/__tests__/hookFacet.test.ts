import { describe, it, expect } from 'vitest';
import { hookFacet } from '../hookFacet';
import realHookSuccess from './fixtures/hook_success.real.json';

// Field meaning is anchored to a real captured hook_event payload
// (`hook_success.real.json`, a PreToolUse:Bash hook). The invariant: a
// `hook_success` record carries exitCode / durationMs / command / hookEvent.
// Verified across the live dev DB as a 100% pattern over 4 hookEvent types
// (PreToolUse, PostToolUse, Stop, SessionStart), not a single case.
describe('hookFacet', () => {
  it('extracts success, duration, command from a real hook_success payload', () => {
    const f = hookFacet(realHookSuccess);
    expect(f.success).toBe(true); // exitCode 0
    expect(f.exitCode).toBe(0);
    expect(f.durationMs).toBe(330);
    expect(f.command).toBe('python ${HOME}/.claude/hooks/remove_ai_footer.py');
    expect(f.hookEvent).toBe('PreToolUse');
  });

  it('marks a non-zero exitCode as failure', () => {
    const f = hookFacet({ type: 'hook_success', exitCode: 2, durationMs: 12 });
    expect(f.success).toBe(false);
    expect(f.exitCode).toBe(2);
  });

  it('returns nulls for a hook_additional_context payload (no exec fields)', () => {
    const f = hookFacet({
      type: 'hook_additional_context',
      hookName: 'SessionStart',
      content: ['injected'],
    });
    expect(f.success).toBeNull();
    expect(f.durationMs).toBeNull();
    expect(f.exitCode).toBeNull();
  });
});
