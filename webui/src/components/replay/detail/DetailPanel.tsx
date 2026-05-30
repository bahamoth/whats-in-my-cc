// webui/src/components/replay/detail/DetailPanel.tsx
import { useState } from 'react';
import { Braces } from 'lucide-react';
import type { FindingDto, GraphNodeDto, GraphEdgeDto } from '../../../api/types';
import type { LlmRequestMetrics } from '../stream/llmRequestMetrics';
import type { ToolMetrics } from './toolMetrics';
import { InsightTab } from './InsightTab';
import { RawTab } from './RawTab';
import { ResponseMetricsPanel } from './ResponseMetricsPanel';
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
  /** True when a thinking marker is selected (which has no graph node): the
   *  panel shows the per-response metrics instead of node detail. */
  thinkingSelected?: boolean;
  /** Per-response metrics for the selected thinking marker (may be null when
   *  the LLM-request span is outside the loaded window). */
  thinkingMetrics?: LlmRequestMetrics | null;
  /** Tool-execution metrics for the selected `tool_call` node (Insight tab). */
  toolMetrics?: ToolMetrics | null;
  /** Per-response metrics for the selected `assistant_message` node (Insight tab). */
  llmMetrics?: LlmRequestMetrics | null;
}

export function DetailPanel({ node, record, findings, thinkingSelected, thinkingMetrics, toolMetrics, llmMetrics }: DetailPanelProps) {
  const [chosen, setChosen] = useState<TabId | null>(null);
  const active = chosen ?? 'insight';

  if (!node) {
    // A thinking marker is selected (no graph node) → show response metrics
    // (the panel handles a null metrics gracefully).
    if (thinkingSelected) {
      return <ResponseMetricsPanel metrics={thinkingMetrics ?? null} />;
    }
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
            node={node}
            toolMetrics={toolMetrics ?? null}
            llmMetrics={llmMetrics ?? null}
          />
        )}
        {active === 'raw' && <RawTab nodeId={node.node_id} record={record} />}
      </div>
    </aside>
  );
}
