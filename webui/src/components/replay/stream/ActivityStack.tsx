// webui/src/components/replay/stream/ActivityStack.tsx
import { useMemo, useState } from 'react';
import { ChevronRight, ChevronDown, Wrench, AlertTriangle } from 'lucide-react';
import { summarizeStack } from './activityGroup';
import type { ActivityStackData } from './activityGroup';
import { nodeLabel } from './nodeLabel';
import { tagForEvent } from './eventTags';
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
  // `null` = no explicit user choice yet → follow `containsSelected` (auto-open
  // when a selection lands inside this run so the host can scroll it into view).
  // Once the user toggles, their explicit true/false wins — so they CAN COLLAPSE
  // the run even while a child is selected (bug: "can't fold after selecting a
  // sub-item", caused by the old unconditional `userExpanded || containsSelected`).
  const [userOverride, setUserOverride] = useState<boolean | null>(null);
  const summary = useMemo(() => summarizeStack(stack), [stack]);

  const containsSelected =
    selectedEventId != null && stack.events.some((ae) => ae.event.event_id === selectedEventId);
  const expanded = userOverride ?? containsSelected;

  return (
    <div
      data-testid="activity-stack"
      data-count={String(summary.count)}
      data-errors={String(summary.errorCount)}
      className={styles.stack}
    >
      <button
        data-testid="activity-stack-toggle"
        className={styles.toggle}
        onClick={() => setUserOverride(!expanded)}
        aria-expanded={expanded}
      >
        {expanded
          ? <ChevronDown size={13} aria-hidden className={styles.chevron} />
          : <ChevronRight size={13} aria-hidden className={styles.chevron} />}
        <Wrench size={12} aria-hidden />
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
                {(() => { const tr = tagForEvent(ae.event); return tr.disposition === 'tagged' && tr.tag
                  ? <span data-testid="event-tag-chip" className={styles.tagChip}>{tr.tag}</span> : null; })()}
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
