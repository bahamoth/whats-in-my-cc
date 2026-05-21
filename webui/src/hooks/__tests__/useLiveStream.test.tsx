import { describe, it, expect, beforeEach, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { MockEventSource } from '../../test/MockEventSource';
import { useLiveStream, type LiveEnvelope } from '../useLiveStream';

function sampleEnv(overrides: Partial<LiveEnvelope> = {}): LiveEnvelope {
  return {
    schema_version: '1',
    session_id: 's',
    event_id: '01HZZZ000000000000000000A',
    kind: 'user_message',
    source_type: 'transcript',
    observed_at: '2026-05-21T10:00:00Z',
    ...overrides,
  };
}

beforeEach(() => {
  MockEventSource.install();
  sessionStorage.clear();
});

describe('useLiveStream', () => {
  it('opens EventSource with the passed URL on mount when no cursor', () => {
    renderHook(() =>
      useLiveStream({ url: '/v1/stream', scope: 'global', onEnvelope: () => {} }),
    );
    expect(MockEventSource.latest()?.url).toBe('/v1/stream');
  });

  it('appends ?last_event_id= when cursor present', () => {
    sessionStorage.setItem('witmcc:cursor:global', '01HZZ');
    renderHook(() =>
      useLiveStream({ url: '/v1/stream', scope: 'global', onEnvelope: () => {} }),
    );
    expect(MockEventSource.latest()?.url).toBe('/v1/stream?last_event_id=01HZZ');
  });

  it('appends &last_event_id= when URL already has a query', () => {
    sessionStorage.setItem('witmcc:cursor:sess-1', '01HZZ');
    renderHook(() =>
      useLiveStream({
        url: '/v1/stream?session=sess-1',
        scope: 'sess-1',
        onEnvelope: () => {},
      }),
    );
    expect(MockEventSource.latest()?.url).toBe('/v1/stream?session=sess-1&last_event_id=01HZZ');
  });

  it('writes received event_id to sessionStorage and calls onEnvelope', () => {
    const onEnvelope = vi.fn();
    renderHook(() => useLiveStream({ url: '/v1/stream', scope: 'global', onEnvelope }));
    const es = MockEventSource.latest()!;
    act(() => es.emit('message', JSON.stringify(sampleEnv({ event_id: 'A' }))));
    expect(sessionStorage.getItem('witmcc:cursor:global')).toBe('A');
    expect(onEnvelope).toHaveBeenCalledOnce();
    expect(onEnvelope.mock.calls[0][0].event_id).toBe('A');
  });

  it('invokes onGap on event: gap and keeps cursor', () => {
    sessionStorage.setItem('witmcc:cursor:global', 'B');
    const onGap = vi.fn();
    renderHook(() =>
      useLiveStream({ url: '/v1/stream', scope: 'global', onEnvelope: () => {}, onGap }),
    );
    const es = MockEventSource.latest()!;
    act(() => es.emit('gap', JSON.stringify({ dropped: 5 })));
    expect(onGap).toHaveBeenCalledWith({ dropped: 5 });
    expect(sessionStorage.getItem('witmcc:cursor:global')).toBe('B');
  });

  it('invokes onResync on event: resync and clears cursor', () => {
    sessionStorage.setItem('witmcc:cursor:global', 'B');
    const onResync = vi.fn();
    renderHook(() =>
      useLiveStream({ url: '/v1/stream', scope: 'global', onEnvelope: () => {}, onResync }),
    );
    const es = MockEventSource.latest()!;
    act(() => es.emit('resync', JSON.stringify({ reason: 'unknown_cursor' })));
    expect(onResync).toHaveBeenCalledWith({ reason: 'unknown_cursor' });
    expect(sessionStorage.getItem('witmcc:cursor:global')).toBeNull();
  });

  it('closes EventSource on unmount', () => {
    const { unmount } = renderHook(() =>
      useLiveStream({ url: '/v1/stream', scope: 'global', onEnvelope: () => {} }),
    );
    const es = MockEventSource.latest()!;
    expect(es.readyState).not.toBe(2);
    unmount();
    expect(es.readyState).toBe(2);
  });

  it('ignores malformed JSON in message data', () => {
    const onEnvelope = vi.fn();
    renderHook(() => useLiveStream({ url: '/v1/stream', scope: 'global', onEnvelope }));
    const es = MockEventSource.latest()!;
    act(() => es.emit('message', 'not json'));
    expect(onEnvelope).not.toHaveBeenCalled();
  });
});
