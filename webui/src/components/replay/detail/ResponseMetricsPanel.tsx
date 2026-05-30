// webui/src/components/replay/detail/ResponseMetricsPanel.tsx
//
// Side-panel view shown when a thinking marker is selected. The reasoning
// plaintext is recorded nowhere, so instead of (absent) content we surface the
// honest per-response metrics joined from the `claude_code.llm_request` span.
import { BrainCog, AlertTriangle } from 'lucide-react';
import type { LlmRequestMetrics } from '../stream/llmRequestMetrics';
import { formatDuration, formatTokens } from '../stream/llmRequestMetrics';
import { InfoTip } from '../insight-strip/InfoTip';
import styles from './ResponseMetricsPanel.module.css';

interface ResponseMetricsPanelProps {
  metrics: LlmRequestMetrics | null;
}

// Plain-language explanations for the token / cache metrics, which are easy to
// misread (output includes thinking tokens; "input" excludes cached context).
const TIPS: Record<string, string> = {
  '출력 토큰': '이 응답에서 생성된 토큰 수입니다. 추론(thinking) 토큰도 여기에 포함됩니다 — 모델이 만들어낸 분량.',
  '입력 토큰': '이번 요청에 새로 전달된(캐시되지 않은) 입력 토큰 수입니다. 컨텍스트 대부분은 보통 캐시 읽기로 재사용됩니다.',
  '캐시 읽기': '프롬프트 캐시에서 재사용한 토큰 수입니다. 클수록 컨텍스트 대부분을 캐시로 재활용 — 비용·지연을 줄입니다.',
  '캐시 생성': '이번에 새로 캐시에 기록한 토큰 수입니다. 다음 요청부터 캐시 읽기로 재사용됩니다.',
};

function Row({ label, value, warn = false }: { label: string; value: string; warn?: boolean }) {
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

export function ResponseMetricsPanel({ metrics }: ResponseMetricsPanelProps) {
  return (
    <aside className={styles.panel} data-testid="response-metrics">
      <div className={styles.head}>
        <BrainCog size={14} aria-hidden className={styles.icon} />
        <span className={styles.title}>추론 · 응답 지표</span>
      </div>

      <p className={styles.note}>
        추론 내용은 transcript에 기록되지 않습니다(암호화된 signature만 존재).
        아래는 이 응답(LLM request)의 실측 지표입니다.
      </p>

      {!metrics ? (
        <p className={styles.empty}>이 응답의 지표를 현재 윈도우에서 찾지 못했습니다.</p>
      ) : (
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
      )}

      {metrics && (metrics.stopReason === 'max_tokens' || metrics.success === false || (metrics.attempt ?? 0) > 1) && (
        <p className={styles.warnNote}>
          <AlertTriangle size={12} aria-hidden /> 이 응답은 잘림/재시도/실패 신호가 있습니다.
        </p>
      )}
    </aside>
  );
}
