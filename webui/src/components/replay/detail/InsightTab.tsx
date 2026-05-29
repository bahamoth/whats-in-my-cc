// webui/src/components/replay/detail/InsightTab.tsx
import type { FindingDto, GraphNodeDto, GraphEdgeDto } from '../../../api/types';
import { FocusedInsightGraph } from '../insight/FocusedInsightGraph';
import { NodeDetail } from './NodeDetail';
import styles from './InsightTab.module.css';

interface InsightTabProps {
  findings: FindingDto[];
  nodes: GraphNodeDto[];
  edges: GraphEdgeDto[];
  selectedNodeId: string | null;
  onSelectNode: (id: string) => void;
  node: GraphNodeDto | null;
  record: unknown;
  episodePhase: string | null;
}

export function InsightTab({ findings, nodes, edges, selectedNodeId, onSelectNode, node, record, episodePhase }: InsightTabProps) {
  return (
    <div className={styles.root}>
      <FocusedInsightGraph
        nodes={nodes}
        edges={edges}
        selectedNodeId={selectedNodeId}
        onSelectNode={onSelectNode}
      />
      {node && (
        <NodeDetail node={node} record={record} episodePhase={episodePhase} findings={findings} />
      )}
      {!node && findings.length === 0 && (
        <p className={styles.empty}>No insights for this node.</p>
      )}
    </div>
  );
}
