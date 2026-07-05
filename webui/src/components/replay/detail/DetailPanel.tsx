// webui/src/components/replay/detail/DetailPanel.tsx
import { useState } from 'react';
import { Braces } from 'lucide-react';
import type { SignalDto, ObservedEventDto, LlmRequestP50Dto } from '../../../api/types';
import type { LlmRequestMetrics } from '../stream/llmRequestMetrics';
import type { ToolMetrics } from './toolMetrics';
import { InsightTab } from './InsightTab';
import { RawTab, type RawBlock } from './RawTab';
import styles from './DetailPanel.module.css';

type TabId = 'insight' | 'raw';

interface DetailPanelProps {
  /** The selected ObservedEvent. The panel is fully event-driven — no graph
   *  node. thinking, hook, tool_call, etc. are all just events here. */
  event: ObservedEventDto | null;
  record: unknown;
  signals: SignalDto[];
  /** Tool-execution metrics for a selected `tool_call` (Insight tab), derived
   *  from events by tool_use_id. */
  toolMetrics?: ToolMetrics | null;
  /** Per-response metrics for a selected `assistant_message` / `thinking`
   *  (Insight tab), derived from events by request_id. */
  llmMetrics?: LlmRequestMetrics | null;
  /** Session-wide p50 baselines for the request-metric rows (PR-3 §3d) —
   *  drives the "세션 중앙값의 x.x×" badge in the Insight tab. */
  llmP50?: LlmRequestP50Dto | null;
  /** Source-split blocks for the Raw tab (entity + correlated sources).
   *  Falls back to the single `record` when absent. */
  rawBlocks?: RawBlock[];
  /** The matching tool_result event when the selected event is a tool_call.
   *  Used by WhatSection (① WHAT layer) to show the full tool output. */
  matchedResult?: ObservedEventDto | null;
  /** S7 — jump to an evidence event id from a Signal (Insight tab). */
  onSelectEvent?: (eventId: string) => void;
}

export function DetailPanel({
  event,
  record,
  signals,
  toolMetrics,
  llmMetrics,
  llmP50,
  rawBlocks,
  matchedResult,
  onSelectEvent,
}: DetailPanelProps) {
  const [chosen, setChosen] = useState<TabId | null>(null);
  const active = chosen ?? 'insight';

  if (!event) {
    return <aside className={styles.panel}><p className={styles.empty}>Select an event to inspect it.</p></aside>;
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
  const hasRecord = record != null || (rawBlocks != null && rawBlocks.length > 0);
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
            signals={signals}
            event={event}
            toolMetrics={toolMetrics ?? null}
            llmMetrics={llmMetrics ?? null}
            llmP50={llmP50 ?? null}
            matchedResult={matchedResult ?? null}
            onSelectEvent={onSelectEvent}
          />
        )}
        {active === 'raw' && <RawTab nodeId={event.event_id} record={record} blocks={rawBlocks} />}
      </div>
    </aside>
  );
}
