/**
 * PR-2 RED — SSE → React Query cache bridge.
 *
 * `useLiveStreamBridge` opens the existing /v1/stream EventSource via the
 * already-tested `useLiveStream` hook and translates frames into cache
 * invalidations on the session-scoped query keys. Behaviour we lock in:
 *
 *  - `event: gap` invalidates events.
 *  - `event: resync` invalidates EVERY session-scoped query (session root subtree).
 *
 * The backend does NOT emit named `graph.updated` / `finding.generated` events
 * — those were assumed in the original plan and removed during PR-2's revision
 * (see plan §10.1 PR-2). The graph layer has since been fully removed.
 */
import { describe, expect, it, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createQueryClient } from '../queryClient';
import { sessionKeys } from '../queries';
import { useLiveStreamBridge } from '../sse';
import { MockEventSource } from '../../test/MockEventSource';

function wrap(qc: QueryClient) {
  return ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={qc}>{children}</QueryClientProvider>
  );
}

describe('useLiveStreamBridge', () => {
  it('opens the /v1/stream connection scoped to the session id', () => {
    MockEventSource.install();
    const qc = createQueryClient();
    renderHook(() => useLiveStreamBridge('SES-A'), { wrapper: wrap(qc) });
    const es = MockEventSource.latest()!;
    expect(es.url).toMatch(/\/v1\/stream\?session=SES-A/);
  });

  it('on `gap` invalidates events immediately', () => {
    MockEventSource.install();
    const qc = createQueryClient();
    const spy = vi.spyOn(qc, 'invalidateQueries');
    renderHook(() => useLiveStreamBridge('SES-C', { client: qc }), { wrapper: wrap(qc) });
    const es = MockEventSource.latest()!;
    act(() => {
      es.emit('gap', JSON.stringify({ dropped: 5 }));
    });
    const keys = spy.mock.calls.map((c) => JSON.stringify(c[0]));
    expect(keys).toContain(JSON.stringify({ queryKey: sessionKeys.events('SES-C') }));
  });

  it('on `resync` invalidates the entire session-scoped subtree', () => {
    MockEventSource.install();
    const qc = createQueryClient();
    const spy = vi.spyOn(qc, 'invalidateQueries');
    renderHook(() => useLiveStreamBridge('SES-D', { client: qc }), { wrapper: wrap(qc) });
    const es = MockEventSource.latest()!;
    act(() => {
      es.emit('resync', JSON.stringify({ reason: 'rebuild' }));
    });
    // Expect a single invalidate against the session root prefix.
    const keys = spy.mock.calls.map((c) => JSON.stringify(c[0]));
    expect(keys).toContain(JSON.stringify({ queryKey: sessionKeys.session('SES-D') }));
  });
});
