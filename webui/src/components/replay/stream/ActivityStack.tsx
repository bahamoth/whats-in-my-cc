// webui/src/components/replay/stream/ActivityStack.tsx
import { useMemo, useState } from 'react';
import { ChevronRight, ChevronDown, Wrench, AlertTriangle } from 'lucide-react';
import { summarizeStack } from './activityGroup';
import type { ActivityStackData } from './activityGroup';
import { nodeLabel } from './nodeLabel';
import { agentColor } from '../../../lib/colorHash';
import { tagVerb, type Tag } from './eventTags';
import { hookFacet } from './hookFacet';
import { formatDuration, durationHeat } from './duration';
import { useT } from '../../../i18n';
import styles from './ActivityStack.module.css';

interface ActivityStackProps {
  stack: ActivityStackData;
  selectedEventId: string | null;
  onSelect: (eventId: string) => void;
  /** flat 모드(필터 활성 — buildStreamModel opts.flat): 서브에이전트 그룹이
   *  없으므로 sidechain 활동 스택에 출처 배지(⑂)를 헤더에 1회 붙인다. */
  flatMode?: boolean;
}

export function ActivityStack({ stack, selectedEventId, onSelect, flatMode = false }: ActivityStackProps) {
  const t = useT();
  const isSidechainStack = flatMode && stack.events.some((ae) => !!ae.event.is_sidechain);
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

  const renderItem = (ae: ActivityStackData['events'][number]) => {
    const label = nodeLabel({ node_kind: ae.event.kind, payload: ae.event.payload, telemetry: ae.event.telemetry, tag: ae.event.tag, is_meta: ae.event.is_meta }, t);
    // B-6b — teammate 디스패치 북엔드: Agent(named) 호출 라벨을 그 팀메이트의
    // agentColor로 칠해 응답 카드와 짝으로 읽히게 한다(연속 레일 없음).
    const dispatchName = (() => {
      if (ae.event.kind !== 'tool_call') return null;
      const payload = ae.event.payload as Record<string, unknown> | null;
      if (!payload || payload['tool_name'] !== 'Agent') return null;
      const input = payload['input'] as Record<string, unknown> | undefined;
      const nm = input?.['name'];
      return typeof nm === 'string' && nm ? nm : null;
    })();
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
        <span
          className={styles.itemPrimary}
          style={dispatchName ? { color: agentColor(dispatchName) } : undefined}
        >
          {label.primary}
        </span>
        {(() => { const tr = ae.event.tag; return tr && tr.disposition === 'tagged' && tr.value
          ? <span data-testid="event-tag-chip" className={styles.tagChip} data-verb={tagVerb(tr.value as Tag)}>{tr.value}</span> : null; })()}
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
                <span className={styles.duration} data-heat={durationHeat(durationMs)}>
                  {formatDuration(durationMs)}
                </span>
              )}
            </span>
          );
        })()}
      </div>
    );
  };

  // A run of exactly one event hides nothing behind a chevron — the collapsed
  // header would just repeat the tool name + duration. Render it inline (the
  // lone item, selectable) with no toggle shell. Single-item collapse removed.
  if (stack.events.length === 1) {
    return (
      <div
        data-testid="activity-stack"
        data-single="true"
        data-count={String(summary.count)}
        data-errors={String(summary.errorCount)}
        className={`${styles.stack} ${styles.single}`}
      >
        {isSidechainStack && (
          <span data-testid="flat-sidechain-badge" className={styles.flatBadge}>
            {t('stream.flatSidechainBadge')}
          </span>
        )}
        <div className={styles.items}>{renderItem(stack.events[0])}</div>
      </div>
    );
  }

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
        {isSidechainStack && (
          <span data-testid="flat-sidechain-badge" className={styles.flatBadge}>
            {t('stream.flatSidechainBadge')}
          </span>
        )}
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
            <span className={styles.duration} data-heat={durationHeat(summary.durationMs)}>
              {formatDuration(summary.durationMs)}
            </span>
          )}
        </span>
      </button>

      {expanded && (
        <div className={styles.items}>
          {stack.events.map((ae) => renderItem(ae))}
        </div>
      )}
    </div>
  );
}
