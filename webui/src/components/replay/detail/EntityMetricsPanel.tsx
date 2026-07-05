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
import { hookFacet } from '../stream/hookFacet';
import type { ToolMetrics } from './toolMetrics';
import { Row, MetricGroup, ResponseMetricsRows, responseWarns, formatBytes } from './metricsRows';
import type { LlmRequestP50Dto } from '../../../api/types';
import { useT } from '../../../i18n';
import styles from './EntityMetricsPanel.module.css';

interface EntityMetricsPanelProps {
  kind: string;
  toolMetrics: ToolMetrics | null;
  llmMetrics: LlmRequestMetrics | null;
  /** The selected node's raw payload — used for kinds whose metrics live in the
   *  payload itself (hook_event: exitCode / durationMs / command). */
  payload?: unknown;
  /** Session-wide p50 baselines for the request-metric rows (PR-3 §3d) — used
   *  to render a "세션 중앙값의 x.x×" badge next to duration/ttft/tokens/cost. */
  llmP50?: LlmRequestP50Dto | null;
}

function Uncollected() {
  const t = useT();
  return <span className={styles.uncollected}>{t('metric.uncollected')}</span>;
}

function ToolMetricsRows({ m }: { m: ToolMetrics }) {
  const t = useT();
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
    <MetricGroup title={t('metric.group.toolExec')} provenance="measured">
      <Row labelKey="metric.label.duration" value={formatDuration(m.durationMs) ?? '—'} />
      <Row
        labelKey="metric.label.result"
        value={m.success == null ? '—' : m.success ? 'ok' : 'error'}
        warn={m.success === false}
      />
      <Row labelKey="metric.label.decisionSource" tipKey="metric.tip.decisionSource" value={decisionValue} />
      <Row labelKey="metric.label.ioSize" tipKey="metric.tip.ioSize" value={sizeValue} />
      <Row labelKey="metric.label.sequence" value={m.sequence != null ? `#${m.sequence}` : '—'} />
    </MetricGroup>
  );
}

function HookMetricsRows({ payload }: { payload: unknown }) {
  const t = useT();
  const h = hookFacet(payload);
  const allNull =
    h.durationMs == null && h.success == null && h.command == null && h.hookEvent == null;
  if (allNull) return <Uncollected />;

  return (
    <MetricGroup title={t('metric.group.hookExec')} provenance="measured">
      <Row labelKey="metric.label.hookEvent" value={h.hookEvent ?? '—'} />
      <Row labelKey="metric.label.duration" value={formatDuration(h.durationMs) ?? '—'} />
      <Row
        labelKey="metric.label.result"
        value={h.success == null ? '—' : h.success ? 'ok' : `error (exit ${h.exitCode})`}
        warn={h.success === false}
      />
      <Row labelKey="metric.label.command" value={h.command ?? '—'} />
    </MetricGroup>
  );
}

export function EntityMetricsPanel({
  kind,
  toolMetrics,
  llmMetrics,
  payload,
  llmP50 = null,
}: EntityMetricsPanelProps) {
  const t = useT();
  if (kind === 'hook_event') {
    return (
      <div className={styles.wrap} data-testid="entity-metrics" data-kind={kind}>
        <HookMetricsRows payload={payload} />
      </div>
    );
  }

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
        {llmMetrics ? <ResponseMetricsRows metrics={llmMetrics} p50={llmP50} /> : <Uncollected />}
        {llmMetrics && responseWarns(llmMetrics) && (
          <p className={styles.warnNote}>
            <AlertTriangle size={12} aria-hidden /> {t('detail.response.warn')}
          </p>
        )}
      </div>
    );
  }

  // Other kinds carry no per-entity metrics — render nothing.
  return null;
}
