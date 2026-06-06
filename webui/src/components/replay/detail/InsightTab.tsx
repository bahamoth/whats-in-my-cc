// webui/src/components/replay/detail/InsightTab.tsx
//
// Metrics-led Insight tab (UX option A): for the selected event, show a
// compact header + the entity's COLLECTED metrics (EntityMetricsPanel, with
// plain-language ⓘ tooltips) + that event's Findings. The full raw payload
// lives in the Raw tab.
import type { FindingDto, ObservedEventDto } from '../../../api/types';
import type { LlmRequestMetrics } from '../stream/llmRequestMetrics';
import { nodeLabel } from '../stream/nodeLabel';
import type { ToolMetrics } from './toolMetrics';
import { EntityMetricsPanel } from './EntityMetricsPanel';
import styles from './InsightTab.module.css';

interface InsightTabProps {
  findings: FindingDto[];
  event: ObservedEventDto | null;
  toolMetrics: ToolMetrics | null;
  llmMetrics: LlmRequestMetrics | null;
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

const SEV_CLASS: Record<string, string> = { high: 'sevHigh', medium: 'sevMed', low: 'sevLow' };

function FindingsList({ findings }: { findings: FindingDto[] }) {
  return (
    <ul className={styles.list}>
      {findings.map((f) => (
        <li key={f.finding_id} className={styles.item}>
          <div className={styles.head}>
            <span className={`${styles.sev} ${styles[SEV_CLASS[f.severity] ?? 'sevLow']}`}>
              {f.severity}
            </span>
            <span className={styles.category}>{f.category}</span>
            <span className={styles.confidence}>{Math.round(f.confidence * 100)}%</span>
          </div>
          <p className={styles.summary}>{f.summary}</p>
        </li>
      ))}
    </ul>
  );
}

export function InsightTab({ findings, event, toolMetrics, llmMetrics }: InsightTabProps) {
  if (!event && findings.length === 0) {
    return (
      <div className={styles.root}>
        <p className={styles.empty}>No insights for this event.</p>
      </div>
    );
  }

  const label = event ? nodeLabel({ node_kind: event.kind, payload: event.payload, telemetry: event.telemetry }) : null;
  const icon = label ? KIND_ICON[label.kind] ?? KIND_ICON.other : null;

  return (
    <div className={styles.root}>
      {event && (
        <>
          <div className={styles.nodeHeader}>
            <span className={styles.nodeIcon} aria-hidden="true">{icon}</span>
            <span className={styles.nodePrimary}>{label?.primary}</span>
            {/* For a tool call, surface WHAT it did (the operation summary) right
                in the header — the metrics below say how long/how big, but not
                what the call actually operated on. */}
            {label?.kind === 'tool' && label.secondary && (
              <span className={styles.nodeSecondary}>{label.secondary}</span>
            )}
            <span className={styles.nodeId}>{event.event_id}</span>
          </div>
          <EntityMetricsPanel
            kind={event.kind}
            toolMetrics={toolMetrics}
            llmMetrics={llmMetrics}
            payload={event.payload}
          />
        </>
      )}

      {findings.length > 0 && (
        <div className={styles.findingsSection}>
          <div className={styles.sectionTitle}>Findings</div>
          <FindingsList findings={findings} />
        </div>
      )}
    </div>
  );
}
