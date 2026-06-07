// webui/src/components/replay/detail/InsightTab.tsx
//
// Metrics-led Insight tab (UX option A): for the selected event, show a
// compact header + the entity's COLLECTED metrics (EntityMetricsPanel, with
// plain-language ⓘ tooltips) + that event's Signals. The full raw payload
// lives in the Raw tab.
import type { SignalDto, ObservedEventDto } from '../../../api/types';
import type { LlmRequestMetrics } from '../stream/llmRequestMetrics';
import { nodeLabel } from '../stream/nodeLabel';
import type { ToolMetrics } from './toolMetrics';
import { EntityMetricsPanel } from './EntityMetricsPanel';
import { eventProvenance } from './eventProvenance';
import { WhatSection } from './WhatSection';
import styles from './InsightTab.module.css';

interface InsightTabProps {
  signals: SignalDto[];
  event: ObservedEventDto | null;
  toolMetrics: ToolMetrics | null;
  llmMetrics: LlmRequestMetrics | null;
  /** The matching tool_result event when the selected event is a tool_call. */
  matchedResult?: ObservedEventDto | null;
}

const KIND_ICON: Record<string, string> = {
  tool: '⚙',
  assistant: '✦',
  thinking: '…',
  user: '◎',
  hook: '↩',
  span: '◇',
  verify: '✓',
  diff: '±',
  other: '·',
};

function SignalsList({ signals }: { signals: SignalDto[] }) {
  return (
    <ul className={styles.list}>
      {signals.map((s) => (
        <li key={s.signal_id} className={styles.item}>
          <div className={styles.head}>
            <span className={styles.detector}>{s.detector}</span>
            {s.subkind && <span className={styles.subkind}>{s.subkind}</span>}
          </div>
          <p className={styles.summary}>{s.summary}</p>
        </li>
      ))}
    </ul>
  );
}

export function InsightTab({ signals, event, toolMetrics, llmMetrics, matchedResult }: InsightTabProps) {
  if (!event && signals.length === 0) {
    return (
      <div className={styles.root}>
        <p className={styles.empty}>No insights for this event.</p>
      </div>
    );
  }

  const label = event ? nodeLabel({ node_kind: event.kind, payload: event.payload, telemetry: event.telemetry }) : null;
  const icon = label ? KIND_ICON[label.kind] ?? KIND_ICON.other : null;
  const prov = event ? eventProvenance(event.kind) : null;

  return (
    <div className={styles.root}>
      {event && (
        <>
          {/* H: header + provenance badge + correlation chips */}
          <div className={styles.nodeHeader}>
            <span className={styles.nodeIcon} aria-hidden="true">{icon}</span>
            <span className={styles.nodePrimary}>{label?.primary}</span>
            {prov && (
              <span
                className={prov.kind === 'native' ? styles.badgeNative : styles.badgeDerived}
                title={prov.kind === 'native' ? 'Claude Code 원본 관측' : 'wimcc 파생 데이터'}
              >
                {prov.label}
              </span>
            )}
            <span className={styles.nodeId}>{event.event_id}</span>
          </div>

          {/* Correlation chips (tool_use_id / request_id / turn_id) — display-only */}
          {(event.tool_use_id || event.request_id || event.turn_id) && (
            <div className={styles.corrChips}>
              {event.tool_use_id && (
                <span className={styles.corrChip} title="tool_use_id">
                  tool <code>{event.tool_use_id}</code>
                </span>
              )}
              {event.request_id && (
                <span className={styles.corrChip} title="request_id">
                  req <code>{event.request_id}</code>
                </span>
              )}
              {event.turn_id && (
                <span className={styles.corrChip} title="turn_id">
                  turn <code>{event.turn_id}</code>
                </span>
              )}
            </div>
          )}

          {/* ① WHAT: what this event did */}
          <div className={styles.section}>
            <div className={styles.sectionTitle}>What — 한 일</div>
            <WhatSection event={event} matchedResult={matchedResult ?? null} />
          </div>

          {/* ② HOW: execution metrics */}
          <div className={styles.section}>
            <div className={styles.sectionTitle}>How — 지표</div>
            <EntityMetricsPanel
              kind={event.kind}
              toolMetrics={toolMetrics}
              llmMetrics={llmMetrics}
              payload={event.payload}
            />
          </div>
        </>
      )}

      {/* ③ SIGNALS */}
      {signals.length > 0 && (
        <div className={styles.signalsSection}>
          <div className={styles.sectionTitle}>Signals</div>
          <SignalsList signals={signals} />
        </div>
      )}
    </div>
  );
}
