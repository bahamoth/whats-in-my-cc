// Slice-8 — global vitest setup. jsdom does not provide EventSource;
// every test that mounts a component using `useLiveStream` (directly or
// transitively) needs a controllable mock. Installing it globally keeps
// existing tests passing without per-file installation.
import { MockEventSource } from './MockEventSource';

if (typeof (globalThis as unknown as { EventSource?: unknown }).EventSource === 'undefined') {
  MockEventSource.install();
}
