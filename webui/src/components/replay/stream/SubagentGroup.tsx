// webui/src/components/replay/stream/SubagentGroup.tsx
// Renders one Task-subagent exchange (a sidechain run) as a single indented,
// COLLAPSIBLE block. Collapsed (the default) it reads as one overview line —
// agent identity, the dispatched prompt's first line, message/tool counts and
// the wall-clock span — so parallel dispatches scan at a glance; expanded it
// shows the inner stream (prompt, the subagent's replies, its tool activity).
import { useMemo, useState } from 'react';
import { ChevronDown, ChevronRight, CornerDownRight, CornerUpLeft } from 'lucide-react';
import { MessageCard } from './MessageCard';
import { ActivityStack } from './ActivityStack';
import { ThinkingMarker } from './ThinkingMarker';
import { formatDuration, durationHeat } from './duration';
import type { SidechainGroup, StreamItem } from './streamModel';
import { agentColor } from '../../../lib/colorHash';
import styles from './SubagentGroup.module.css';

interface SubagentGroupProps {
  group: SidechainGroup;
  selectedEventId: string | null;
  onSelect: (eventId: string) => void;
  findingEventIds: Set<string>;
  /** Top-level standalone background subagent: drop the nested indent + own
   *  border (the hairline gutter rail beside it provides both), so the rail's
   *  ▢ node connects to the card. Nested (batch/workflow) children stay indented. */
  flush?: boolean;
}

interface GroupSummary {
  messageCount: number;
  toolCount: number;
  /** wall-clock span (ms) from the group's first to last observed timestamp. */
  durationMs: number;
  /** first line of the orchestrator's dispatched prompt, '' when absent. */
  promptPreview: string;
}

function itemEventIds(it: StreamItem): string[] {
  if (it.type === 'message') return [it.eventId];
  if (it.type === 'thinking') return it.events.map((e) => e.eventId);
  if (it.type === 'activity-run') return it.events.map((ae) => ae.event.event_id);
  if (it.type === 'batch-group' || it.type === 'workflow-group')
    return it.agentGroups.flatMap(itemEventIds);
  if (it.type === 'subagent-end') return [];
  return it.items.flatMap(itemEventIds);
}

function summarizeGroup(group: SidechainGroup): GroupSummary {
  let messageCount = 0;
  let toolCount = 0;
  let min = Infinity;
  let max = -Infinity;
  let promptPreview = '';
  const see = (iso: string) => {
    const t = new Date(iso).getTime();
    if (!Number.isNaN(t)) {
      min = Math.min(min, t);
      max = Math.max(max, t);
    }
  };
  for (const it of group.items) {
    if (it.type === 'message') {
      messageCount++;
      see(it.timestamp);
      if (!promptPreview && it.role === 'user') {
        promptPreview = it.text.split('\n', 1)[0].trim();
      }
    } else if (it.type === 'activity-run') {
      for (const ae of it.events) {
        if (ae.event.kind === 'tool_call') toolCount++;
        see(ae.event.observed_at);
      }
    } else if (it.type === 'thinking') {
      for (const e of it.events) see(e.timestamp);
    }
  }
  return { messageCount, toolCount, durationMs: max > min ? max - min : 0, promptPreview };
}

