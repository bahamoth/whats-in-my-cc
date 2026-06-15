// webui/src/components/replay/stream/BatchGroup.tsx
// Renders one PARALLEL-DISPATCH batch (siblings spawned in the same assistant
// turn) as a 2-level collapsible container, sitting in the conversation as ONE
// chronological slot. Concurrency lives only BETWEEN the child agents, each of
// which is a serial sub-stream, so the batch absorbs the interleave and the
// children render as clean SubagentGroups.
//
//  - L0 (collapsed, default): batch identity (chip "병렬 배치", N agents, status
//    ✓N/N or ⏳, total wall-clock span) + the synthesis (종합) line. Children
//    hidden. While a batch is still running (settled === false) it defaults to
//    EXPANDED as a progress aid.
//  - L1 (expanded): each child SubagentGroup (which owns its own L2 detail fold)
//    + a bottom outcome line repeating the synthesis.
//
// Same prop signature as SubagentGroup so ConversationStream can swap it in.
import { useMemo, useState } from 'react';
import { ChevronDown, ChevronRight, GitFork } from 'lucide-react';
import { SubagentGroup } from './SubagentGroup';
import { formatDuration, durationHeat } from './duration';
import { workflowTimeline, agentDurationHeat } from './workflowStats';
import type { BatchGroup as BatchGroupModel, SidechainGroup, StreamItem } from './streamModel';
import styles from './BatchGroup.module.css';

interface BatchGroupProps {
  group: BatchGroupModel;
  selectedEventId: string | null;
  onSelect: (eventId: string) => void;
  findingEventIds: Set<string>;
}

/** All event ids reachable from a stream item (recursively through children) —
 *  used for the auto-expand-on-selection contains check. */
function itemEventIds(it: StreamItem): string[] {
  if (it.type === 'message') return [it.eventId];
  if (it.type === 'thinking') return it.events.map((e) => e.eventId);
  if (it.type === 'activity-run') return it.events.map((ae) => ae.event.event_id);
  if (it.type === 'batch-group' || it.type === 'workflow-group')
    return it.agentGroups.flatMap(itemEventIds);
  if (it.type === 'subagent-end' || it.type === 'workflow-end') return [];
  return it.items.flatMap(itemEventIds);
}

/** Wall-clock span (ms) across ALL child agents — from the earliest to the
 *  latest observed timestamp seen in any child's items. Child SidechainGroups
 *  hold message timestamps and activity-run / thinking observed_at; we sweep
 *  the same fields SubagentGroup.summarizeGroup reads so the batch total spans
 *  the whole parallel window (min start … max end). */
function batchSpanMs(groups: SidechainGroup[]): number {
  let min = Infinity;
  let max = -Infinity;
  const see = (iso: string) => {
    const t = new Date(iso).getTime();
    if (!Number.isNaN(t)) {
      min = Math.min(min, t);
      max = Math.max(max, t);
    }
  };
  for (const g of groups) {
    for (const it of g.items) {
      if (it.type === 'message') see(it.timestamp);
      else if (it.type === 'activity-run') for (const ae of it.events) see(ae.event.observed_at);
      else if (it.type === 'thinking') for (const e of it.events) see(e.timestamp);
    }
  }
  return max > min ? max - min : 0;
}

export function BatchGroup({
  group,
  selectedEventId,
  onSelect,
  findingEventIds,
}: BatchGroupProps) {
  // Fold policy mirrors SubagentGroup (null = no explicit choice → follow
  // containsSelected), with one addition: a STILL-RUNNING batch (settled ===
  // false) defaults to EXPANDED so progress is visible without a click. An
  // explicit toggle always wins.
  const [userOverride, setUserOverride] = useState<boolean | null>(null);
  const containsSelected =
    selectedEventId != null &&
    group.agentGroups.some((g) => itemEventIds(g).includes(selectedEventId));
  const defaultExpanded = containsSelected || !group.settled;
  const expanded = userOverride ?? defaultExpanded;

  const n = group.agentGroups.length;
  const doneCount = group.agentGroups.filter((g) => g.conclusion != null).length;
  const spanMs = useMemo(() => batchSpanMs(group.agentGroups), [group.agentGroups]);
  // Always-visible mini-gantt: one lane per agent (start offset + duration) so a
  // batch shows its concurrency at a glance, symmetric with WorkflowGroup (the
  // FanPanel "요약 상시" model — spec §6.5).
  const tl = useMemo(() => workflowTimeline(group.agentGroups), [group.agentGroups]);
  const pct = (ms: number) => (tl.spanMs > 0 ? (ms / tl.spanMs) * 100 : 0);

  const synthesis = group.synthesis;

  return (
    <section data-testid="batch-group" data-expanded={String(expanded)} className={styles.group}>
      <div className={styles.headerRow}>
        <button
          data-testid="batch-toggle"
          className={styles.header}
          onClick={() => setUserOverride(!expanded)}
          aria-expanded={expanded}
        >
          {expanded
            ? <ChevronDown size={13} aria-hidden className={styles.chevron} />
            : <ChevronRight size={13} aria-hidden className={styles.chevron} />}
          <GitFork size={13} aria-hidden className={styles.icon} />
          <span data-testid="batch-chip" className={styles.chip}>병렬 배치</span>
          <span data-testid="batch-meta" className={styles.meta}>
            <span>{n} agents</span>
            <span data-testid="batch-status" className={styles.status}>
              {group.settled ? `✓ ${doneCount}/${n}` : '⏳'}
            </span>
            {spanMs > 0 && (
              <span className={styles.duration} data-heat={durationHeat(spanMs)}>
                {formatDuration(spanMs)}
              </span>
            )}
            {group.concurrentMainCount ? (
              <span data-testid="batch-concurrent" className={styles.concurrent}>⟂ main {group.concurrentMainCount}건 동시</span>
            ) : null}
          </span>
        </button>
      </div>
      {/* L0 synthesis line — visible whether collapsed or expanded so the
          batch's outcome is always one glance away. While running it reads
          "진행 중" until the main thread's synthesis message lands. */}
      <div data-testid="batch-synthesis" className={styles.synthesis}>
        <span className={styles.synthesisLabel}>종합</span>
        <span className={styles.synthesisText}>{synthesis || '진행 중'}</span>
      </div>
      {/* always-visible mini-gantt — concurrency at a glance, even collapsed */}
      <div className={styles.gantt}>
        {tl.lanes.map((l) => (
          <div key={l.id} data-testid="batch-lane" className={styles.lane}>
            <span className={styles.laneLabel} title={l.label}>{l.label}</span>
            <div className={styles.track}>
              <div
                className={styles.bar}
                data-heat={agentDurationHeat(l.durMs)}
                style={{ left: `${pct(l.startMs)}%`, width: `${Math.max(1.5, pct(l.durMs))}%`, background: l.color }}
              >
                <span className={styles.barLabel}>{formatDuration(l.durMs)}</span>
              </div>
            </div>
          </div>
        ))}
      </div>
      {expanded && (
        <div className={styles.body}>
          {group.agentGroups.map((g) => (
            <SubagentGroup
              key={g.id}
              group={g}
              selectedEventId={selectedEventId}
              onSelect={onSelect}
              findingEventIds={findingEventIds}
            />
          ))}
        </div>
      )}
    </section>
  );
}
