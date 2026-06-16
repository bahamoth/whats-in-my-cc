// webui/src/components/replay/detail/metricsRows.tsx
//
// Neutral, shared metric-row primitives used by BOTH EntityMetricsPanel (the
// per-node Insight surface) and ResponseMetricsPanel (the nodeless
// thinking-marker surface). Lives in its own module so neither panel imports
// from the other.
//
//   - `Row`                 — a single label/value row with an optional ⓘ tip
//   - `TIPS`                — plain-language explanations keyed by row label
//   - `ResponseMetricsRows` — the full per-response (LLM request) metric grid
//   - `responseWarns`       — truncation / retry / failure signal
//   - `formatBytes`         — shared byte formatter
import type { ReactNode } from 'react';
import type { LlmRequestMetrics } from '../stream/llmRequestMetrics';
import {
  formatDuration,
  formatQuerySource,
  formatThroughput,
  formatTokens,
  formatUsd,
} from '../stream/llmRequestMetrics';
import { InfoTip } from '../insight-strip/InfoTip';
import { ProvenanceBadge } from '../insight-strip/ProvenanceBadge';
import type { Provenance } from '../insight-strip/provenance';
import { useT, type MessageKey } from '../../../i18n';
import styles from './metricsRows.module.css';

// l10n — a row's label and (optional) explanatory tip are both catalog keys;
// Row resolves them via t(). The tip text lives under `metric.tip.*` and the
// label under `metric.label.*`. Token/cache rows are easy to misread; the tool
// metrics (decision source, byte sizes) likewise need their meaning spelled out.
export function Row({
  labelKey,
  tipKey,
  value,
  warn = false,
}: {
  labelKey: MessageKey;
  tipKey?: MessageKey;
  value: string;
  warn?: boolean;
}) {
  const t = useT();
  const label = t(labelKey);
  return (
    <div className={styles.row} data-warn={String(warn)}>
      <span className={styles.k}>
        {label}
        {tipKey && <InfoTip label={label} text={t(tipKey)} />}
      </span>
      <span className={styles.v}>{value}</span>
    </div>
  );
}

export function formatBytes(n: number | null): string {
  if (n == null) return '—';
  if (n >= 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${n} B`;
}

/**
 * S7 (UX 재설계) — a HOW subheading + provenance pill wrapping a group of
 * metric rows. The provenance pill is rendered at the *group* level (not per
 * row): every row in a group shares the same source (e.g. the OTel
 * `llm_request` span), so 12 identical "측정" pills would be noise. The pill
 * still satisfies "값 옆 상시 표기" — it states the trust level of the
 * subheading's values once. (Deviation from a literal per-row reading of the
 * design spec §6.6; recorded in implementation-notes.)
 */
export function MetricGroup({
  title,
  provenance,
  children,
}: {
  title: string;
  provenance?: Provenance;
  children: ReactNode;
}) {
  return (
    <div className={styles.group}>
      <div className={styles.groupHead}>
        <span className={styles.groupTitle}>{title}</span>
        {provenance && <ProvenanceBadge provenance={provenance} />}
      </div>
      <div className={styles.grid}>{children}</div>
    </div>
  );
}

/** The per-response metric rows, shared by the assistant-message node path and
 *  the (nodeless) thinking-marker `ResponseMetricsPanel`. S7 — grouped under
 *  LLM 동작 / 토큰 / 비용 subheadings, each with a provenance pill. All three
 *  groups are `measured`: timing/stop_reason/attempts come from the OTel
 *  `llm_request` span, tokens from the same span, and cost from the reported
 *  `api_request_log` value. */
export function ResponseMetricsRows({ metrics }: { metrics: LlmRequestMetrics }) {
  const t = useT();
  return (
    <>
      <MetricGroup title={t('metric.group.llmActivity')} provenance="measured">
        <Row labelKey="metric.label.duration" value={formatDuration(metrics.durationMs) ?? '—'} />
        <Row labelKey="metric.label.ttft" value={formatDuration(metrics.ttftMs) ?? '—'} />
        <Row
          labelKey="metric.label.stopReason"
          value={metrics.stopReason ?? '—'}
          warn={metrics.stopReason === 'max_tokens'}
        />
        <Row
          labelKey="metric.label.attempts"
          value={metrics.attempt != null ? t('metric.attemptCount', metrics.attempt) : '—'}
          warn={metrics.attempt != null && metrics.attempt > 1}
        />
        <Row
          labelKey="metric.label.success"
          value={metrics.success == null ? '—' : metrics.success ? t('metric.yes') : t('metric.no')}
          warn={metrics.success === false}
        />
        <Row labelKey="metric.label.model" value={metrics.model ?? '—'} />
        {formatQuerySource(metrics.querySource) && (
          <Row
            labelKey="metric.label.querySource"
            tipKey="metric.tip.querySource"
            value={formatQuerySource(metrics.querySource)!}
          />
        )}
      </MetricGroup>

      <MetricGroup title={t('metric.group.tokens')} provenance="measured">
        <Row
          labelKey="metric.label.outputTokens"
          tipKey="metric.tip.outputTokens"
          value={formatTokens(metrics.outputTokens) ?? '—'}
        />
        {formatThroughput(metrics.outputTokens, metrics.durationMs) && (
          <Row
            labelKey="metric.label.outputSpeed"
            tipKey="metric.tip.outputSpeed"
            value={formatThroughput(metrics.outputTokens, metrics.durationMs)!}
          />
        )}
        <Row
          labelKey="metric.label.inputTokens"
          tipKey="metric.tip.inputTokens"
          value={formatTokens(metrics.inputTokens) ?? '—'}
        />
        <Row
          labelKey="metric.label.cacheReads"
          tipKey="metric.tip.cacheReads"
          value={formatTokens(metrics.cacheReadTokens) ?? '—'}
        />
        <Row
          labelKey="metric.label.cacheCreation"
          tipKey="metric.tip.cacheCreation"
          value={formatTokens(metrics.cacheCreationTokens) ?? '—'}
        />
      </MetricGroup>

      {metrics.costUsd != null && (
        <MetricGroup title={t('metric.group.cost')} provenance="measured">
          <Row
            labelKey="metric.label.billedCost"
            tipKey="metric.tip.billedCost"
            value={formatUsd(metrics.costUsd) ?? '—'}
          />
        </MetricGroup>
      )}
    </>
  );
}

/** True when the response metrics carry a truncation / retry / failure signal. */
export function responseWarns(metrics: LlmRequestMetrics): boolean {
  return (
    metrics.stopReason === 'max_tokens' ||
    metrics.success === false ||
    (metrics.attempt ?? 0) > 1
  );
}
