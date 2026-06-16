// webui/src/components/replay/stream/ThinkingMarker.tsx
import { BrainCog, AlertTriangle } from 'lucide-react';
import type { ThinkingMarker as ThinkingMarkerData } from './streamModel';
import { formatDuration, formatTokens } from './llmRequestMetrics';
import { useT } from '../../../i18n';
import styles from './ThinkingMarker.module.css';

interface ThinkingMarkerProps {
  marker: ThinkingMarkerData;
  selectedEventId: string | null;
  onSelect: (eventId: string) => void;
}

export function ThinkingMarker({ marker, selectedEventId, onSelect }: ThinkingMarkerProps) {
  const t = useT();
  // One redacted thinking event == one LLM response (no merging).
  const entry = marker.events[0];
  const m = entry.metrics;
  const selected = entry.eventId === selectedEventId;

  const duration = formatDuration(m?.durationMs ?? null);
  const tokens = formatTokens(m?.outputTokens ?? null);
  // Surface only genuinely abnormal responses inline; full detail on select.
  const warn =
    (m?.attempt != null && m.attempt > 1) ||
    m?.success === false ||
    m?.stopReason === 'max_tokens';

  return (
    <div
      className={styles.wrap}
      data-testid="thinking-marker"
      data-selected={String(selected)}
    >
      <button
        type="button"
        className={styles.line}
        onClick={() => onSelect(entry.eventId)}
        aria-label={t('stream.thinking.aria')}
        title={t('stream.thinking.title')}
      >
        <BrainCog size={13} aria-hidden className={styles.icon} />
        <span className={styles.label}>{t('stream.reasoning')}</span>
        {tokens && (
          <>
            <span className={styles.sep} aria-hidden>·</span>
            <span className={styles.metricMuted}>{tokens} tok</span>
          </>
        )}
        {warn && (
          <AlertTriangle size={12} aria-label={t('stream.thinking.warnAria')} className={styles.warn} />
        )}
        {/* Far right = elapsed time (duration), consistent with action rows. */}
        <span className={styles.duration}>{duration ?? '—'}</span>
      </button>
    </div>
  );
}
