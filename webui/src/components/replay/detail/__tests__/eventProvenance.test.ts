import { describe, expect, test } from 'vitest';
import { eventProvenance } from '../eventProvenance';

describe('eventProvenance', () => {
  test('native vs derived', () => {
    expect(eventProvenance('tool_call').kind).toBe('native');
    expect(eventProvenance('assistant_message').kind).toBe('native');
    expect(eventProvenance('diff_hunk').kind).toBe('derived');
    expect(eventProvenance('verification_run').kind).toBe('derived');
  });

  test('signal is derived', () => {
    expect(eventProvenance('signal').kind).toBe('derived');
  });

  // l10n — the badge label moved to the catalog (detail.provenance.*); this
  // module now only decides native vs derived. The localized label text is
  // asserted in the InsightTab component test instead.

  test('user_message is native', () => {
    expect(eventProvenance('user_message').kind).toBe('native');
  });

  test('thinking is native', () => {
    expect(eventProvenance('thinking').kind).toBe('native');
  });

  test('unknown kind defaults to native', () => {
    expect(eventProvenance('some_unknown_kind').kind).toBe('native');
  });
});
