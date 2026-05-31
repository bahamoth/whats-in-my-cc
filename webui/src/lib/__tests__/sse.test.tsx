/**
 * PR-2 RED — SSE → React Query cache bridge.
 *
 * `useLiveStreamBridge` opens the existing /v1/stream EventSource via the
 * already-tested `useLiveStream` hook and translates frames into cache
 * invalidations on the session-scoped query keys. Behaviour we lock in:
 *
 *  - Every envelope debounce-invalidates the graph query (graph changes
 *    are too small to refetch per-envelope; we coalesce).
 *  - `event: gap` invalidates events + graph.
 *  - `event: resync` invalidates EVERY session-scoped query (summary,
 *    graph, events, findings, verification, diff-hunks).
 *
 * The backend does NOT emit `graph.updated` / `finding.generated` named
 * events — those were assumed in the original plan and removed during
 * PR-2's revision (see plan §10.1 PR-2).
 */
import { describe, expect, it, vi, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createQueryClient } from '../queryClient';
import { sessionKeys } from '../queries';
import { useLiveStreamBridge } from '../sse';
import { MockEventSource } from '../../test/MockEventSource';

afterEach(() => {
  vi.useRealTimers();
});

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

  it('coalesces graph invalidations across rapid envelopes', () => {
    vi.useFakeTimers();
    MockEventSource.install();
    const qc = createQueryClient();
    const spy = vi.spyOn(qc, 'invalidateQueries');
    renderHook(() => useLiveStreamBridge('SES-B', { client: qc }), { wrapper: wrap(qc) });
    const es = MockEventSource.latest()!;
    const env = JSON.stringify({
      schema_version: 'v1',
      session_id: 'SES-B',
      event_id: 'evt-1',
      kind: 'tool_call',
      source_type: 'transcript',
      observed_at: '2026-05-29T00:00:00Z',
    });
    act(() => {
      es.emit('message', env);
      es.emit('message', env);
      es.emit('message', env);
    });
    // Pre-flush: should not have fired yet (debounced).
    expect(spy).not.toHaveBeenCalledWith({ queryKey: sessionKeys.graph('SES-B') });
    act(() => {
      vi.advanceTimersByTime(2000);
    });
    const graphCalls = spy.mock.calls.filter(
      (c) => JSON.stringify(c[0]) === JSON.stringify({ queryKey: sessionKeys.graph('SES-B') }),
    );
    expect(graphCalls.length).toBe(1);
  });

  it('on `gap` invalidates events + graph immediately', () => {
    MockEventSource.install();
    const qc = createQueryClient();
    const spy = vi.spyOn(qc, 'invalidateQueries');
    renderHook(() => useLiveStreamBridge('SES-C', { client: qc }), { wrapper: wrap(qc) });
    const es = MockEventSource.latest()!;
    act(() => {
      es.emit('gap', JSON.stringify({ dropped: 5 }));
    });
    const keys = spy.mock.calls.map((c) => JSON.stringify(c[0]));
    expect(keys).toContain(JSON.stringify({ queryKey: sessionKeys.graph('SES-C') }));
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
