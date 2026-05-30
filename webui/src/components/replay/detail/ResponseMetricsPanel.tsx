// webui/src/components/replay/detail/ResponseMetricsPanel.tsx
//
// Side-panel view shown when a thinking marker is selected (a nodeless
// selection). The reasoning plaintext is recorded nowhere, so instead of
// (absent) content we surface the honest per-response metrics joined from the
// `claude_code.llm_request` span.
//
// The metric rows themselves (Row + InfoTip + TIPS) live in the neutral
// `metricsRows` module and are reused here — this component only adds the
// thinking-specific frame (heading, "not recorded" note, warn footnote).
import { BrainCog, AlertTriangle } from 'lucide-react';
import type { LlmRequestMetrics } from '../stream/llmRequestMetrics';
import { ResponseMetricsRows, responseWarns } from './metricsRows';
import styles from './ResponseMetricsPanel.module.css';

interface ResponseMetricsPanelProps {
  metrics: LlmRequestMetrics | null;
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
        <ResponseMetricsRows metrics={metrics} />
      )}

      {metrics && responseWarns(metrics) && (
        <p className={styles.warnNote}>
          <AlertTriangle size={12} aria-hidden /> 이 응답은 잘림/재시도/실패 신호가 있습니다.
        </p>
      )}
    </aside>
  );
}
