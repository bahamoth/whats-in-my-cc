/**
 * AnalysisPanel — session-level behavioral metrics surface.
 *
 * Separate from the replay detail / InsightTab (spec §8.3, 원칙 7).
 * Shows deterministic, fact-only metrics — NO judgment/threshold coloring (spec §6.3).
 * props: { metrics: SessionMetricsDto | null }
 */
import { useMemo, useState } from 'react';
import type { SessionMetricsDto, SignalDto, EvidenceRef, VerificationRunDto } from '../../../api/types';
import { useT, type TFunction } from '../../../i18n';
import { InfoTip } from '../insight-strip/InfoTip';
import { RhythmStrip } from '../../dash/RhythmStrip';
import styles from './AnalysisPanel.module.css';

interface AnalysisPanelProps {
  metrics: SessionMetricsDto | null;
  /** Session signals — drive the detector drill-down (grouped by detector). */
  signals?: SignalDto[];
  /** §3b 검증 리듬 — 세션 run 목록(기존 /verification-runs 재사용). */
  verificationRuns?: VerificationRunDto[];
  /** 세션 시간 범위(first/last observed_at) — pct 분모. */
  sessionSpan?: { first: string; last: string } | null;
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
function signalLabel(s: SignalDto, t: TFunction): string {
  if (s.detector === 're_read') {
    const fp = s.facts.file_path;
    const rc = s.facts.read_count;
    if (typeof fp === 'string') {
      return t('analysis.reReadLabel', { file: fp, count: String(rc ?? '?') });
    }
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

/** 백엔드 rhythm pct와 동일 정의: (started_at−first)/(last−first)×100,
 *  소수 1자리, span 0 → 50 (SSOT: tests/api_verification_summary.rs). */
function rhythmRunsOf(
  runs: VerificationRunDto[],
  span: { first: string; last: string },
): Array<{ pct: number; status: string; eventId: string }> {
  const a = Date.parse(span.first);
  const b = Date.parse(span.last);
  if (Number.isNaN(a) || Number.isNaN(b)) return [];
  const ms = b - a;
  return runs
    .filter((r) => !Number.isNaN(Date.parse(r.started_at)))
    .sort((x, y) => x.started_at.localeCompare(y.started_at))
    .map((r) => ({
      pct:
        ms > 0
          ? Math.min(100, Math.max(0, Math.round(((Date.parse(r.started_at) - a) / ms) * 1000) / 10))
          : 50,
      status: r.status,
      eventId: r.trigger_event_id,
    }));
}

export function AnalysisPanel({
  metrics,
  signals,
  verificationRuns,
  sessionSpan,
  onSelectEvent,
  'data-testid': testId,
}: AnalysisPanelProps) {
  const t = useT();
  const [expanded, setExpanded] = useState<string | null>(null);

  // Group signals by detector for the drill-down under each bar.
  const signalsByDetector = useMemo(() => {
    const m: Record<string, SignalDto[]> = {};
    for (const s of signals ?? []) (m[s.detector] ??= []).push(s);
    return m;
  }, [signals]);

  const rhythmRuns = useMemo(
    () => (verificationRuns && sessionSpan ? rhythmRunsOf(verificationRuns, sessionSpan) : []),
    [verificationRuns, sessionSpan],
  );

  if (!metrics) {
    return (
      <div className={styles.root} data-testid={testId}>
        <p className={styles.empty}>{t('analysis.empty')}</p>
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
        <div className={styles.sectionTitle}>{t('analysis.sessionMetrics')}</div>
        <div className={styles.metricsTable}>
          {/* 도구 실패: rate는 count에서 계산 */}
          <div className={styles.metricRow}>
            <span className={styles.metricLabel}>{t('analysis.toolFailures')}</span>
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
            <span className={styles.metricLabel}>{t('analysis.verificationPassed')}</span>
            <span className={styles.metricCount}>
              {metrics.verification_passed}/{metrics.verification_passed + metrics.verification_failed}
              {metrics.verification_unknown > 0
                ? t('analysis.unmeasuredInline', metrics.verification_unknown)
                : ''}
            </span>
            <span className={styles.metricRate}>
              {metrics.verification_passed + metrics.verification_failed > 0
                ? pct(metrics.verification_passed / (metrics.verification_passed + metrics.verification_failed))
                : t('analysis.noMeasurement')}
            </span>
          </div>
          {/* context bloat */}
          <div className={styles.metricRow}>
            <span className={styles.metricLabel}>{t('analysis.contextBloatCount')}</span>
            <span className={styles.metricCount}>{metrics.context_bloat_count}</span>
            <span className={styles.metricRate} />
          </div>
        </div>
      </div>

      {/* --- 검증 실행 리듬 (§3b) — 진행률은 시간 기준(대시보드 rhythm과 동일) --- */}
      <div className={styles.detectorSection}>
        <div className={styles.sectionTitle}>
          {t('analysis.rhythm.title')}
          <InfoTip label={t('analysis.rhythm.title')} text={t('analysis.rhythm.tip')} />
        </div>
        {rhythmRuns.length === 0 ? (
          <p className={styles.noDetectors} data-testid="rhythm-empty">—</p>
        ) : (
          <>
            <div className={styles.rhythmMeta}>
              {t('analysis.rhythm.meta', {
                g: rhythmRuns.length,
                p: rhythmRuns.filter((r) => r.status === 'passed').length,
              })}
            </div>
            <RhythmStrip
              runs={rhythmRuns}
              onRunClick={(i) => {
                const eid = rhythmRuns[i]?.eventId;
                if (eid) onSelectEvent?.(eid);
              }}
            />
            <div className={styles.rhythmAxis}>
              <span>0%</span>
              <span>25%</span>
              <span>50%</span>
              <span>75%</span>
              <span>100%</span>
            </div>
          </>
        )}
      </div>

      {/* --- Detector signal distribution --- */}
      <div className={styles.detectorSection}>
        <div className={styles.sectionTitle}>{t('analysis.detectorDistribution')}</div>
        {detectorEntries.length === 0 ? (
          <p className={styles.noDetectors}>{t('analysis.noSignals')}</p>
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
                              {signalLabel(s, t)}
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
