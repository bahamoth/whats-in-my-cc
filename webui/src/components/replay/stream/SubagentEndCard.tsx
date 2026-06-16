// webui/src/components/replay/stream/SubagentEndCard.tsx
// The compact "end card" closing a background subagent's hairline rail: shows
// when it finished, how long it ran, message/tool counts, and its conclusion —
// so the rail reads request (start card) → … → result (this end card) without
// expanding anything. When the subagent was Agent run_in_background, a matched
// <task-notification> gives a deterministic status (completed/failed/killed) +
// a jump to the notification 원문. A synthetic row (no source event); see
// SubagentEndCard in streamModel + insertSubagentEndCards/syncTaskNotifications.
import type { KeyboardEvent } from 'react';
import { CheckCircle2, Clock, MessageSquare, Wrench, Bell } from 'lucide-react';
import type { SubagentEndCard as EndCard } from './streamModel';
import { formatDuration, durationHeat } from './duration';
import { endStatusLabel } from './endStatus';
import { useT } from '../../../i18n';
import styles from './SubagentEndCard.module.css';

function timeLabel(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const p = (n: number) => String(n).padStart(2, '0');
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

interface Props {
  card: EndCard;
  onSelect?: (eventId: string) => void;
  selected?: boolean;
}

export function SubagentEndCard({ card, onSelect, selected = false }: Props) {
  const t = useT();
  const status = card.status ? endStatusLabel(card.status) : null;
  const selectable = !!(card.notificationEventId && onSelect);
  return (
    <div
      data-testid="subagent-end-card"
      className={styles.card}
      data-status={status?.kind ?? 'done'}
      data-selected={String(selected)}
      style={{ ['--agentColor' as string]: card.color }}
      {...(selectable
        ? {
            role: 'button',
            tabIndex: 0,
            onClick: () => onSelect!(card.notificationEventId!),
            onKeyDown: (e: KeyboardEvent) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                onSelect!(card.notificationEventId!);
              }
            },
          }
        : {})}
    >
      <div className={styles.head}>
        <CheckCircle2 size={13} aria-hidden className={styles.check} />
        <span className={styles.label}>{t('stream.endCard.label')}</span>
        <span className={styles.time}>{timeLabel(card.endTimestamp)}</span>
        {status && (
          <span data-testid="subagent-end-status" className={styles.statusPill}>
            {status.text}
          </span>
        )}
        <span className={styles.stats}>
          <span data-heat={durationHeat(card.durationMs)}>
            <Clock size={11} aria-hidden /> {formatDuration(card.durationMs)}
          </span>
          <span>
            <MessageSquare size={11} aria-hidden /> {card.messageCount}
          </span>
          <span>
            <Wrench size={11} aria-hidden /> {card.toolCount}
          </span>
          {card.notificationEventId && onSelect && (
            <button
              data-testid="subagent-end-jump"
              className={styles.jump}
              title={t('stream.endCard.jumpToNotification')}
              onClick={(e) => {
                e.stopPropagation();
                onSelect(card.notificationEventId!);
              }}
            >
              <Bell size={10} aria-hidden /> {t('stream.notification')}
            </button>
          )}
        </span>
      </div>
      <div className={styles.concl}>
        <span className={styles.conclLabel}>{t('stream.conclusion')}</span>
        {card.conclusion}
      </div>
    </div>
  );
}
