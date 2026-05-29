// webui/src/components/replay/detail/InsightTab.tsx
import type { FindingDto, GraphNodeDto, GraphEdgeDto } from '../../../api/types';
import { FocusedInsightGraph } from '../insight/FocusedInsightGraph';
import styles from './InsightTab.module.css';

interface InsightTabProps {
  findings: FindingDto[];
  nodes: GraphNodeDto[];
  edges: GraphEdgeDto[];
  selectedNodeId: string | null;
  onSelectNode: (id: string) => void;
}

const SEV_CLASS: Record<string, string> = { high: 'sevHigh', medium: 'sevMed', low: 'sevLow' };

export function InsightTab({ findings, nodes, edges, selectedNodeId, onSelectNode }: InsightTabProps) {
  return (
    <div className={styles.root}>
      <FocusedInsightGraph
        nodes={nodes}
        edges={edges}
        selectedNodeId={selectedNodeId}
        onSelectNode={onSelectNode}
      />
      {findings.length === 0 ? (
        <p className={styles.empty}>No insights for this node.</p>
      ) : (
        <ul className={styles.list}>
          {findings.map((f) => (
            <li key={f.finding_id} className={styles.item}>
              <div className={styles.head}>
                <span className={`${styles.sev} ${styles[SEV_CLASS[f.severity] ?? 'sevLow']}`}>{f.severity}</span>
                <span className={styles.category}>{f.category}</span>
                <span className={styles.confidence}>{Math.round(f.confidence * 100)}%</span>
              </div>
              <p className={styles.summary}>{f.summary}</p>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
