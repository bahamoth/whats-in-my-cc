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
import type { LlmRequestMetrics } from '../stream/llmRequestMetrics';
import { formatDuration, formatTokens } from '../stream/llmRequestMetrics';
import { InfoTip } from '../insight-strip/InfoTip';
import styles from './metricsRows.module.css';

// Plain-language explanations. Token/cache rows are easy to misread; the tool
// metrics (decision source, byte sizes) likewise need their meaning spelled out.
export const TIPS: Record<string, string> = {
  '출력 토큰': '이 응답에서 생성된 토큰 수입니다. 추론(thinking) 토큰도 여기에 포함됩니다 — 모델이 만들어낸 분량.',
  '입력 토큰': '이번 요청에 새로 전달된(캐시되지 않은) 입력 토큰 수입니다. 컨텍스트 대부분은 보통 캐시 읽기로 재사용됩니다.',
  '캐시 읽기': '프롬프트 캐시에서 재사용한 토큰 수입니다. 클수록 컨텍스트 대부분을 캐시로 재활용 — 비용·지연을 줄입니다.',
  '캐시 생성': '이번에 새로 캐시에 기록한 토큰 수입니다. 다음 요청부터 캐시 읽기로 재사용됩니다.',
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

/** The per-response metric rows, shared by the assistant-message node path and
 *  the (nodeless) thinking-marker `ResponseMetricsPanel`. */
export function ResponseMetricsRows({ metrics }: { metrics: LlmRequestMetrics }) {
  return (
    <div className={styles.grid}>
      <Row label="소요 시간" value={formatDuration(metrics.durationMs) ?? '—'} />
      <Row label="첫 토큰까지(ttft)" value={formatDuration(metrics.ttftMs) ?? '—'} />
      <Row label="출력 토큰" value={formatTokens(metrics.outputTokens) ?? '—'} />
      <Row label="입력 토큰" value={formatTokens(metrics.inputTokens) ?? '—'} />
      <Row label="캐시 읽기" value={formatTokens(metrics.cacheReadTokens) ?? '—'} />
      <Row label="캐시 생성" value={formatTokens(metrics.cacheCreationTokens) ?? '—'} />
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
    </div>
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
