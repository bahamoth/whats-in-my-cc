/**
 * AnalysisPanel — session-level behavioral metrics surface.
 *
 * Separate from the replay detail / InsightTab (spec §8.3, 원칙 7).
 * Shows deterministic, fact-only metrics — NO judgment/threshold coloring (spec §6.3).
 * props: { metrics: SessionMetricsDto | null }
 */
import { useMemo, useState } from 'react';
import type { SessionMetricsDto, SignalDto, EvidenceRef } from '../../../api/types';
import styles from './AnalysisPanel.module.css';

interface AnalysisPanelProps {
  metrics: SessionMetricsDto | null;
  /** Session signals — drive the detector drill-down (grouped by detector). */
  signals?: SignalDto[];
  /** Select/deep-link an evidence event when a drilled signal is clicked. */
  onSelectEvent?: (eventId: string) => void;
  /** Forwarded to root div for test selection (e.g. data-testid). */
  'data-testid'?: string;
}

function pct(rate: number): string {
  return Math.round(rate * 100) + '%';
}

/** First evidence event id of a signal (refs are bare-string or {event_id}). */
function firstEvidenceEventId(s: SignalDto): string | null {
  for (const ref of s.evidence_refs) {
    if (typeof ref === 'string') return ref;
    if (ref && typeof (ref as Exclude<EvidenceRef, string>).event_id === 'string') {
      return (ref as Exclude<EvidenceRef, string>).event_id as string;
    }
  }
  return null;
}

/** One-line, fact-only label for a drilled signal (no judgment words). */
function signalLabel(s: SignalDto): string {
  if (s.detector === 're_read') {
    const fp = s.facts.file_path;
    const rc = s.facts.read_count;
    if (typeof fp === 'string') return `${fp} · ${rc ?? '?'}회`;
  }
  if (s.detector === 'tool_failure') {
    const tool = s.facts.tool_name;
    const excerpt = s.facts.error_excerpt;
    if (typeof tool === 'string') {
      return typeof excerpt === 'string' ? `${tool} · ${excerpt}` : tool;
    }
  }
  return s.summary;
}

export function AnalysisPanel({
  metrics,
  signals,
  onSelectEvent,
  'data-testid': testId,
}: AnalysisPanelProps) {
  const [expanded, setExpanded] = useState<string | null>(null);

  // Group signals by detector for the drill-down under each bar.
  const signalsByDetector = useMemo(() => {
    const m: Record<string, SignalDto[]> = {};
    for (const s of signals ?? []) (m[s.detector] ??= []).push(s);
    return m;
  }, [signals]);

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
          {/* 도구 실패: rate는 count에서 계산 */}
          <div className={styles.metricRow}>
            <span className={styles.metricLabel}>도구 실패</span>
            <span className={styles.metricCount}>
              {metrics.tool_failure_count}/{metrics.tool_call_total}
            </span>
            <span className={styles.metricRate}>
              {metrics.tool_call_total > 0
                ? pct(metrics.tool_failure_count / metrics.tool_call_total)
                : '—'}
            </span>
          </div>
          {/* 검증: 분모는 measured(passed+failed), unknown 별도 노출 */}
          <div className={styles.metricRow}>
            <span className={styles.metricLabel}>검증 통과 (측정분)</span>
            <span className={styles.metricCount}>
              {metrics.verification_passed}/{metrics.verification_passed + metrics.verification_failed}
              {metrics.verification_unknown > 0 ? ` · 미측정 ${metrics.verification_unknown}` : ''}
            </span>
            <span className={styles.metricRate}>
              {metrics.verification_passed + metrics.verification_failed > 0
                ? pct(metrics.verification_passed / (metrics.verification_passed + metrics.verification_failed))
                : '측정 없음'}
            </span>
          </div>
          {/* context bloat */}
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
            {detectorEntries.map(([detector, count]) => {
              const sigs = signalsByDetector[detector] ?? [];
              const canExpand = sigs.length > 0;
              const isOpen = expanded === detector;
              return (
                <div key={detector}>
                  <button
                    type="button"
                    className={styles.detectorRow}
                    aria-expanded={canExpand ? isOpen : undefined}
                    disabled={!canExpand}
                    onClick={() => canExpand && setExpanded(isOpen ? null : detector)}
                  >
                    <span className={styles.detectorName}>{detector}</span>
                    <div className={styles.detectorBarTrack}>
                      <div
                        className={styles.bar}
                        style={{ width: `${Math.round((count / maxCount) * 100)}%` }}
                        aria-label={`${detector}: ${count}`}
                      />
                    </div>
                    <span className={styles.detectorCount}>{count}</span>
                  </button>
                  {isOpen && (
                    <ul className={styles.signalList}>
                      {sigs.map((s) => {
                        const eid = firstEvidenceEventId(s);
                        return (
                          <li key={s.signal_id}>
                            <button
                              type="button"
                              className={styles.signalItem}
                              disabled={!eid || !onSelectEvent}
                              onClick={() => eid && onSelectEvent?.(eid)}
                            >
                              {signalLabel(s)}
                            </button>
                          </li>
                        );
                      })}
                    </ul>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
