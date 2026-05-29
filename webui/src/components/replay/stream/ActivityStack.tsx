// webui/src/components/replay/stream/ActivityStack.tsx
import { useMemo, useState } from 'react';
import { ChevronRight, ChevronDown, Wrench, AlertTriangle } from 'lucide-react';
import { summarizeStack } from './activityGroup';
import type { ActivityStackData } from './activityGroup';
import { nodeLabel } from './nodeLabel';
import styles from './ActivityStack.module.css';

interface ActivityStackProps {
  stack: ActivityStackData;
  selectedEventId: string | null;
  onSelect: (eventId: string) => void;
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

export function ActivityStack({ stack, selectedEventId, onSelect }: ActivityStackProps) {
  const [userExpanded, setUserExpanded] = useState(false);
  const summary = useMemo(() => summarizeStack(stack), [stack]);

  // Auto-expand when the host selects one of our events (e.g. timeline /
  // subgraph click): the host needs the selected activity item mounted so it
  // can scroll it into view. Manual toggling still works on top of this.
  const containsSelected =
    selectedEventId != null && stack.events.some((ae) => ae.event.event_id === selectedEventId);
  const expanded = userExpanded || containsSelected;

  const phase = summary.phase ?? '';

  return (
    <div
      data-testid="activity-stack"
      data-phase={phase}
      data-count={String(summary.count)}
      data-errors={String(summary.errorCount)}
      className={styles.stack}
    >
      <button
        data-testid="activity-stack-toggle"
        className={styles.toggle}
        onClick={() => setUserExpanded((v) => !v)}
        aria-expanded={expanded}
      >
        {expanded
          ? <ChevronDown size={13} aria-hidden className={styles.chevron} />
          : <ChevronRight size={13} aria-hidden className={styles.chevron} />}
        <Wrench size={12} aria-hidden />
        {phase && <span className={styles.phase}>{phase}</span>}
        {summary.topTools.length > 0 && (
          <span className={styles.topTools}>{summary.topTools.join(' · ')}</span>
        )}
        <span className={styles.count}>{summary.count} events</span>
        {summary.durationMs > 0 && (
          <span className={styles.duration}>{formatDuration(summary.durationMs)}</span>
        )}
        {summary.errorCount > 0 && (
          <span className={styles.badgeError}>
            <AlertTriangle size={10} aria-hidden />
            {summary.errorCount}
          </span>
        )}
      </button>

      {expanded && (
        <div className={styles.items}>
          {stack.events.map((ae) => {
            const label = nodeLabel({ node_kind: ae.event.kind, payload: ae.event.payload });
            const isSelected = selectedEventId === ae.event.event_id;
            return (
              <div
                key={ae.event.event_id}
                data-testid="activity-item"
                data-selected={String(isSelected)}
                role="button"
                tabIndex={0}
                className={styles.item}
                onClick={() => onSelect(ae.event.event_id)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    onSelect(ae.event.event_id);
                  }
                }}
              >
                <span className={styles.itemPrimary}>{label.primary}</span>
                {label.secondary && (
                  <span className={styles.itemSecondary}>{label.secondary}</span>
                )}
                {ae.result != null && (
                  ae.result.isError
                    ? <span className={styles.itemBadgeError}>error</span>
                    : <span className={styles.itemBadgeOk}>ok</span>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
