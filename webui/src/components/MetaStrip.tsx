import type { SessionDetail } from '../api/types';

export function MetaStrip({ session }: { session: SessionDetail }) {
  const turns = new Set(
    session.events
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
