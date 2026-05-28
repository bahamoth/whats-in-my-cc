/**
 * PR-3 — Episode phase strip. A horizontal stacked bar whose segments are
 * proportional to each episode's wall-clock duration. Segments expose
 * `data-state="<phase>"` so the design tokens drive colour.
 *
 * No chart library yet — d3-scale lands in PR-4 when waterfall needs it.
 * For now a tested div+CSS implementation gives us reliable behaviour and
 * predictable jsdom snapshots.
 */
import { useState } from 'react';
import type { EpisodeDto } from '../../api/types';
import styles from './EpisodeStrip.module.css';

interface EpisodeStripProps {
  episodes: EpisodeDto[];
  onZoomTo?: (startISO: string, endISO: string) => void;
}

function durationMs(e: EpisodeDto): number {
  const start = Date.parse(e.started_at);
  const end = Date.parse(e.ended_at);
  if (Number.isNaN(start) || Number.isNaN(end) || end <= start) return 1;
  return end - start;
}

export function EpisodeStrip({ episodes, onZoomTo }: EpisodeStripProps) {
  const [hoveredId, setHoveredId] = useState<string | null>(null);

  if (episodes.length === 0) {
    return <div className={styles.empty} aria-hidden="true" />;
  }

  const totalMs = episodes.reduce((acc, e) => acc + durationMs(e), 0) || 1;
  const hovered = hoveredId ? episodes.find((e) => e.episode_id === hoveredId) ?? null : null;

  return (
    <div className={styles.strip} role="img" aria-label={`${episodes.length} episode phases`}>
      <div className={styles.bar}>
        {episodes.map((e) => {
          const widthPct = (durationMs(e) / totalMs) * 100;
          return (
            <button
              type="button"
              key={e.episode_id}
              data-testid={`episode-segment-${e.episode_id}`}
              data-state={e.phase}
              data-width-pct={widthPct.toFixed(4)}
              className={styles.segment}
              style={{ width: `${widthPct}%` }}
              onMouseEnter={() => setHoveredId(e.episode_id)}
              onMouseLeave={() => setHoveredId((cur) => (cur === e.episode_id ? null : cur))}
              onClick={() => onZoomTo?.(e.started_at, e.ended_at)}
              aria-label={`Episode ${e.episode_id} — phase ${e.phase}`}
            />
          );
        })}
      </div>
      {hovered && (
        <div role="tooltip" className={styles.tooltip}>
          <strong>{hovered.phase}</strong>
          {hovered.summary ? ` — ${hovered.summary}` : ''}
          <span className={styles.tooltipSubtle}>
            {' · '}
            {Math.max(1, Math.round(durationMs(hovered) / 1000))}s
          </span>
        </div>
      )}
    </div>
  );
}
