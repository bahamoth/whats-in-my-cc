// webui/src/components/replay/detail/DetailPanel.tsx
import { useState } from 'react';
import { Braces } from 'lucide-react';
import type { FindingDto, GraphNodeDto, GraphEdgeDto } from '../../../api/types';
import { InsightTab } from './InsightTab';
import { RawTab } from './RawTab';
import styles from './DetailPanel.module.css';

type TabId = 'insight' | 'raw';

interface DetailPanelProps {
  node: GraphNodeDto | null;
  record: unknown;
  findings: FindingDto[];
  episodePhase: string | null;
  nodes: GraphNodeDto[];
  edges: GraphEdgeDto[];
  onSelectNode: (id: string) => void;
}

export function DetailPanel({ node, record, findings, episodePhase, nodes, edges, onSelectNode }: DetailPanelProps) {
  const [chosen, setChosen] = useState<TabId | null>(null);
  const active = chosen ?? 'insight';

  if (!node) {
    return <aside className={styles.panel}><p className={styles.empty}>Select a node to inspect it.</p></aside>;
  }

  const tab = (id: TabId, label: string) => (
    <button
      type="button"
      role="tab"
      aria-selected={active === id}
      className={`${styles.tab} ${active === id ? styles.active : ''}`}
      onClick={() => setChosen(id)}
    >
      {label}
    </button>
  );

  // The Raw tab is secondary to Insight, so the raw source record was easy to
  // miss (#2). Mark it with a braces glyph and, when a record is loaded but the
  // tab isn't open, an accent dot to invite the click.
  const hasRecord = record != null;
  const rawTab = (
    <button
      type="button"
      role="tab"
      aria-selected={active === 'raw'}
      data-has-record={hasRecord ? 'true' : 'false'}
      className={`${styles.tab} ${active === 'raw' ? styles.active : ''} ${hasRecord && active !== 'raw' ? styles.tabEmphasis : ''}`}
      onClick={() => setChosen('raw')}
    >
      <Braces size={12} aria-hidden />
      Raw
      {hasRecord && active !== 'raw' && <span className={styles.tabDot} aria-hidden />}
    </button>
  );

  return (
    <aside className={styles.panel}>
      <div className={styles.tabs} role="tablist">
        {tab('insight', 'Insight')}
        {rawTab}
      </div>
      <div className={styles.body} role="tabpanel">
        {active === 'insight' && (
          <InsightTab
            findings={findings}
            nodes={nodes}
            edges={edges}
            selectedNodeId={node.node_id}
            onSelectNode={onSelectNode}
            node={node}
            record={record}
            episodePhase={episodePhase}
          />
        )}
        {active === 'raw' && <RawTab nodeId={node.node_id} record={record} />}
      </div>
    </aside>
  );
}
