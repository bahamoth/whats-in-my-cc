// webui/src/components/replay/detail/DetailPanel.tsx
import { useState } from 'react';
import type { FindingDto, GraphNodeDto } from '../../../api/types';
import { InsightTab } from './InsightTab';
import { DetailTab } from './DetailTab';
import { RawTab } from './RawTab';
import styles from './DetailPanel.module.css';

type TabId = 'insight' | 'detail' | 'raw';

interface DetailPanelProps {
  node: GraphNodeDto | null;
  record: unknown;
  findings: FindingDto[];
  episodePhase: string | null;
}

export function DetailPanel({ node, record, findings, episodePhase }: DetailPanelProps) {
  // null = "follow the default rule"; a value = an explicit user choice that sticks.
  const [chosen, setChosen] = useState<TabId | null>(null);
  const fallback: TabId = findings.length > 0 ? 'insight' : 'detail';
  const active = chosen ?? fallback;

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

  return (
    <aside className={styles.panel}>
      <div className={styles.tabs} role="tablist">
        {tab('insight', 'Insight')}
        {tab('detail', 'Detail')}
        {tab('raw', 'Raw')}
      </div>
      <div className={styles.body} role="tabpanel">
        {active === 'insight' && <InsightTab findings={findings} />}
        {active === 'detail' && <DetailTab node={node} record={record} episodePhase={episodePhase} />}
        {active === 'raw' && <RawTab nodeId={node.node_id} record={record} />}
      </div>
    </aside>
  );
}
