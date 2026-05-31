// Scroll-anchoring decision for the conversation stream, extracted as pure
// functions so the "follow live appends only while parked at the tip; never let
// measurement-driven scrolls re-engage autoscroll" rule is unit-testable
// (regression guard for the "can't focus while streaming" bug).

export const STICK_THRESHOLD = 48;
const GESTURE_WINDOW_MS = 200;

export interface ScrollMetrics {
  scrollHeight: number;
  scrollTop: number;
  clientHeight: number;
}

/** True when the viewport is parked at (near) the bottom — the zone where the
 *  stream should follow live appends. */
export function isNearBottom(m: ScrollMetrics, threshold = STICK_THRESHOLD): boolean {
  return m.scrollHeight - m.scrollTop - m.clientHeight < threshold;
}

/** The next "stick to bottom" value for a scroll event, or `null` when the
 *  scroll should be IGNORED (it was not a genuine user gesture — e.g. the
 *  virtualizer's measurement pass — so it must not flip the stick decision and
 *  yank the viewport). `lastUserScrollMs` is the timestamp of the last
 *  wheel/pointer/key gesture; a scroll within `gestureWindowMs` of it counts as
 *  user-driven. */
export function nextStickState(
  nowMs: number,
  lastUserScrollMs: number,
  m: ScrollMetrics,
  opts?: { gestureWindowMs?: number; threshold?: number },
): boolean | null {
  const gestureWindow = opts?.gestureWindowMs ?? GESTURE_WINDOW_MS;
  if (nowMs - lastUserScrollMs > gestureWindow) return null; // measurement/programmatic → ignore
  return isNearBottom(m, opts?.threshold);
}
