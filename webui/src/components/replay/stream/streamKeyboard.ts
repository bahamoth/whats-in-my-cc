// S10 (UX 재설계 §7.4) — pure keyboard-navigation helpers for the conversation
// stream. Kept out of the component so the (heterogeneous, virtualised) item →
// selectable-event mapping is unit-testable. The component wires j/k/e to
// `onSelect`; the existing scroll-into-view effect brings the selection into
// view, so these helpers never touch the DOM.

import type { StreamItem } from './streamModel';

/** Statuses that mark a synthetic end-card / group as a failure. */
const FAILED_STATUS = new Set(['failed', 'killed', 'error']);

/** The single representative event id used to SELECT an item via j/k. Returns
 *  null for items that carry no jumpable event (e.g. a group whose dispatch
 *  call is outside the loaded window). */
export function primaryEventId(item: StreamItem): string | null {
  switch (item.type) {
    case 'message':
      return item.eventId ?? null;
    case 'thinking':
      return item.events[0]?.eventId ?? null;
    case 'activity-run':
      return item.events[0]?.event.event_id ?? null;
    case 'scaffold-group':
      return item.items[0]?.eventId ?? null;
    case 'sidechain-group':
      return item.taskEventId ?? item.notificationEventId ?? null;
    case 'batch-group':
      return item.agentGroups[0]?.taskEventId ?? null;
    case 'workflow-group':
      return item.taskEventId ?? item.notificationEventId ?? null;
    case 'subagent-end':
      return item.notificationEventId ?? null;
    case 'workflow-end':
      return item.notificationEventId ?? null;
    default:
      return null;
  }
}

/** Ordered list of selectable event ids — the j/k navigation spine. */
export function spineEventIds(items: StreamItem[]): string[] {
  return items.map(primaryEventId).filter((id): id is string => !!id);
}

/** Move the selection one step up/down the spine. Clamps at the ends (no wrap);
 *  with no current selection, `down` picks the first id and `up` the last. */
export function stepEventId(
  items: StreamItem[],
  current: string | null,
  dir: 'up' | 'down',
): string | null {
  const ids = spineEventIds(items);
  if (ids.length === 0) return null;
  if (current == null) return dir === 'down' ? ids[0] : ids[ids.length - 1];
  const i = ids.indexOf(current);
  if (i < 0) return dir === 'down' ? ids[0] : ids[ids.length - 1];
  const next = dir === 'down' ? i + 1 : i - 1;
  if (next < 0 || next >= ids.length) return current; // clamp
  return ids[next];
}

/** Does this item carry an error worth jumping to (e key)? */
function itemError(item: StreamItem): string | null {
  switch (item.type) {
    case 'activity-run': {
      const failed = item.events.find((e) => e.result?.isError);
      return failed ? failed.event.event_id : null;
    }
    case 'subagent-end':
      return item.status && FAILED_STATUS.has(item.status)
        ? item.notificationEventId ?? null
        : null;
    case 'workflow-end':
      return FAILED_STATUS.has(item.status) ? item.notificationEventId ?? null : null;
    case 'sidechain-group':
      return item.endStatus && FAILED_STATUS.has(item.endStatus)
        ? item.notificationEventId ?? null
        : null;
    default:
      return null;
  }
}

/** The next error event id strictly after the current selection (e key), or
 *  null when none follows. Position is taken from the spine order. */
export function nextErrorEventId(items: StreamItem[], current: string | null): string | null {
  const ids = spineEventIds(items);
  const start = current == null ? -1 : ids.indexOf(current);
  for (let k = 0; k < items.length; k++) {
    const pid = primaryEventId(items[k]);
    const pos = pid ? ids.indexOf(pid) : -1;
    if (pos <= start) continue;
    const err = itemError(items[k]);
    if (err) return err;
  }
  return null;
}
