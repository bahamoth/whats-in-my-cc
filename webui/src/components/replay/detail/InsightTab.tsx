// webui/src/components/replay/detail/InsightTab.tsx
//
// Metrics-led Insight tab (UX option A): for the selected graph node, show a
// compact header + the entity's COLLECTED metrics (EntityMetricsPanel, with
// plain-language ⓘ tooltips) + that node's Findings. The old FocusedInsightGraph
// subgraph and the shallow per-kind NodeDetail sections were removed — the full
// raw payload still lives in the Raw tab.
import type { FindingDto, GraphNodeDto } from '../../../api/types';
import type { LlmRequestMetrics } from '../stream/llmRequestMetrics';
import { nodeLabel } from '../stream/nodeLabel';
import type { ToolMetrics } from './toolMetrics';
import { EntityMetricsPanel } from './EntityMetricsPanel';
import styles from './InsightTab.module.css';

interface InsightTabProps {
  findings: FindingDto[];
  node: GraphNodeDto | null;
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

export function InsightTab({ findings, node, toolMetrics, llmMetrics }: InsightTabProps) {
  if (!node && findings.length === 0) {
    return (
      <div className={styles.root}>
        <p className={styles.empty}>No insights for this node.</p>
      </div>
    );
  }

  const label = node ? nodeLabel(node) : null;
  const icon = label ? KIND_ICON[label.kind] ?? KIND_ICON.other : null;

  return (
    <div className={styles.root}>
      {node && (
        <>
          <div className={styles.nodeHeader}>
            <span className={styles.nodeIcon} aria-hidden="true">{icon}</span>
            <span className={styles.nodePrimary}>{label?.primary}</span>
            <span className={styles.nodeId}>{node.node_id}</span>
          </div>
          <EntityMetricsPanel
            kind={node.node_kind}
            toolMetrics={toolMetrics}
            llmMetrics={llmMetrics}
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