export function SubagentGroup({
  group,
  selectedEventId,
  onSelect,
  findingEventIds,
  flush = false,
}: SubagentGroupProps) {
  // Same fold policy as ActivityStack: `null` = no explicit user choice yet →
  // follow `containsSelected` (auto-open when a selection lands inside so the
  // host can scroll it into view); an explicit toggle then wins either way.
  const [userOverride, setUserOverride] = useState<boolean | null>(null);
  const summary = useMemo(() => summarizeGroup(group), [group]);
  const containsSelected =
    selectedEventId != null &&
    group.items.some((it) => itemEventIds(it).includes(selectedEventId));
  const expanded = userOverride ?? containsSelected;

  // 사이드카 description(디스패치한 Task의 의도)이 프롬프트 첫 줄보다 정확한
  // 한 줄 정체성이다 — 있으면 우선한다.
  const preview = group.description || summary.promptPreview;

  return (
    <section
      data-testid="subagent-group"
      data-expanded={String(expanded)}
      className={`${styles.group} ${flush ? styles.flush : ''}`}
      style={{ ['--agentColor' as string]: agentColor(group.agentId) }}
    >
      <div className={styles.headerRow}>
        <button
          data-testid="subagent-toggle"
          className={styles.header}
          onClick={() => setUserOverride(!expanded)}
          aria-expanded={expanded}
        >
          {expanded
            ? <ChevronDown size={13} aria-hidden className={styles.chevron} />
            : <ChevronRight size={13} aria-hidden className={styles.chevron} />}
          <CornerDownRight size={13} aria-hidden className={styles.icon} />
          <span className={styles.label}>Subagent</span>
          <span data-testid="subagent-swatch" className={styles.swatch} aria-hidden />
          {group.agentType && (
            <span data-testid="subagent-type" className={styles.agentType}>
              {group.agentType}
            </span>
          )}
          {group.agentId && (
            <span data-testid="subagent-agent-chip" className={styles.agentChip} title={group.agentId}>
              {group.agentId.slice(0, 6)}
            </span>
          )}
          {preview && (
            <span data-testid="subagent-preview" className={styles.preview}>
              {preview}
            </span>
          )}
          <span data-testid="subagent-meta" className={styles.meta}>
            <span>메시지 {summary.messageCount}</span>
            <span>도구 {summary.toolCount}</span>
            {summary.durationMs > 0 && (
              <span className={styles.duration} data-heat={durationHeat(summary.durationMs)}>
                {formatDuration(summary.durationMs)}
              </span>
            )}
            {group.concurrentMainCount ? (
              <span data-testid="subagent-concurrent" className={styles.concurrent}>⟂ main {group.concurrentMainCount}건 동시</span>
            ) : null}
            {group.conclusion ? (
              <span data-testid="subagent-status" className={styles.statusDone}>✓ 완료</span>
            ) : (
              <span data-testid="subagent-status" className={styles.statusRun}>● 실행 중</span>
            )}
          </span>
        </button>
        {group.taskEventId && (
          <button
            data-testid="subagent-jump"
            className={styles.jump}
            title="호출한 Task로 이동"
            aria-label="호출한 Task로 이동"
            onClick={() => onSelect(group.taskEventId!)}
          >
            <CornerUpLeft size={12} aria-hidden />
            Task
          </button>
        )}
      </div>
      {/* 결론 = 그 agent의 마지막 assistant_message 요약. 헤더 아래에 두어
          접힌 상태에서도 "이 에이전트가 무엇을 결론지었나"가 한눈에 보인다.
          단, 끝 카드(hasEndCard)가 결론을 담당하면 여기선 숨긴다(요청→결과 분리). */}
      {group.conclusion && !group.hasEndCard && (
        <div data-testid="subagent-conclusion" className={styles.conclusion}>
          <span className={styles.conclusionLabel}>결론</span>
          <span className={styles.conclusionText}>{group.conclusion}</span>
        </div>
      )}
      {expanded && (
        <div className={styles.body}>
          {group.items.map((it) => {
            if (it.type === 'message') {
              return (
                <MessageCard
                  key={it.id}
                  item={it}
                  selected={it.eventId === selectedEventId}
                  onSelect={onSelect}
                  hasFinding={findingEventIds.has(it.eventId)}
                />
              );
            }
            if (it.type === 'activity-run') {
              return (
                <ActivityStack
                  key={it.id}
                  stack={{ events: it.events }}
                  selectedEventId={selectedEventId}
                  onSelect={onSelect}
                />
              );
            }
            if (it.type === 'thinking') {
              return (
                <ThinkingMarker
                  key={it.id}
                  marker={it}
                  selectedEventId={selectedEventId}
                  onSelect={onSelect}
                />
              );
            }
            // nested sidechain groups do not occur (grouping is one level deep)
            return null;
          })}
        </div>
      )}
    </section>
  );
}
