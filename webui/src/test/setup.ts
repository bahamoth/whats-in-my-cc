// Slice-8 — global vitest setup. jsdom does not provide EventSource;
// every test that mounts a component using `useLiveStream` (directly or
// transitively) needs a controllable mock. Installing it globally keeps
// existing tests passing without per-file installation.
import '@testing-library/jest-dom/vitest';
import { cleanup } from '@testing-library/react';
import { afterEach } from 'vitest';
import { MockEventSource } from './MockEventSource';

// vitest.config.ts uses `globals: false`, so RTL's auto-cleanup hook
// is not registered automatically. Register it here so every test gets
// a clean DOM (otherwise multi-render assertions fail with "found N").
afterEach(() => {
  cleanup();
});

if (typeof (globalThis as unknown as { EventSource?: unknown }).EventSource === 'undefined') {
  MockEventSource.install();
}

// Slice-9 — jsdom does not ship IntersectionObserver. SessionDetailPage
// uses it for scroll-back paging; install a no-op default so existing tests
// that don't override it still get a constructable shim.
if (typeof (globalThis as unknown as { IntersectionObserver?: unknown }).IntersectionObserver === 'undefined') {
  class NoopIO {
    constructor(_cb: IntersectionObserverCallback) {}
    observe() {}
    disconnect() {}
    unobserve() {}
    root = null;
    rootMargin = '';
    thresholds: ReadonlyArray<number> = [];
    takeRecords(): IntersectionObserverEntry[] { return []; }
  }
  (globalThis as Record<string, unknown>).IntersectionObserver = NoopIO;
}

// PR-7 — React Flow uses ResizeObserver. jsdom does not implement it; a
// no-op shim is enough since we never assert layout dimensions in tests.
if (typeof (globalThis as unknown as { ResizeObserver?: unknown }).ResizeObserver === 'undefined') {
  class NoopRO {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  (globalThis as Record<string, unknown>).ResizeObserver = NoopRO;
}

// PR-7 — React Flow also reads `window.matchMedia` for prefers-reduced-motion.
// Provide a default mock that reports "no match" so tests behave like a
// normal-motion environment.
if (typeof window !== 'undefined' && !window.matchMedia) {
  window.matchMedia = ((q: string) => ({
    matches: false,
    media: q,
    onchange: null,
    addListener: () => {},
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => false,
  })) as typeof window.matchMedia;
}
