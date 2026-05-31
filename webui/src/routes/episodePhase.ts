export type EpisodeLike = {
  phase: string;
  started_at: string;
  ended_at: string;
  episode_id?: string;
};

/**
 * Pick the MOST SPECIFIC episode covering instant `t` and return its phase.
 *
 * Episodes can OVERLAP — e.g. a stale/wide episode and a narrow accurate one
 * spanning the same instant. A naive first-match (`.find`) returns whichever
 * comes first in the array, typically the widest/earliest-starting (stale) one,
 * giving an event the WRONG phase badge (e.g. a `Read` getting a wide `action`
 * episode's badge). To resolve this deterministically we pick, among all
 * episodes covering `t`, the narrowest by duration.
 *
 * Selection (deterministic):
 *   1. NARROWEST: smallest `(Date(ended_at) - Date(started_at))`.
 *   2. tie → LATEST `started_at` (string compare; ISO-8601 sorts correctly).
 *   3. tie → smallest `episode_id` lexicographically (falls back to `phase`
 *      when no `episode_id`), so the result is fully deterministic.
 *
 * Containment uses inclusive string compare on `started_at`/`ended_at`
 * (ISO-8601 timestamps compare correctly as strings). DURATION uses
 * `Date.parse` because lexicographic order is not duration order.
 *
 * Slice 3 B3; design spec
 * `docs/superpowers/specs/2026-05-31-episode-redesign-slice3-design.md` §B3.
 */
export function phaseAt(episodes: EpisodeLike[], t: string): string | null {
  let best: EpisodeLike | null = null;
  let bestDur = Number.POSITIVE_INFINITY;

  for (const e of episodes) {
    // Inclusive containment; ISO-8601 strings compare correctly.
    if (!(e.started_at <= t && t <= e.ended_at)) continue;

    const dur = Date.parse(e.ended_at) - Date.parse(e.started_at);
    if (best === null) {
      best = e;
      bestDur = dur;
      continue;
    }

    // 1. narrowest duration wins.
    if (dur < bestDur) {
      best = e;
      bestDur = dur;
      continue;
    }
    if (dur > bestDur) continue;

    // 2. duration tie → latest started_at wins.
    if (e.started_at > best.started_at) {
      best = e;
      continue;
    }
    if (e.started_at < best.started_at) continue;

    // 3. started_at tie → smallest episode_id (then phase) lexicographically.
    const eKey = e.episode_id ?? e.phase;
    const bestKey = best.episode_id ?? best.phase;
    if (eKey < bestKey) best = e;
  }

  return best ? best.phase : null;
}
