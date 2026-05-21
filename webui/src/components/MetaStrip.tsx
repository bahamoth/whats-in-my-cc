import type { ObservedEventDto, SessionDetail } from '../api/types';

/** Slice-9 — `events` was inlined on SessionDetail and used here just to
 *  derive a turn count. Now SessionDetailPage owns the event window
 *  separately (useSessionWindow); pass that array in as `events`. */
export function MetaStrip({
  session,
  events,
}: {
  session: SessionDetail;
  events: ObservedEventDto[];
}) {
  const turns = new Set(
    events
      .map((e) => e.turn_id)
      .filter((t): t is string => Boolean(t)),
  ).size;
  return (
    <div>
      <strong>{session.summary.event_count} events</strong>
      {turns > 0 && <> · {turns} turns</>}
      {' · '}
      {session.summary.first_observed_at} → {session.summary.last_observed_at}
    </div>
  );
}
