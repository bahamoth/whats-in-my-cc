// Slice-8 — sessionStorage cursor for SSE Last-Event-ID resume across F5 reload.
//
// Scope strings:
//   'global'      — used by SessionListPage subscribing to /v1/stream
//   '<sessionId>' — used by SessionDetailPage subscribing to /v1/stream?session=<id>
//
// sessionStorage clears on tab close but survives reload, which matches the
// user expectation that F5 should feel continuous and reopening the app
// should start fresh.

const KEY_PREFIX = 'witmcc:cursor:';

export function readCursor(scope: string): string | null {
  return sessionStorage.getItem(KEY_PREFIX + scope);
}

export function writeCursor(scope: string, eventId: string): void {
  sessionStorage.setItem(KEY_PREFIX + scope, eventId);
}

export function clearCursor(scope: string): void {
  sessionStorage.removeItem(KEY_PREFIX + scope);
}
