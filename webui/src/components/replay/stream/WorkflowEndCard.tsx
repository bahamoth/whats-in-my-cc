// webui/src/components/replay/stream/WorkflowEndCard.tsx
// A workflow's deterministic completion card, synced from its <task-notification>
// (joined by tool_use_id). Closes the workflow's gutter rail with the authoritative
// status (completed/failed/killed) + summary + a jump to the notification 원문 —
// replacing the heuristic "first message after = synthesis". Synthetic row; see
// WorkflowEndCard in streamModel + syncTaskNotifications.
import type { KeyboardEvent } from 'react';
import { CheckCircle2, Bell } from 'lucide-react';
import type { WorkflowEndCard as EndCard } from './streamModel';
import { endStatusLabel } from './endStatus';
import styles from './WorkflowEndCard.module.css';

function timeLabel(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const p = (n: number) => String(n).padStart(2, '0');
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

interface Props {
  card: EndCard;
  onSelect?: (eventId: string) => void;
}

export function WorkflowEndCard({ card, onSelect }: Props) {
  const status = endStatusLabel(card.status);
  const selectable = !!onSelect;
  return (
    <div
      data-testid="workflow-end-card"
      className={styles.card}
      data-status={status.kind}
      {...(selectable
        ? {
            role: 'button',
            tabIndex: 0,
            onClick: () => onSelect!(card.notificationEventId),
            onKeyDown: (e: KeyboardEvent) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                onSelect!(card.notificationEventId);
              }
            },
          }
        : {})}
    >
      <div className={styles.head}>
        <CheckCircle2 size={13} aria-hidden className={styles.check} />
        <span className={styles.label}>워크플로우 종료</span>
        {card.name && <span className={styles.name}>{card.name}</span>}
        <span className={styles.time}>{timeLabel(card.endTimestamp)}</span>
        <span className={styles.stats}>
          <span data-testid="workflow-end-status" className={styles.statusPill}>
            {status.text}
          </span>
          <span>{card.agentCount} agents</span>
          {onSelect && (
            <button
              data-testid="workflow-end-jump"
              className={styles.jump}
              title="종료 알림 원문으로 이동"
              onClick={(e) => {
                e.stopPropagation();
                onSelect(card.notificationEventId);
              }}
            >
              <Bell size={10} aria-hidden /> 알림
            </button>
          )}
        </span>
      </div>
      {card.summary && (
        <div className={styles.summary}>
          <span className={styles.summaryLabel}>결과</span>
          {card.summary}
        </div>
      )}
    </div>
  );
}
