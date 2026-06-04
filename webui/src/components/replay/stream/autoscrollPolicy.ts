// Pure decision logic for the explicit autoscroll (stick-to-bottom) model that
// replaces react-virtual's implicit followOnAppend + the hand-rolled 2s
// bottom-pin. Kept pure (no DOM, no React) so the policy is unit-testable;
// useAutoscroll wires it to the scroll element and is verified by browser smoke.

/** Distance (px) from the bottom within which the viewport counts as "at the
 *  tip", so live appends keep following and the autoscroll toggle re-engages. */
export const DEFAULT_BOTTOM_THRESHOLD = 80;

export interface ScrollMetrics {
  scrollTop: number;
  scrollHeight: number;
  clientHeight: number;
}

/** True when the viewport is within `threshold` px of the bottom (or the
 *  content does not overflow at all). */
export function isAtBottom(m: ScrollMetrics, threshold = DEFAULT_BOTTOM_THRESHOLD): boolean {
  return m.scrollHeight - m.scrollTop - m.clientHeight <= threshold;
}

/** A lightweight fingerprint of the rendered item list: the first/last stable
 *  item ids plus the count. Comparing two of these tells us whether the change
 *  was an append at the tip (newest), a prepend of older history, or both. */
export interface ItemsSignature {
  first: string | null;
  last: string | null;
  count: number;
}

export interface ItemsChange {
  /** How many items were appended at the tip (newest end). 0 for a pure
   *  prepend / no change. Drives the OFF-state "N new" badge. */
  appended: number;
  /** Whether older history was prepended at the top (first id changed). The
   *  caller ignores prepends for autoscroll — react-virtual's anchorTo:'end'
   *  keeps the viewport stable across them. */
  prepended: boolean;
}

/** Classify an items-list change by comparing the previous and next
 *  signatures. `prev === null` (initial mount) reports no movement; the hook
 *  handles the first paint's scroll-to-bottom separately.
 *
 *  Append (loadNewer / SSE) and prepend (loadOlder) arrive as separate state
 *  updates in this app, so the common cases are a pure append (last id
 *  changed, first unchanged) or a pure prepend (first id changed, last
 *  unchanged). When both ends change (e.g. an LRU trim alongside an append),
 *  we still report the tip growth via the count delta and flag the prepend. */
export function classifyChange(prev: ItemsSignature | null, next: ItemsSignature): ItemsChange {
  if (!prev) return { appended: 0, prepended: false };
  const prepended = next.first !== prev.first && next.first !== null;
  const appendedAtTip = next.last !== prev.last;
  const delta = next.count - prev.count;
  const appended = appendedAtTip && delta > 0 ? delta : 0;
  return { appended, prepended };
}
