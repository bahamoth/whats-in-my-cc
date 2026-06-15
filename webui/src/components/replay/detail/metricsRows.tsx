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
import styles from './metricsRows.module.css';

// Plain-language explanations. Token/cache rows are easy to misread; the tool
// metrics (decision source, byte sizes) likewise need their meaning spelled out.
export const TIPS: Record<string, string> = {
  '출력 토큰': '이 응답에서 생성된 토큰 수입니다. 추론(thinking) 토큰도 여기에 포함됩니다 — 모델이 만들어낸 분량.',
  '입력 토큰': '이번 요청에 새로 전달된(캐시되지 않은) 입력 토큰 수입니다. 컨텍스트 대부분은 보통 캐시 읽기로 재사용됩니다.',
  '캐시 읽기': '프롬프트 캐시에서 재사용한 토큰 수입니다. 클수록 컨텍스트 대부분을 캐시로 재활용 — 비용·지연을 줄입니다.',
  '캐시 생성': '이번에 새로 캐시에 기록한 토큰 수입니다. 다음 요청부터 캐시 읽기로 재사용됩니다.',
  '청구 비용': '이 요청의 실측 비용(USD)입니다. Claude Code가 보고한 값(api_request_log)으로, 토큰×공개요금 추정과는 다릅니다.',
  '출력 속도': '생성 처리량입니다 — 출력 토큰 ÷ 요청 소요 시간(초). 추론 토큰도 출력에 포함됩니다.',
  '요청 출처': '이 요청을 보낸 주체입니다. 메인 스레드(사용자 대화) 또는 서브에이전트(general-purpose·Explore 등) — 누가 호출했는지.',
  '결정 출처': '이 도구 실행이 허용된 경위입니다. config = 설정에 의해 자동 허용, user = 사용자가 직접 승인 등 — 권한 결정의 출처.',
  '입력/결과 크기': '도구에 전달한 입력과 도구가 반환한 결과의 크기(바이트)입니다. 결과가 클수록 컨텍스트를 많이 차지합니다.',
};

export function Row({ label, value, warn = false }: { label: string; value: string; warn?: boolean }) {
  const tip = TIPS[label];
  return (
    <div className={styles.row} data-warn={String(warn)}>
      <span className={styles.k}>
        {label}
        {tip && <InfoTip label={label} text={tip} />}
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
  return (
    <>
      <MetricGroup title="LLM 동작" provenance="measured">
        <Row label="소요 시간" value={formatDuration(metrics.durationMs) ?? '—'} />
        <Row label="첫 토큰까지(ttft)" value={formatDuration(metrics.ttftMs) ?? '—'} />
        <Row
          label="종료 사유"
          value={metrics.stopReason ?? '—'}
          warn={metrics.stopReason === 'max_tokens'}
        />
        <Row
          label="시도"
          value={metrics.attempt != null ? `${metrics.attempt}회` : '—'}
          warn={metrics.attempt != null && metrics.attempt > 1}
        />
        <Row
          label="성공"
          value={metrics.success == null ? '—' : metrics.success ? '예' : '아니오'}
          warn={metrics.success === false}
        />
        <Row label="모델" value={metrics.model ?? '—'} />
        {formatQuerySource(metrics.querySource) && (
          <Row label="요청 출처" value={formatQuerySource(metrics.querySource)!} />
        )}
      </MetricGroup>

      <MetricGroup title="토큰" provenance="measured">
        <Row label="출력 토큰" value={formatTokens(metrics.outputTokens) ?? '—'} />
        {formatThroughput(metrics.outputTokens, metrics.durationMs) && (
          <Row label="출력 속도" value={formatThroughput(metrics.outputTokens, metrics.durationMs)!} />
        )}
        <Row label="입력 토큰" value={formatTokens(metrics.inputTokens) ?? '—'} />
        <Row label="캐시 읽기" value={formatTokens(metrics.cacheReadTokens) ?? '—'} />
        <Row label="캐시 생성" value={formatTokens(metrics.cacheCreationTokens) ?? '—'} />
      </MetricGroup>

      {metrics.costUsd != null && (
        <MetricGroup title="비용" provenance="measured">
          <Row label="청구 비용" value={formatUsd(metrics.costUsd) ?? '—'} />
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
