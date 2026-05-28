/**
 * PR-8 RED — useMediaQuery wraps `window.matchMedia` with React state so
 * components can react to viewport changes without re-implementing the
 * subscribe/unsubscribe dance.
 */
import { describe, expect, it, beforeEach, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useMediaQuery } from '../useMediaQuery';

let listeners: Array<(ev: MediaQueryListEvent) => void> = [];
let currentMatches = false;

function installMatchMedia() {
  listeners = [];
  window.matchMedia = ((query: string) => ({
    matches: currentMatches,
    media: query,
    addEventListener: (_: string, fn: (ev: MediaQueryListEvent) => void) => {
      listeners.push(fn);
    },
    removeEventListener: (_: string, fn: (ev: MediaQueryListEvent) => void) => {
      const i = listeners.indexOf(fn);
      if (i >= 0) listeners.splice(i, 1);
    },
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: () => false,
    onchange: null,
  })) as unknown as typeof window.matchMedia;
}

beforeEach(() => {
  installMatchMedia();
  currentMatches = false;
});

describe('useMediaQuery', () => {
  it('returns the initial match state', () => {
    currentMatches = true;
    const { result } = renderHook(() => useMediaQuery('(min-width: 1400px)'));
    expect(result.current).toBe(true);
  });

  it('returns false when the query does not match initially', () => {
    currentMatches = false;
    const { result } = renderHook(() => useMediaQuery('(max-width: 640px)'));
    expect(result.current).toBe(false);
  });

  it('updates when matchMedia fires a change', () => {
    const { result } = renderHook(() => useMediaQuery('(max-width: 900px)'));
    expect(result.current).toBe(false);
    act(() => {
      listeners.forEach((fn) =>
        fn({ matches: true, media: '(max-width: 900px)' } as MediaQueryListEvent),
      );
    });
    expect(result.current).toBe(true);
  });
});
