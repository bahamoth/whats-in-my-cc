// webui/src/components/replay/detail/EntityMetricsPanel.tsx
//
// Metrics-led, per-node Insight view (UX option A): when a graph node is
// selected, show that entity's COLLECTED metrics with plain-language meaning
// (ⓘ tooltips), kind-dependent.
//   - tool_call          → tool execution metrics from `ToolMetrics`
//                          (log_record facet attributes; see toolMetrics.ts)
//   - assistant_message  → per-response (LLM request) metrics from
//     | thinking            `LlmRequestMetrics` (claude_code.llm_request span)
//   - other kinds        → no metrics (empty)
//
// DRY: the response-row rendering (Row + InfoTip + TIPS) lives in the neutral
// `metricsRows` module and is shared with `ResponseMetricsPanel`. This panel
// only adds the tool-specific rows + the kind switch.
import { AlertTriangle } from 'lucide-react';
import type { LlmRequestMetrics } from '../stream/llmRequestMetrics';
import { formatDuration } from '../stream/llmRequestMetrics';
import type { ToolMetrics } from './toolMetrics';
import { Row, ResponseMetricsRows, responseWarns, formatBytes } from './metricsRows';
import rows from './metricsRows.module.css';
import styles from './EntityMetricsPanel.module.css';

interface EntityMetricsPanelProps {
  kind: string;
  toolMetrics: ToolMetrics | null;
  llmMetrics: LlmRequestMetrics | null;
}

function Uncollected() {
  return <span className={styles.uncollected}>지표 미수집</span>;
}

function ToolMetricsRows({ m }: { m: ToolMetrics }) {
  const allNull =
    m.durationMs == null &&
    m.success == null &&
    m.decisionSource == null &&
    m.decisionType == null &&
    m.inputBytes == null &&
    m.resultBytes == null &&
    m.sequence == null;
  if (allNull) return <Uncollected />;

  const decisionValue =
    m.decisionType || m.decisionSource
      ? [m.decisionType, m.decisionSource].filter(Boolean).join(' · ')
      : '—';

  const sizeValue =
    m.inputBytes == null && m.resultBytes == null
      ? '—'
      : `${formatBytes(m.inputBytes)} → ${formatBytes(m.resultBytes)}`;

  return (
    <div className={rows.grid}>
      <Row label="소요 시간" value={formatDuration(m.durationMs) ?? '—'} />
      <Row
        label="결과"
        value={m.success == null ? '—' : m.success ? 'ok' : 'error'}
        warn={m.success === false}
      />
      <Row label="결정 출처" value={decisionValue} />
      <Row label="입력/결과 크기" value={sizeValue} />
      <Row label="순서" value={m.sequence != null ? `#${m.sequence}` : '—'} />
    </div>
  );
}

export function EntityMetricsPanel({ kind, toolMetrics, llmMetrics }: EntityMetricsPanelProps) {
  if (kind === 'tool_call') {
    return (
      <div className={styles.wrap} data-testid="entity-metrics" data-kind={kind}>
        {toolMetrics ? <ToolMetricsRows m={toolMetrics} /> : <Uncollected />}
      </div>
    );
  }

  // 'thinking' is not a graph node today; kept for forward-compat — reached only if thinking nodes are ever materialized
  if (kind === 'assistant_message' || kind === 'thinking') {
    return (
      <div className={styles.wrap} data-testid="entity-metrics" data-kind={kind}>
        {llmMetrics ? <ResponseMetricsRows metrics={llmMetrics} /> : <Uncollected />}
        {llmMetrics && responseWarns(llmMetrics) && (
          <p className={styles.warnNote}>
            <AlertTriangle size={12} aria-hidden /> 이 응답은 잘림/재시도/실패 신호가 있습니다.
          </p>
        )}
      </div>
    );
  }

  // Other kinds carry no per-entity metrics — render nothing.
  return null;
}
