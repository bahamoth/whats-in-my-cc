// webui/src/components/replay/stream/ActivityStack.tsx
import { useMemo, useState } from 'react';
import { ChevronRight, ChevronDown, Wrench, AlertTriangle } from 'lucide-react';
import { summarizeStack } from './activityGroup';
import type { ActivityStackData } from './activityGroup';
import { nodeLabel } from './nodeLabel';
import { tagForEvent, tagVerb } from './eventTags';
import { hookFacet } from './hookFacet';
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
        <span data-testid="fold-meta" className={styles.meta}>
          {summary.errorCount > 0 && (
            <span className={styles.badgeError}>
              <AlertTriangle size={10} aria-hidden />
              {summary.errorCount}
            </span>
          )}
          {summary.durationMs > 0 && (
            <span className={styles.duration}>{formatDuration(summary.durationMs)}</span>
          )}
        </span>
      </button>

      {expanded && (
        <div className={styles.items}>
          {stack.events.map((ae) => {
            const label = nodeLabel({ node_kind: ae.event.kind, payload: ae.event.payload });
            const isSelected = selectedEventId === ae.event.event_id;
            // hook_event carries its own success/duration in its payload (not a
            // matched tool_result), so derive the badge + duration from there.
            const hook = ae.event.kind === 'hook_event' ? hookFacet(ae.event.payload) : null;
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
                  ? <span data-testid="event-tag-chip" className={styles.tagChip} data-verb={tagVerb(tr.tag)}>{tr.tag}</span> : null; })()}
                {label.secondary && (
                  <span className={styles.itemSecondary}>{label.secondary}</span>
                )}
                {(() => {
                  // One right-aligned meta cluster for every item — order: time,
                  // then status — so tool and hook rows line up consistently.
                  // hook time/status come from the event's own payload; tool's
                  // from the upstream-computed durationMs + matched result.
                  const status: 'ok' | 'error' | null = hook
                    ? (hook.success == null ? null : hook.success ? 'ok' : 'error')
                    : ae.result == null
                    ? null
                    : ae.result.isError
                    ? 'error'
                    : 'ok';
                  const durationMs = hook ? hook.durationMs : ae.durationMs ?? null;
                  if (durationMs == null && status == null) return null;
                  return (
                    <span data-testid="activity-meta" className={styles.meta}>
                      {status === 'ok' && <span className={styles.itemBadgeOk}>ok</span>}
                      {status === 'error' && <span className={styles.itemBadgeError}>error</span>}
                      {durationMs != null && (
                        <span className={styles.duration}>{formatDuration(durationMs)}</span>
                      )}
                    </span>
                  );
                })()}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
