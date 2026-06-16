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
import { useT } from '../../../i18n';
import styles from './ResponseMetricsPanel.module.css';

interface ResponseMetricsPanelProps {
  metrics: LlmRequestMetrics | null;
}

export function ResponseMetricsPanel({ metrics }: ResponseMetricsPanelProps) {
  const t = useT();
  return (
    <aside className={styles.panel} data-testid="response-metrics">
      <div className={styles.head}>
        <BrainCog size={14} aria-hidden className={styles.icon} />
        <span className={styles.title}>{t('detail.response.title')}</span>
      </div>

      <p className={styles.note}>{t('detail.response.note')}</p>

      {!metrics ? (
        <p className={styles.empty}>{t('detail.response.empty')}</p>
      ) : (
        <ResponseMetricsRows metrics={metrics} />
      )}

      {metrics && responseWarns(metrics) && (
        <p className={styles.warnNote}>
          <AlertTriangle size={12} aria-hidden /> {t('detail.response.warn')}
        </p>
      )}
    </aside>
  );
}
