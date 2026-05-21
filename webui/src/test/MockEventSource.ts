// Slice-8 — test helper for vitest. Replaces global EventSource with a
// controllable mock so tests can dispatch frames synchronously and assert
// state transitions without real network I/O.
//
// Usage:
//   MockEventSource.install();
//   render(<Component />);
//   const es = MockEventSource.latest()!;
//   act(() => es.emit('message', JSON.stringify(envelope)));

type Listener = (ev: { data: string; lastEventId?: string }) => void;
type ErrorListener = (ev: Event) => void;

export class MockEventSource {
  static instances: MockEventSource[] = [];

  url: string;
  readyState = 0; // CONNECTING
  onmessage: Listener | null = null;
  onerror: ErrorListener | null = null;
  onopen: ((ev: Event) => void) | null = null;
  private namedListeners: Map<string, Listener[]> = new Map();

  constructor(url: string) {
    this.url = url;
    MockEventSource.instances.push(this);
    queueMicrotask(() => {
      this.readyState = 1; // OPEN
      this.onopen?.(new Event('open'));
    });
  }

  addEventListener(name: string, fn: Listener): void {
    const arr = this.namedListeners.get(name) ?? [];
    arr.push(fn);
    this.namedListeners.set(name, arr);
  }

  removeEventListener(name: string, fn: Listener): void {
    const arr = this.namedListeners.get(name);
    if (!arr) return;
    const i = arr.indexOf(fn);
    if (i >= 0) arr.splice(i, 1);
  }

  /** Dispatch an SSE frame to listeners. `eventName='message'` triggers `onmessage`. */
  emit(eventName: string, data: string, lastEventId?: string): void {
    const ev = { data, lastEventId };
    if (eventName === 'message') this.onmessage?.(ev);
    this.namedListeners.get(eventName)?.forEach((fn) => fn(ev));
  }

  emitError(): void {
    this.onerror?.(new Event('error'));
  }

  close(): void {
    this.readyState = 2; // CLOSED
  }

  static install(): void {
    (globalThis as unknown as { EventSource: typeof MockEventSource }).EventSource =
      MockEventSource;
    MockEventSource.instances = [];
  }

  static latest(): MockEventSource | undefined {
    return MockEventSource.instances[MockEventSource.instances.length - 1];
  }
}
