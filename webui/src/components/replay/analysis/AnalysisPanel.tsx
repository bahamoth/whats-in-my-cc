/**
 * AnalysisPanel — session-level behavioral metrics surface.
 *
 * Separate from the replay detail / InsightTab (spec §8.3, 원칙 7).
 * Shows deterministic, fact-only metrics — NO judgment/threshold coloring (spec §6.3).
 * props: { metrics: SessionMetricsDto | null }
 */
import type { SessionMetricsDto } from '../../../api/types';
import styles from './AnalysisPanel.module.css';

interface AnalysisPanelProps {
  metrics: SessionMetricsDto | null;
  /** Forwarded to root div for test selection (e.g. data-testid). */
  'data-testid'?: string;
}

function pct(rate: number): string {
  return Math.round(rate * 100) + '%';
}

export function AnalysisPanel({ metrics, 'data-testid': testId }: AnalysisPanelProps) {
  if (!metrics) {
    return (
      <div className={styles.root} data-testid={testId}>
        <p className={styles.empty}>분석할 지표가 없습니다.</p>
      </div>
    );
  }

  // Detector distribution: sort by count descending for readability.
  const detectorEntries = Object.entries(metrics.detector_firing).sort(
    ([, a], [, b]) => b - a,
  );
  const maxCount = detectorEntries.length > 0
    ? Math.max(...detectorEntries.map(([, c]) => c))
    : 1;

  return (
    <div className={styles.root} data-testid={testId}>
      {/* --- Core metrics table --- */}
      <div>
        <div className={styles.sectionTitle}>세션 지표</div>
        <div className={styles.metricsTable}>
          <div className={styles.metricRow}>
            <span className={styles.metricLabel}>도구 실패율</span>
            <span className={styles.metricCount}>
              {metrics.tool_failure_count}/{metrics.tool_call_total}
            </span>
            <span className={styles.metricRate}>{pct(metrics.tool_failure_rate)}</span>
          </div>
          <div className={styles.metricRow}>
            <span className={styles.metricLabel}>검증 통과율</span>
            <span className={styles.metricCount}>
              {metrics.verification_passed}/{metrics.verification_total}
            </span>
            <span className={styles.metricRate}>{pct(metrics.verification_pass_rate)}</span>
          </div>
          <div className={styles.metricRow}>
            <span className={styles.metricLabel}>캐시 히트율</span>
            <span className={styles.metricCount} />
            <span className={styles.metricRate}>{pct(metrics.cache_hit_ratio)}</span>
          </div>
          <div className={styles.metricRow}>
            <span className={styles.metricLabel}>Context bloat 횟수</span>
            <span className={styles.metricCount}>{metrics.context_bloat_count}</span>
            <span className={styles.metricRate} />
          </div>
        </div>
      </div>

      {/* --- Detector signal distribution --- */}
      <div className={styles.detectorSection}>
        <div className={styles.sectionTitle}>Detector 신호 분포</div>
        {detectorEntries.length === 0 ? (
          <p className={styles.noDetectors}>감지된 신호 없음</p>
        ) : (
          <div className={styles.detectorList}>
            {detectorEntries.map(([detector, count]) => (
              <div key={detector} className={styles.detectorRow}>
                <span className={styles.detectorName}>{detector}</span>
                <div className={styles.detectorBarTrack}>
                  <div
                    className={styles.bar}
                    style={{ width: `${Math.round((count / maxCount) * 100)}%` }}
                    aria-label={`${detector}: ${count}`}
                  />
                </div>
                <span className={styles.detectorCount}>{count}</span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
