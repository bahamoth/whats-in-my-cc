import { useMemo, useState } from 'react';
import { ChevronDown, ChevronRight, Workflow as WorkflowIcon, CornerUpLeft } from 'lucide-react';
import { SubagentGroup } from './SubagentGroup';
import { formatDuration } from './duration';
import { workflowStats, workflowTimeline, agentDurationHeat } from './workflowStats';
import type { WorkflowGroup as WorkflowGroupModel, StreamItem } from './streamModel';
import styles from './WorkflowGroup.module.css';

interface Props { group: WorkflowGroupModel; selectedEventId: string | null; onSelect: (id: string) => void; findingEventIds: Set<string>; }

function itemEventIds(it: StreamItem): string[] {
  if (it.type === 'message') return [it.eventId];
  if (it.type === 'thinking') return it.events.map((e) => e.eventId);
  if (it.type === 'activity-run') return it.events.map((ae) => ae.event.event_id);
  if (it.type === 'batch-group' || it.type === 'workflow-group') return it.agentGroups.flatMap(itemEventIds);
  if (it.type === 'subagent-end' || it.type === 'workflow-end') return [];
  return it.items.flatMap(itemEventIds);
}

export function WorkflowGroup({ group, selectedEventId, onSelect, findingEventIds }: Props) {
  const [userOverride, setUserOverride] = useState<boolean | null>(null);
  // S5 — per-lane inline drill (independent of the full "전체 펼치기" toggle).
  const [drilled, setDrilled] = useState<Set<string>>(() => new Set());
  const toggleDrill = (id: string) =>
    setDrilled((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  const containsSelected = selectedEventId != null && group.agentGroups.some((g) => itemEventIds(g).includes(selectedEventId));
  const expanded = userOverride ?? (containsSelected || !group.settled);
  const stats = useMemo(() => workflowStats(group.agentGroups), [group.agentGroups]);
  const tl = useMemo(() => workflowTimeline(group.agentGroups), [group.agentGroups]);
  const pct = (n: number) => (tl.spanMs > 0 ? (n / tl.spanMs) * 100 : 0);

  // Single-agent workflow: a 1-bar gantt, "최대 병렬 1" stats, and a wrapper
  // chevron over one child are noise (the synth's primary single-item offender).
  // Keep the workflow identity as a compact one-line label and render the lone
  // child directly — it stays collapsible on its own (no double shell).
  if (group.agentGroups.length === 1) {
    const only = group.agentGroups[0];
    return (
      <section data-testid="workflow-group" data-flattened="true" className={styles.group}>
        <div className={styles.flatLabel}>
          <WorkflowIcon size={12} className={styles.icon} aria-hidden />
          <span className={styles.chip}>워크플로우</span>
          <span className={styles.name}>{group.name ?? '워크플로우'}</span>
          <span className={styles.status}>{group.settled ? '✓' : '⏳'}</span>
          {group.synthesis && <span className={styles.flatSynth}>· {group.synthesis}</span>}
          {group.taskEventId && (
            <button
              data-testid="wf-jump"
              className={styles.jump}
              title="이 워크플로우를 띄운 Workflow 호출로 이동"
              aria-label="Workflow 호출로 이동"
              onClick={() => onSelect(group.taskEventId!)}
            >
              <CornerUpLeft size={12} aria-hidden />
              호출
            </button>
          )}
        </div>
        <SubagentGroup group={only} selectedEventId={selectedEventId} onSelect={onSelect} findingEventIds={findingEventIds} />
      </section>
    );
  }

  return (
    <section data-testid="workflow-group" data-expanded={String(expanded)} className={styles.group}>
      <div className={styles.headerRow}>
        <button data-testid="wf-toggle" className={styles.header} onClick={() => setUserOverride(!expanded)} aria-expanded={expanded}>
          {expanded ? <ChevronDown size={13} className={styles.chevron} /> : <ChevronRight size={13} className={styles.chevron} />}
          <WorkflowIcon size={13} className={styles.icon} aria-hidden />
          <span className={styles.chip}>워크플로우</span>
          <span className={styles.name}>{group.name ?? '워크플로우'}</span>
          <span className={styles.meta}>
            <span>{stats.agentCount} agents</span>
            <span className={styles.status}>{group.settled ? `✓ ${stats.agentCount}/${stats.agentCount}` : '⏳'}</span>
            {tl.spanMs > 0 && <span className={styles.duration} data-heat={agentDurationHeat(stats.longestMs)}>{formatDuration(tl.spanMs)}</span>}
            {group.concurrentMainCount ? (
              <span data-testid="wf-concurrent" className={styles.concurrent}>⟂ main {group.concurrentMainCount}건 동시</span>
            ) : null}
          </span>
        </button>
        {group.taskEventId && (
          <button
            data-testid="wf-jump"
            className={styles.jump}
            title="이 워크플로우를 띄운 Workflow 호출로 이동"
            aria-label="Workflow 호출로 이동"
            onClick={() => onSelect(group.taskEventId!)}
          >
            <CornerUpLeft size={12} aria-hidden />
            호출
          </button>
        )}
      </div>

      <div data-testid="wf-synthesis" className={styles.synthesis}>
        <span className={styles.synthesisLabel}>종합</span>
        <span>{group.synthesis || '진행 중'}</span>
      </div>

      <div data-testid="wf-stats" className={styles.stats}>
        <span className={styles.stat}>최대 병렬 <b>{stats.maxConcurrency}</b></span>
        <span className={styles.stat} data-heat={agentDurationHeat(stats.longestMs)}>최장 <b>{formatDuration(stats.longestMs)}</b></span>
        <span className={styles.stat}>중앙값 <b>{formatDuration(stats.medianMs)}</b></span>
        {stats.incomplete > 0 && <span className={styles.stat}>미완 <b>{stats.incomplete}</b></span>}
      </div>

      {/* 항상 보이는 컴팩트 미니 간트 — S5: 레인 클릭 시 그 에이전트만 인라인 드릴 */}
      <div className={styles.gantt}>
        {tl.lanes.map((l) => {
          const isDrilled = expanded || drilled.has(l.id);
          return (
            <button
              key={l.id}
              type="button"
              data-testid="wf-lane"
              data-drilled={isDrilled ? 'true' : undefined}
              className={styles.lane}
              aria-expanded={isDrilled}
              title={`${l.label} — 클릭하여 펼치기`}
              onClick={() => toggleDrill(l.id)}
            >
              <span className={styles.laneLabel}>{l.label}</span>
              <div className={styles.track}>
                <div className={styles.bar}
                     style={{ left: `${pct(l.startMs)}%`, width: `${Math.max(1.5, pct(l.durMs))}%`, background: l.color }}>
                  <span className={styles.barLabel}>{formatDuration(l.durMs)}</span>
                </div>
              </div>
            </button>
          );
        })}
      </div>

      {(expanded ? group.agentGroups : group.agentGroups.filter((g) => drilled.has(g.id))).length > 0 && (
        <div className={styles.body}>
          {(expanded ? group.agentGroups : group.agentGroups.filter((g) => drilled.has(g.id))).map((g) => (
            <SubagentGroup key={g.id} group={g} selectedEventId={selectedEventId} onSelect={onSelect} findingEventIds={findingEventIds} />
          ))}
        </div>
      )}
    </section>
  );
}
