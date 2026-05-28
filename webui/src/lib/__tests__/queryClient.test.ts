/**
 * PR-1 RED — TanStack Query client defaults that lock in regression-safe
 * behaviour for every subsequent PR:
 *  - non-zero staleTime so panels do not refetch on every focus
 *  - retry disabled on 4xx so a 401 from --auth on never floods the server
 *  - retry disabled on 410 Gone so retention-swept sessions surface cleanly
 *
 * See plan §10.1 PR-1.
 */
import { describe, expect, it } from 'vitest';
import { createQueryClient } from '../queryClient';

describe('createQueryClient', () => {
  it('returns a configured QueryClient instance', () => {
    const qc = createQueryClient();
    expect(qc).toBeDefined();
    // Smoke check that it behaves like a TanStack Query client.
    expect(typeof qc.getQueryCache).toBe('function');
    expect(typeof qc.setQueryData).toBe('function');
  });

  it('uses a non-zero staleTime so views do not thrash refetch', () => {
    const qc = createQueryClient();
    const defaults = qc.getDefaultOptions();
    const stale = defaults.queries?.staleTime;
    expect(typeof stale).toBe('number');
    expect(stale as number).toBeGreaterThan(0);
  });

  it('disables retry for client errors (4xx) so auth failures stop fast', () => {
    const qc = createQueryClient();
    const retry = qc.getDefaultOptions().queries?.retry;
    expect(retry).toBeTypeOf('function');
    const fn = retry as (failureCount: number, error: unknown) => boolean;

    const err401 = Object.assign(new Error('unauthorized'), { status: 401 });
    const err410 = Object.assign(new Error('gone'), { status: 410 });
    const err500 = Object.assign(new Error('boom'), { status: 500 });

    expect(fn(0, err401)).toBe(false);
    expect(fn(0, err410)).toBe(false);
    // 5xx is allowed to retry a small number of times.
    expect(fn(0, err500)).toBe(true);
    expect(fn(5, err500)).toBe(false);
  });

  it('disables window-focus refetch (read-only viewer, no annotation)', () => {
    const qc = createQueryClient();
    const defaults = qc.getDefaultOptions();
    expect(defaults.queries?.refetchOnWindowFocus).toBe(false);
  });
});
