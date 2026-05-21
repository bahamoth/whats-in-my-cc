// Slice-8 — global vitest setup. jsdom does not provide EventSource;
// every test that mounts a component using `useLiveStream` (directly or
// transitively) needs a controllable mock. Installing it globally keeps
// existing tests passing without per-file installation.
import { MockEventSource } from './MockEventSource';

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
